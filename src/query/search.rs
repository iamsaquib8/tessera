use std::fmt;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::{limit_or_default, TYPE_KINDS};
use super::find::page_footer;
use super::legacy;
use crate::output::{paginate, AgentText, Page};
use crate::types::{SearchHit, SearchOptions, UnusedOptions};

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
        let kinds = if args.kinds.is_empty() {
            ["function", "method", "constructor"]
                .iter()
                .map(|k| k.to_string())
                .collect()
        } else {
            args.kinds
                .iter()
                .filter(|k| !TYPE_KINDS.contains(&k.as_str()))
                .cloned()
                .collect()
        };
        let unused = legacy::unused_conn(
            conn,
            UnusedOptions {
                kinds,
                languages: args.languages.clone(),
                exported: args.exported,
                path_prefix: args.path.clone(),
                limit: UNUSED_SCAN_LIMIT,
            },
        )?;
        let hits = unused
            .symbols
            .into_iter()
            .filter(|u| {
                args.pattern.is_empty()
                    || args.pattern == "*"
                    || u.symbol.name.contains(args.pattern.trim_matches('*'))
            })
            .map(|u| SearchHit {
                symbol: u.symbol,
                score: 0.0,
            })
            .collect();
        (
            hits,
            Some(
                "Classes, structs, interfaces and types are not checked for use until type references are indexed (v0.11).".to_string(),
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
