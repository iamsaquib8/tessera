use std::fmt;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::{is_test_path, limit_or_default, TYPE_KINDS};
use super::find::page_footer;
use super::{graph, legacy};
use crate::db;
use crate::output::{paginate, AgentText, Page};
use crate::types::{SearchHit, SearchOptions};

const UNUSED_SCAN_LIMIT: usize = 500;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchArgs {
    pub pattern: String,
    pub kinds: Vec<String>,
    pub languages: Vec<String>,
    pub path: Option<String>,
    pub exported: Option<bool>,
    pub unused: bool,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub pattern: String,
    pub hits: Vec<SearchHit>,
    pub note: Option<String>,
    pub page: Page,
}

pub fn run(conn: &Connection, args: &SearchArgs) -> Result<SearchResults> {
    let limit = limit_or_default(args.limit);
    let (hits, note) = if args.unused {
        (
            unused(conn, args)?,
            Some(
                "Types are checked through type references in TypeScript, JavaScript and Java; elsewhere only functions and methods are. Interface and abstract members are skipped.".to_string(),
            ),
        )
    } else {
        let result = legacy::search_conn(
            conn,
            &args.pattern,
            SearchOptions {
                kinds: args.kinds.clone(),
                languages: args.languages.clone(),
                exported: args.exported,
                path_prefix: args.path.clone(),
                limit: legacy::SEARCH_SCAN_LIMIT,
            },
        )?;
        (result.hits, None)
    };
    let (hits, page) = paginate(hits, limit, args.cursor.as_deref())?;
    Ok(SearchResults {
        pattern: args.pattern.clone(),
        hits,
        note,
        page,
    })
}

impl fmt::Display for SearchResults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for SearchResults {
    fn agent_text(&self) -> String {
        let mut out = String::new();
        if self.hits.is_empty() {
            out.push_str(&format!("No symbols match {}.\n", self.pattern));
        }
        for hit in &self.hits {
            let s = &hit.symbol;
            out.push_str(&format!(
                "{} {} {}:{}\n",
                s.kind, s.qualified_name, s.path, s.start_line
            ));
        }
        if let Some(note) = &self.note {
            out.push_str(&format!("note: {note}\n"));
        }
        out.push_str(&page_footer(&self.page));
        out
    }
}

/// Languages whose type uses are indexed as references, so an unreferenced
/// type there really is unused.
const TYPE_REFERENCE_LANGUAGES: &[&str] = &["typescript", "javascript", "java"];

/// Symbols nothing resolves to and no unresolved reference names.
fn unused(conn: &Connection, args: &SearchArgs) -> Result<Vec<SearchHit>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                s.start_line, s.end_line, s.signature, s.exported, p.kind
         FROM symbols s JOIN files f ON f.id = s.file_id
         LEFT JOIN symbols p ON p.file_id = s.file_id AND p.qualified_name = s.container
         ORDER BY s.qualified_name, s.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((db::map_symbol(row)?, row.get::<_, Option<String>>(11)?))
    })?;
    let pattern = args.pattern.trim_matches('*');
    let mut hits = Vec::new();
    for row in rows {
        let (symbol, owner_kind) = row?;
        let is_type = TYPE_KINDS.contains(&symbol.kind.as_str());
        let kind_wanted = if args.kinds.is_empty() {
            matches!(symbol.kind.as_str(), "function" | "method")
                || (is_type && TYPE_REFERENCE_LANGUAGES.contains(&symbol.language.as_str()))
        } else {
            args.kinds.contains(&symbol.kind)
        };
        let abstract_member = owner_kind.as_deref() == Some("interface")
            || symbol.signature.trim_start().starts_with("abstract ");
        if !kind_wanted
            || abstract_member
            || is_test_path(&symbol.path)
            || (!pattern.is_empty() && !symbol.name.contains(pattern))
            || (!args.languages.is_empty() && !args.languages.contains(&symbol.language))
            || args
                .exported
                .is_some_and(|exported| symbol.exported != exported)
            || args
                .path
                .as_deref()
                .is_some_and(|prefix| !symbol.path.starts_with(prefix))
        {
            continue;
        }
        if !graph::is_referenced(conn, &symbol)? {
            hits.push(SearchHit { symbol, score: 0.0 });
            if hits.len() >= UNUSED_SCAN_LIMIT {
                break;
            }
        }
    }
    Ok(hits)
}
