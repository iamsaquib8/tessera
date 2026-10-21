use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::{is_test_path, limit_or_default};
use super::find::page_footer;
use super::{graph, legacy};
use crate::output::{paginate, AgentText, Confidence, Page};
use crate::types::SymbolRecord;

const DEFAULT_DEPTH: usize = 4;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlastArgs {
    pub symbol: Option<String>,
    pub diff_from: Option<String>,
    pub diff_to: Option<String>,
    pub depth: usize,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlastCaller {
    pub symbol: SymbolRecord,
    pub depth: usize,
    pub criticality: f32,
    /// The weakest edge on the path to the target.
    pub confidence: Confidence,
    pub via: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlastResult {
    pub target: String,
    pub changed: Vec<SymbolRecord>,
    pub callers: Vec<BlastCaller>,
    pub tests: Vec<String>,
    pub partial: bool,
    pub page: Page,
}

pub fn run(conn: &Connection, root: &Path, args: &BlastArgs) -> Result<BlastResult> {
    let depth = if args.depth == 0 {
        DEFAULT_DEPTH
    } else {
        args.depth
    };
    let limit = limit_or_default(args.limit);
    let (target, changed, callers, partial) = if let Some(symbol) = &args.symbol {
        let seeds: Vec<i64> = graph::definitions(conn, symbol)?
            .iter()
            .map(|s| s.id)
            .collect();
        let (reached, capped) = graph::blast(conn, &seeds, depth)?;
        let callers = reached
            .into_iter()
            .map(|c| BlastCaller {
                symbol: c.symbol,
                depth: c.depth,
                criticality: c.criticality,
                confidence: c.confidence,
                via: None,
            })
            .collect::<Vec<_>>();
        (symbol.clone(), Vec::new(), callers, capped)
    } else if let Some(from) = &args.diff_from {
        let changed = legacy::diff_changed_symbols(conn, root, from, args.diff_to.as_deref())?;
        let seeds: Vec<i64> = changed.1.iter().map(|s| s.id).collect();
        let (reached, capped) = graph::blast(conn, &seeds, depth)?;
        let callers = reached
            .into_iter()
            .map(|c| BlastCaller {
                via: changed
                    .1
                    .iter()
                    .find(|s| s.id == c.via)
                    .map(|s| s.qualified_name.clone()),
                symbol: c.symbol,
                depth: c.depth,
                criticality: c.criticality,
                confidence: c.confidence,
            })
            .collect();
        (changed.0, changed.1, callers, capped)
    } else {
        bail!("pass a symbol, or --diff <rev> for changed code");
    };
    let tests: BTreeSet<String> = callers
        .iter()
        .filter(|c| is_test_path(&c.symbol.path))
        .map(|c| c.symbol.path.clone())
        .collect();
    let (callers, page) = paginate(callers, limit, args.cursor.as_deref())?;
    Ok(BlastResult {
        target,
        changed,
        callers,
        tests: tests.into_iter().collect(),
        partial,
        page,
    })
}

impl fmt::Display for BlastResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for BlastResult {
    fn agent_text(&self) -> String {
        let mut out = String::new();
        if !self.changed.is_empty() {
            out.push_str(&format!("Changed symbols in {}:\n", self.target));
            for s in &self.changed {
                out.push_str(&format!(
                    "  {} {}:{}\n",
                    s.qualified_name, s.path, s.start_line
                ));
            }
        }
        if self.callers.is_empty() {
            out.push_str(&format!("No callers found for {}.\n", self.target));
        } else {
            out.push_str(&format!(
                "{} callers affected by {} (ranked{}):\n",
                self.page.total,
                self.target,
                confidence_note(self.callers.iter().map(|c| c.confidence))
            ));
            for c in &self.callers {
                out.push_str(&format!(
                    "  {:>5.1} d{} {}{} {}:{}{}\n",
                    c.criticality,
                    c.depth,
                    c.symbol.qualified_name,
                    confidence_mark(c.confidence),
                    c.symbol.path,
                    c.symbol.start_line,
                    c.via
                        .as_deref()
                        .map(|v| format!(" via {v}"))
                        .unwrap_or_default()
                ));
            }
        }
        if !self.tests.is_empty() {
            out.push_str(&format!("Tests to run: {}\n", self.tests.join(", ")));
        }
        if self.partial {
            out.push_str("note: traversal stopped at the frontier cap; results are partial.\n");
        }
        out.push_str(&page_footer(&self.page));
        out
    }
}

/// Marks a result line that is not an exact (high-confidence) match.
pub(crate) fn confidence_mark(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "",
        Confidence::Medium => " (likely)",
        Confidence::Low | Confidence::Name => " (possible)",
    }
}

/// Explains the marks once, and names name-level matching only when it occurred.
pub(crate) fn confidence_note(confidences: impl Iterator<Item = Confidence>) -> &'static str {
    let worst = confidences.max().unwrap_or(Confidence::High);
    match worst {
        Confidence::High => "; all exact",
        Confidence::Medium => "; \"likely\" means matched by a unique name",
        Confidence::Low => "; \"possible\" means one of several same-named symbols",
        Confidence::Name => "; \"possible\" includes name-level matching",
    }
}
