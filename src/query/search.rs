use std::fmt;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::{is_test_path, limit_or_default, TYPE_KINDS};
use super::find::page_footer;
use super::legacy;
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
    let (mut hits, mut note) = if args.unused {
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
                limit: legacy::SEARCH_SCAN_LIMIT + 1,
            },
        )?;
        (result.hits, None)
    };
    // The scans stop at a fixed number of matches. A list that reaches it is
    // not the whole result, and the page must not claim it is.
    let scan_limit = if args.unused {
        UNUSED_SCAN_LIMIT
    } else {
        legacy::SEARCH_SCAN_LIMIT
    };
    let capped = hits.len() > scan_limit;
    hits.truncate(scan_limit);
    let (hits, mut page) = paginate(hits, limit, args.cursor.as_deref())?;
    if capped {
        page.truncated = true;
        let cap_note = format!(
            "Stopped at {scan_limit} matches, so more exist. Narrow the pattern, or add --kind, --language or --path, to see the rest."
        );
        note = Some(match note {
            Some(note) => format!("{cap_note} {note}"),
            None => cap_note,
        });
    }
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
    let mut sql = String::from(
        "SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                s.start_line, s.end_line, s.signature, s.exported, p.kind
         FROM symbols s JOIN files f ON f.id = s.file_id
         LEFT JOIN symbols p ON p.file_id = s.file_id AND p.qualified_name = s.container
         WHERE NOT EXISTS (SELECT 1 FROM edges e WHERE e.to_symbol_id = s.id)
           AND NOT EXISTS (
               SELECT 1 FROM refs r
               WHERE r.symbol_name = s.name
                 AND EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id AND e.to_symbol_id IS NULL AND e.resolver IS NULL)
           )",
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(exported) = args.exported {
        sql.push_str(" AND s.exported = ?");
        params_vec.push(Box::new(exported as i32));
    }
    if let Some(path) = &args.path {
        sql.push_str(" AND instr(f.path, ?) = 1");
        params_vec.push(Box::new(path.clone()));
    }
    let pattern = args.pattern.trim_matches('*');
    if !pattern.is_empty() {
        sql.push_str(" AND instr(s.name, ?) > 0");
        params_vec.push(Box::new(pattern.to_string()));
    }
    if !args.languages.is_empty() {
        let placeholders = std::iter::repeat_n("?", args.languages.len())
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" AND f.language IN ({placeholders})"));
        for lang in &args.languages {
            params_vec.push(Box::new(lang.clone()));
        }
    }
    if !args.kinds.is_empty() {
        let placeholders = std::iter::repeat_n("?", args.kinds.len())
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(" AND s.kind IN ({placeholders})"));
        for kind in &args.kinds {
            params_vec.push(Box::new(kind.clone()));
        }
    } else {
        let type_placeholders = std::iter::repeat_n("?", TYPE_KINDS.len())
            .collect::<Vec<_>>()
            .join(",");
        let lang_placeholders = std::iter::repeat_n("?", TYPE_REFERENCE_LANGUAGES.len())
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(
            " AND (
                s.kind IN ('function', 'method')
                OR (s.kind IN ({type_placeholders}) AND f.language IN ({lang_placeholders}))
            )"
        ));
        for kind in TYPE_KINDS {
            params_vec.push(Box::new(kind.to_string()));
        }
        for lang in TYPE_REFERENCE_LANGUAGES {
            params_vec.push(Box::new(lang.to_string()));
        }
    }

    sql.push_str(" ORDER BY s.qualified_name, s.id");

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok((db::map_symbol(row)?, row.get::<_, Option<String>>(11)?))
    })?;

    let mut hits = Vec::new();
    for row in rows {
        let (symbol, owner_kind) = row?;
        let abstract_member = owner_kind.as_deref() == Some("interface")
            || symbol.signature.trim_start().starts_with("abstract ");
        if abstract_member || is_test_path(&symbol.path) {
            continue;
        }
        hits.push(SearchHit { symbol, score: 0.0 });
        if hits.len() > UNUSED_SCAN_LIMIT {
            break;
        }
    }
    Ok(hits)
}
