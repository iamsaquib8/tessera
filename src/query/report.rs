//! The pull-request report: `verify` findings, the callers of the changed
//! code, the contracts the change touches, and the tests to run, as one
//! Markdown comment.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use super::blast::{BlastArgs, BlastResult};
use super::map::{self, MapArgs};
use super::verify::{changed_paths, verify_paths, Changes, VerifyResult};

/// The marker a bot looks for to update its own comment.
pub const MARKER: &str = "<!-- tessera -->";
const MAX_CALLERS: usize = 10;
const MAX_ROWS: usize = 50;
/// GitHub rejects comments over 65,536 characters.
const MAX_CHARS: usize = 60_000;

#[derive(Debug, Clone, Serialize)]
pub struct TouchedContract {
    pub kind: String,
    pub name: String,
    /// Services on the other side: the callers of a route or method this
    /// change serves, the providers of one it calls.
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub verify: VerifyResult,
    pub blast: Option<BlastResult>,
    pub contracts: Vec<TouchedContract>,
}

pub fn run(conn: &Connection, root: &Path, changes: &Changes) -> Result<Report> {
    let changed = changed_paths(root, changes)?;
    let verify = verify_paths(conn, root, &changed, changes)?;
    let rev = match changes {
        Changes::Since(rev) => rev.clone(),
        _ => "HEAD".to_string(),
    };
    let blast = super::blast(
        conn,
        root,
        &BlastArgs {
            diff_from: Some(rev),
            limit: MAX_CALLERS,
            ..BlastArgs::default()
        },
    )
    .ok();
    let contracts = touched(conn, root, &changed)?;
    Ok(Report {
        verify,
        blast,
        contracts,
    })
}

/// Contracts with a site in a changed file, and who is on the other side.
fn touched(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
) -> Result<Vec<TouchedContract>> {
    let map = map::run(conn, root, &MapArgs::default())?;
    // In a workspace, sites carry their repository's prefix (`web/src/…`).
    let prefix = crate::workspace::repo_prefix(root);
    let changed: HashSet<String> = changed.iter().map(|p| format!("{prefix}{p}")).collect();
    let changed = &changed;
    let mut out = Vec::new();
    for contract in map.contracts.iter().filter(|c| c.kind != "table") {
        let provides = contract.providers.iter().any(|s| changed.contains(&s.path));
        let uses = contract.consumers.iter().any(|s| changed.contains(&s.path));
        if !provides && !uses {
            continue;
        }
        let other = if provides {
            &contract.consumers
        } else {
            &contract.providers
        };
        let services: BTreeSet<String> = other
            .iter()
            .filter(|s| !changed.contains(&s.path))
            .map(|s| s.service.clone())
            .collect();
        out.push(TouchedContract {
            kind: contract.kind.clone(),
            name: contract.name.clone(),
            services: services.into_iter().collect(),
        });
    }
    Ok(out)
}

/// Text for a Markdown table cell or list item: `|` and `<` escaped, line
/// breaks flattened.
pub fn cell(text: &str) -> String {
    text.replace('\n', " ")
        .replace('|', "\\|")
        .replace('<', "&lt;")
}

/// `text` as inline code, with a fence long enough for any backticks in it.
pub fn code(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{}{pad}{fence}", text.replace('|', "\\|"))
}

impl Report {
    pub fn markdown(&self) -> String {
        let mut out = format!("{MARKER}\n");
        let blocking: Vec<_> = self.verify.findings.iter().filter(|f| f.blocking).collect();
        let advisory: Vec<_> = self
            .verify
            .findings
            .iter()
            .filter(|f| !f.blocking && f.id != "V6")
            .collect();
        let headline = match blocking.len() {
            0 => "### Tessera: No problems found".to_string(),
            1 => "### Tessera: 1 problem to fix".to_string(),
            n => format!("### Tessera: {n} problems to fix"),
        };
        out.push_str(&headline);
        out.push_str("\n\n");
        let changed = self.blast.as_ref().map_or(0, |b| b.changed.len());
        let callers = self.blast.as_ref().map_or(0, |b| b.page.total);
        out.push_str(&format!(
            "{changed} changed symbol(s), {callers} caller(s), {} test file(s) to run, {} contract(s) touched.\n\n",
            self.verify.tests.len(),
            self.contracts.len()
        ));
        if !blocking.is_empty() {
            out.push_str("| Check | Where | Problem |\n| --- | --- | --- |\n");
            for finding in blocking.iter().take(MAX_ROWS) {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    finding.id,
                    code(&location(finding.path.as_deref(), finding.line)),
                    cell(&finding.message)
                ));
            }
            if blocking.len() > MAX_ROWS {
                out.push_str(&format!("\n…and {} more.\n", blocking.len() - MAX_ROWS));
            }
            out.push('\n');
        }
        if !advisory.is_empty() {
            out.push_str(&format!(
                "<details><summary>Advisory ({})</summary>\n\n",
                advisory.len()
            ));
            for finding in advisory.iter().take(MAX_ROWS) {
                out.push_str(&format!(
                    "- {} {} {}\n",
                    finding.id,
                    code(&location(finding.path.as_deref(), finding.line)),
                    cell(&finding.message)
                ));
            }
            out.push_str("\n</details>\n\n");
        }
        if !self.contracts.is_empty() {
            out.push_str("**Contracts this change touches**\n\n");
            for contract in self.contracts.iter().take(MAX_ROWS) {
                let other = if contract.services.is_empty() {
                    String::new()
                } else {
                    format!(
                        ": also used by {}",
                        contract
                            .services
                            .iter()
                            .map(|s| cell(s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                out.push_str(&format!(
                    "- {}{other}\n",
                    code(&format!("{} {}", contract.kind, contract.name))
                ));
            }
            out.push('\n');
        }
        if let Some(blast) = self.blast.as_ref().filter(|b| !b.callers.is_empty()) {
            out.push_str(&format!(
                "<details><summary>Callers of the changed code (top {} of {})</summary>\n\n| Caller | Where | Distance |\n| --- | --- | --- |\n",
                blast.callers.len(),
                blast.page.total
            ));
            for caller in &blast.callers {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    code(&caller.symbol.qualified_name),
                    code(&format!(
                        "{}:{}",
                        caller.symbol.path, caller.symbol.start_line
                    )),
                    caller.depth
                ));
            }
            out.push_str("\n</details>\n\n");
        }
        if !self.verify.tests.is_empty() {
            out.push_str(&format!(
                "<details><summary>Tests to run ({})</summary>\n\n",
                self.verify.tests.len()
            ));
            for test in self.verify.tests.iter().take(MAX_ROWS) {
                out.push_str(&format!("- {}\n", code(test)));
            }
            out.push_str("\n</details>\n\n");
        }
        out.push_str("<sub>From [Tessera](https://github.com/iamsaquib8/tessera): a local, deterministic code graph. Only high-confidence findings block.</sub>\n");
        if out.len() > MAX_CHARS {
            let mut cut = MAX_CHARS;
            while !out.is_char_boundary(cut) {
                cut -= 1;
            }
            out.truncate(cut);
            out.push_str("\n\n…report truncated.\n");
        }
        out
    }
}

fn location(path: Option<&str>, line: Option<usize>) -> String {
    match (path, line) {
        (Some(path), Some(line)) => format!("{path}:{line}"),
        (Some(path), None) => path.to_string(),
        _ => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_and_code_escape_markdown() {
        assert_eq!(cell("a | b <T>\nc"), "a \\| b &lt;T> c");
        assert_eq!(code("x"), "`x`");
        assert_eq!(code("a`b"), "``a`b``");
        assert_eq!(code("`a`"), "`` `a` ``");
        assert_eq!(code("a|b"), "`a\\|b`");
    }
}
