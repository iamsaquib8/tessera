use std::fmt;
use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::{limit_or_default, read_body, CONTAINER_KINDS};
use super::legacy;
use crate::output::{paginate, AgentText, MatchKind, Page};
use crate::types::{ImportRecord, SymbolRecord};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FindArgs {
    pub symbol: Option<String>,
    pub path: Option<String>,
    pub include_body: bool,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindMatch {
    pub symbol: SymbolRecord,
    pub match_kind: MatchKind,
    pub body: Option<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindResult {
    pub query: String,
    pub matches: Vec<FindMatch>,
    pub outline: Vec<SymbolRecord>,
    pub imports: Vec<ImportRecord>,
    pub page: Page,
}

pub fn run(conn: &Connection, root: &Path, args: &FindArgs) -> Result<FindResult> {
    let limit = limit_or_default(args.limit);
    if let Some(symbol) = &args.symbol {
        let definitions = legacy::find_definition_conn(conn, symbol)?;
        let match_kind = definitions.match_kind;
        let (page_items, page) = paginate(definitions.matches, limit, args.cursor.as_deref())?;
        let matches = page_items
            .into_iter()
            .map(|symbol| {
                let body = args
                    .include_body
                    .then(|| read_body(root, &symbol).ok())
                    .flatten();
                let members = if CONTAINER_KINDS.contains(&symbol.kind.as_str()) {
                    legacy::signature_conn(conn, &symbol.qualified_name)?
                        .members
                        .into_iter()
                        .map(|m| m.signature)
                        .collect()
                } else {
                    Vec::new()
                };
                Ok(FindMatch {
                    symbol,
                    match_kind,
                    body,
                    members,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(FindResult {
            query: symbol.clone(),
            matches,
            outline: Vec::new(),
            imports: Vec::new(),
            page,
        });
    }
    if let Some(path) = &args.path {
        let outline = legacy::get_outline_conn(conn, Path::new(path))?.symbols;
        let imports = legacy::imports_conn(conn, path)?.imports;
        let (outline, page) = paginate(outline, limit, args.cursor.as_deref())?;
        return Ok(FindResult {
            query: path.clone(),
            matches: Vec::new(),
            outline,
            imports,
            page,
        });
    }
    bail!("pass a symbol name, or --path for a file or directory outline")
}

impl fmt::Display for FindResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for FindResult {
    fn agent_text(&self) -> String {
        let mut out = String::new();
        if self.matches.is_empty() && self.outline.is_empty() {
            out.push_str(&format!(
                "No definition or outline found for {}.\nhint: try `tessera search {}` or check the path.\n",
                self.query, self.query
            ));
            return out;
        }
        if matches!(self.matches.first(), Some(m) if matches!(m.match_kind, MatchKind::Fuzzy { .. }))
        {
            out.push_str(&format!(
                "No exact definition of {}; closest matches:\n",
                self.query
            ));
        }
        for m in &self.matches {
            let s = &m.symbol;
            out.push_str(&format!(
                "{} {} {}:{}-{}\n",
                s.kind, s.qualified_name, s.path, s.start_line, s.end_line
            ));
            if m.body.is_none() {
                out.push_str(&format!("  {}\n", s.signature));
            }
            for member in &m.members {
                out.push_str(&format!("    {member}\n"));
            }
            if let Some(body) = &m.body {
                out.push_str(body);
                out.push('\n');
            }
        }
        if !self.outline.is_empty() {
            out.push_str(&format!("Outline of {}\n", self.query));
            for s in &self.outline {
                out.push_str(&format!(
                    "  {} {} {}:{}\n",
                    s.kind, s.qualified_name, s.path, s.start_line
                ));
            }
        }
        for import in &self.imports {
            out.push_str(&format!(
                "  import {} ({}:{})\n",
                import.source, import.from_path, import.line
            ));
        }
        out.push_str(&page_footer(&self.page));
        out
    }
}

pub(super) fn page_footer(page: &Page) -> String {
    match &page.next_cursor {
        Some(cursor) => format!(
            "[{} of {} shown; more with cursor {cursor}]\n",
            page.returned, page.total
        ),
        None => String::new(),
    }
}
