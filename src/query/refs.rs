use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::blast::{confidence_mark, confidence_note};
use super::common::{bare_name, is_test_path, limit_or_default};
use super::find::page_footer;
use super::graph;
use crate::extract::truncate_chars;
use crate::output::{paginate, AgentText, Confidence, Page};
use crate::types::ReferenceRecord;

const JS_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RefsArgs {
    pub target: String,
    pub tests_only: bool,
    /// Leave out references below this confidence; `None` keeps everything,
    /// including unresolved references by the same name.
    pub min_confidence: Option<Confidence>,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefHit {
    #[serde(flatten)]
    pub reference: ReferenceRecord,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefsResult {
    pub target: String,
    pub definitions: usize,
    pub references: Vec<RefHit>,
    pub importers: Vec<String>,
    pub page: Page,
}

pub fn run(conn: &Connection, args: &RefsArgs) -> Result<RefsResult> {
    let limit = limit_or_default(args.limit);
    let target = args
        .target
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/");
    if is_indexed_file(conn, &target)? {
        let importers = importers_of(conn, &target)?;
        let (importers, page) = paginate(importers, limit, args.cursor.as_deref())?;
        return Ok(RefsResult {
            target,
            definitions: 0,
            references: Vec::new(),
            importers,
            page,
        });
    }
    let name = bare_name(&target).to_string();
    let definitions = graph::definitions(conn, &target)?;
    let ids: Vec<i64> = definitions.iter().map(|d| d.id).collect();
    let mut references: Vec<RefHit> = graph::incoming_refs(conn, &ids, &name)?
        .into_iter()
        .map(|(reference, confidence)| RefHit {
            reference,
            confidence,
        })
        .collect();
    if args.tests_only {
        references.retain(|r| is_test_path(&r.reference.path));
    }
    if let Some(min) = args.min_confidence {
        references.retain(|r| r.confidence <= min);
    }
    let (references, page) = paginate(references, limit, args.cursor.as_deref())?;
    Ok(RefsResult {
        target,
        definitions: definitions.len(),
        references,
        importers: Vec::new(),
        page,
    })
}

fn is_indexed_file(conn: &Connection, path: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM files WHERE path = ?1", params![path], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}

fn importers_of(conn: &Connection, target: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT f.path, i.source FROM imports i JOIN files f ON f.id = i.file_id ORDER BY f.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut importers = BTreeSet::new();
    for row in rows {
        let (from, source) = row?;
        if from != target && import_points_at(&from, &source, target) {
            importers.insert(from);
        }
    }
    Ok(importers.into_iter().collect())
}

fn import_points_at(from: &str, source: &str, target: &str) -> bool {
    let target_stem = target.rsplit_once('.').map_or(target, |(stem, _)| stem);
    if source.starts_with('.') {
        let base = Path::new(from).parent().unwrap_or(Path::new(""));
        let joined = normalize(&base.join(source));
        let joined_str = joined.to_string_lossy().replace('\\', "/");
        return joined_str == target
            || joined_str == target_stem
            || JS_EXTENSIONS
                .iter()
                .any(|ext| format!("{joined_str}/index.{ext}") == target);
    }
    let dotted = source.replace('.', "/");
    target_stem == dotted || target_stem.ends_with(&format!("/{dotted}"))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

impl fmt::Display for RefsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for RefsResult {
    fn agent_text(&self) -> String {
        let mut out = String::new();
        if !self.importers.is_empty() {
            out.push_str(&format!("Files importing {}:\n", self.target));
            for importer in &self.importers {
                out.push_str(&format!("  {importer}\n"));
            }
        } else if self.references.is_empty() {
            out.push_str(&format!(
                "No references to {} ({} definitions).\n",
                self.target, self.definitions
            ));
        } else {
            out.push_str(&format!(
                "{} references to {} ({} definitions{}):\n",
                self.page.total,
                self.target,
                self.definitions,
                confidence_note(self.references.iter().map(|r| r.confidence))
            ));
            for hit in &self.references {
                let r = &hit.reference;
                out.push_str(&format!(
                    "  {}:{}:{} {}{}{}\n",
                    r.path,
                    r.line,
                    r.column,
                    truncate_chars(&r.context, 120),
                    r.from_symbol
                        .as_deref()
                        .map(|s| format!("  ← {s}"))
                        .unwrap_or_default(),
                    confidence_mark(hit.confidence)
                ));
            }
        }
        out.push_str(&page_footer(&self.page));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_dotted_imports() {
        assert!(import_points_at(
            "lib/email_worker.js",
            "./util",
            "lib/util.js"
        ));
        assert!(import_points_at(
            "specs/util.spec.js",
            "../lib/util",
            "lib/util.js"
        ));
        assert!(import_points_at(
            "src/a.ts",
            "./widgets",
            "src/widgets/index.ts"
        ));
        assert!(import_points_at(
            "app/service.py",
            "app.util",
            "app/util.py"
        ));
        assert!(!import_points_at(
            "app/service.py",
            "app.models",
            "app/util.py"
        ));
        assert!(!import_points_at("lib/a.js", "./utility", "lib/util.js"));
    }
}
