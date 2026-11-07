use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::blast::{confidence_mark, confidence_note};
use super::common::{bare_name, is_test_path, limit_or_default};
use super::find::page_footer;
use super::graph::{self, DefinitionSite};
use crate::extract::truncate_chars;
use crate::output::{paginate, AgentText, Confidence, Page};
use crate::resolve::modules::{ModuleResolver, ModuleTarget};
use crate::types::{Language, ReferenceRecord};

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
    /// The definition this reference resolved to; `None` for unresolved
    /// same-name references.
    pub definition: Option<DefinitionSite>,
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
    run_at(conn, None, args)
}

/// [`run`], resolving a file's importers with the module resolver as well
/// (tsconfig `paths`, workspace packages) when the repository root is known.
pub fn run_at(conn: &Connection, root: Option<&Path>, args: &RefsArgs) -> Result<RefsResult> {
    let limit = limit_or_default(args.limit);
    let target = args
        .target
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/");
    if is_indexed_file(conn, &target)? {
        let importers = importers_of(conn, root, &target)?;
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
        .map(|(reference, confidence, definition)| RefHit {
            reference,
            confidence,
            definition,
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

/// The files that import `target`: the ones the resolver linked
/// (`import_bindings.target_path`), the ones whose other imports it resolves
/// to `target`, and, as a last resort, a reading of the import string.
fn importers_of(conn: &Connection, root: Option<&Path>, target: &str) -> Result<Vec<String>> {
    let mut importers = BTreeSet::new();
    let mut linked = conn.prepare(
        "SELECT DISTINCT f.path FROM import_bindings b JOIN files f ON f.id = b.file_id
         WHERE b.target_path = ?1",
    )?;
    for row in linked.query_map(params![target], |row| row.get::<_, String>(0))? {
        importers.insert(row?);
    }
    let target_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![target],
            |r| r.get(0),
        )
        .optional()?;
    let resolver = match root {
        Some(root) => Some(ModuleResolver::build(conn, root)?),
        None => None,
    };
    let mut stmt = conn.prepare(
        "SELECT f.path, f.language, i.source FROM imports i JOIN files f ON f.id = i.file_id
         ORDER BY f.path",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (from, language, source) = row?;
        if from == target || importers.contains(&from) {
            continue;
        }
        let lang = Language::from_name(&language);
        let resolved = match (&resolver, lang, target_id) {
            (Some(resolver), Some(lang), Some(id)) => {
                resolver.resolve(&from, &source, lang) == ModuleTarget::File(id)
            }
            _ => false,
        };
        let js = matches!(
            lang,
            Some(Language::JavaScript | Language::TypeScript | Language::Tsx)
        );
        if resolved || import_points_at(&from, &source, target, js) {
            importers.insert(from);
        }
    }
    Ok(importers.into_iter().collect())
}

/// A reading of the import string alone: a relative specifier that names the
/// target (with or without extension, or as a directory's index), and, outside
/// JavaScript, a dotted module name that ends the target's path.
fn import_points_at(from: &str, source: &str, target: &str, js: bool) -> bool {
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
    if js {
        return false;
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
            "lib/util.js",
            true
        ));
        assert!(import_points_at(
            "specs/util.spec.js",
            "../lib/util",
            "lib/util.js",
            true
        ));
        assert!(import_points_at(
            "src/a.ts",
            "./widgets",
            "src/widgets/index.ts",
            true
        ));
        assert!(import_points_at(
            "app/service.py",
            "app.util",
            "app/util.py",
            false
        ));
        assert!(!import_points_at(
            "app/service.py",
            "app.models",
            "app/util.py",
            false
        ));
        assert!(!import_points_at(
            "lib/a.js",
            "./utility",
            "lib/util.js",
            true
        ));
        // a package name is not a file of that name elsewhere in a JS repo
        assert!(!import_points_at(
            "src/d.ts",
            "lodash",
            "src/lodash.js",
            true
        ));
    }
}
