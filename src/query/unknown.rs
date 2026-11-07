//! The answer for a symbol name the index does not know. Commands that walk
//! the graph from a name (`blast`, `refs`, `path`) must not report "nothing
//! found" for one: a misspelt name would read as "safe to change".

use anyhow::Result;
use rusqlite::Connection;

use super::legacy;
use crate::types::SymbolSuggestion;

/// Near matches offered for an unknown name.
const SUGGESTIONS: usize = 3;

/// The closest real symbols to `symbol`, best first.
pub fn suggest(conn: &Connection, symbol: &str) -> Result<Vec<SymbolSuggestion>> {
    legacy::fuzzy_candidates(conn, symbol, SUGGESTIONS)
}

/// The one message every command prints for an unknown symbol.
pub fn message(symbol: &str, suggestions: &[SymbolSuggestion]) -> String {
    if suggestions.is_empty() {
        return format!(
            "No definition of {symbol}.\nhint: try `tessera search {symbol}` or check the spelling.\n"
        );
    }
    let mut out = format!("No definition of {symbol}; closest matches:\n");
    for s in suggestions {
        out.push_str(&format!("  {} {}:{}\n", s.qualified_name, s.path, s.line));
    }
    out
}
