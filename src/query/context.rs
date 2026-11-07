use std::fmt;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::blast::confidence_mark;
use super::common::read_body;
use super::{graph, legacy};
use crate::db;
use crate::output::{estimate_tokens, AgentText, Confidence};
use crate::types::SymbolRecord;

const DEFAULT_BUDGET: usize = 2_000;
const MAX_SIBLINGS: usize = 5;
/// How far up the call graph to look for tests that exercise the symbol, and
/// how many symbols that walk may visit.
const TEST_DEPTH: usize = 4;
const TEST_SCAN_CAP: usize = 2_000;
/// The budget trims these further; the cap keeps hub symbols cheap.
const MAX_NEIGHBOURS: usize = 60;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextArgs {
    pub symbol: String,
    pub budget_tokens: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextResult {
    pub symbol: Option<SymbolRecord>,
    pub signature: String,
    pub body: Option<String>,
    pub dependencies: Vec<String>,
    pub callers: Vec<String>,
    pub tests: Vec<String>,
    pub siblings: Vec<String>,
    pub tokens: usize,
}

pub fn run(conn: &Connection, root: &Path, args: &ContextArgs) -> Result<ContextResult> {
    let budget = if args.budget_tokens == 0 {
        DEFAULT_BUDGET
    } else {
        args.budget_tokens
    };
    let Some(symbol) = db::resolve_symbol(conn, &args.symbol)? else {
        return Ok(ContextResult {
            symbol: None,
            signature: String::new(),
            body: None,
            dependencies: Vec::new(),
            callers: Vec::new(),
            tests: Vec::new(),
            siblings: Vec::new(),
            tokens: 0,
        });
    };
    let body = read_body(root, &symbol).ok();
    let line = |(s, confidence): &(SymbolRecord, Confidence)| {
        format!(
            "{}  {}:{}  {}{}",
            s.qualified_name,
            s.path,
            s.start_line,
            s.signature,
            confidence_mark(*confidence)
        )
    };
    let tests = graph::tests_reaching(conn, symbol.id, TEST_DEPTH, TEST_SCAN_CAP)?
        .into_iter()
        .map(|t| format!("{} ({}:{})", t.qualified_name, t.path, t.start_line))
        .collect();
    let mut result = ContextResult {
        signature: symbol.signature.clone(),
        body,
        dependencies: graph::callees(conn, symbol.id, MAX_NEIGHBOURS)?
            .iter()
            .map(line)
            .collect(),
        callers: graph::callers(conn, symbol.id, MAX_NEIGHBOURS)?
            .iter()
            .map(line)
            .collect(),
        tests,
        siblings: legacy::siblings_conn(conn, &symbol.qualified_name)?
            .siblings
            .into_iter()
            .take(MAX_SIBLINGS)
            .map(|s| s.qualified_name)
            .collect(),
        symbol: Some(symbol),
        tokens: 0,
    };
    fit_to_budget(&mut result, budget);
    Ok(result)
}

fn fit_to_budget(result: &mut ContextResult, budget: usize) {
    // Render once and keep a running character count: each item removed takes
    // its line (`"  "`, the item, `"\n"`) off the text, and emptying a section
    // takes its header off too. Re-rendering after every removed item would
    // cost the whole text each time, which is quadratic in the number of
    // tests and callers.
    let mut chars = result.agent_text().chars().count();
    while chars.div_ceil(4) > budget {
        let trimmed = pop_item(&mut result.siblings, SIBLINGS_TITLE, &mut chars)
            || pop_item(&mut result.tests, TESTS_TITLE, &mut chars)
            || pop_item(&mut result.dependencies, CALLS_TITLE, &mut chars)
            || pop_item(&mut result.callers, CALLED_BY_TITLE, &mut chars);
        if trimmed {
            continue;
        }
        if let Some(body) = &mut result.body {
            let keep = budget.saturating_mul(4) / 2;
            if body.chars().count() > keep {
                *body = body.chars().take(keep).collect::<String>()
                    + "\n… (body truncated to fit budget)";
            }
        }
        // Nothing is left to remove: the body was the last thing to cut.
        break;
    }
    result.tokens = estimate_tokens(&result.agent_text());
}

/// Removes the last item of a section and takes its lines off `chars`.
fn pop_item(items: &mut Vec<String>, title: &str, chars: &mut usize) -> bool {
    let Some(item) = items.pop() else {
        return false;
    };
    *chars -= item.chars().count() + 3;
    if items.is_empty() {
        // `"{title}:\n"`
        *chars -= title.chars().count() + 2;
    }
    true
}

const CALLS_TITLE: &str = "Calls";
const CALLED_BY_TITLE: &str = "Called by";
const TESTS_TITLE: &str = "Tests";
const SIBLINGS_TITLE: &str = "Related (shared callers)";

impl fmt::Display for ContextResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for ContextResult {
    fn agent_text(&self) -> String {
        let Some(symbol) = &self.symbol else {
            return "Symbol not found. Try `tessera search <name>`.\n".to_string();
        };
        let mut out = format!(
            "{} {} {}:{}-{}\n",
            symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line, symbol.end_line
        );
        if let Some(body) = &self.body {
            out.push_str(body);
            out.push('\n');
        }
        let mut section = |title: &str, items: &[String]| {
            if !items.is_empty() {
                out.push_str(&format!("{title}:\n"));
                for item in items {
                    out.push_str(&format!("  {item}\n"));
                }
            }
        };
        section(CALLS_TITLE, &self.dependencies);
        section(CALLED_BY_TITLE, &self.callers);
        section(TESTS_TITLE, &self.tests);
        section(SIBLINGS_TITLE, &self.siblings);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loop that re-renders the whole text after every removed item.
    fn fit_by_rendering(result: &mut ContextResult, budget: usize) {
        loop {
            result.tokens = estimate_tokens(&result.agent_text());
            if result.tokens <= budget {
                return;
            }
            let trimmed = result.siblings.pop().is_some()
                || result.tests.pop().is_some()
                || result.dependencies.pop().is_some()
                || result.callers.pop().is_some();
            if trimmed {
                continue;
            }
            if let Some(body) = &mut result.body {
                let keep = budget.saturating_mul(4) / 2;
                if body.chars().count() > keep {
                    *body = body.chars().take(keep).collect::<String>()
                        + "\n… (body truncated to fit budget)";
                    continue;
                }
            }
            return;
        }
    }

    fn bundle(body: Option<String>, neighbours: usize, tests: usize) -> ContextResult {
        let list = |prefix: &str, n: usize| -> Vec<String> {
            (0..n)
                .map(|i| {
                    format!("{prefix}{i}  src/módulo/{prefix}{i}.ts:{i}  function {prefix}{i}()")
                })
                .collect()
        };
        ContextResult {
            symbol: Some(SymbolRecord {
                id: 1,
                name: "target".to_string(),
                qualified_name: "Svc.target".to_string(),
                kind: "method".to_string(),
                file_id: 1,
                path: "src/svc.ts".to_string(),
                language: "typescript".to_string(),
                start_line: 10,
                end_line: 40,
                signature: "target(a: number): number".to_string(),
                exported: true,
            }),
            signature: "target(a: number): number".to_string(),
            body,
            dependencies: list("callee", neighbours),
            callers: list("caller", neighbours),
            tests: list("test", tests),
            siblings: list("sibling", neighbours.min(5)),
            tokens: 0,
        }
    }

    #[test]
    fn a_budget_smaller_than_the_truncation_note_still_terminates() {
        // The truncated body plus its note can exceed a tiny budget; the old
        // loop cut the body again and again, forever.
        for budget in [1, 10, 40] {
            let mut result = bundle(Some("let x = 1;\n".repeat(400)), 3, 3);
            fit_to_budget(&mut result, budget);
            assert!(result.callers.is_empty() && result.tests.is_empty());
            let body = result.body.as_deref().unwrap();
            assert!(body.ends_with("(body truncated to fit budget)"));
            assert_eq!(result.tokens, estimate_tokens(&result.agent_text()));
        }
    }

    #[test]
    fn running_count_trims_exactly_as_rendering_every_time_did() {
        let bodies = [
            None,
            Some("fn short() {}".to_string()),
            Some("let x = 1;\n".repeat(400)),
            Some("ñandú ".repeat(900)),
        ];
        for body in &bodies {
            for (neighbours, tests) in [(0, 0), (3, 0), (60, 700), (60, 5)] {
                for budget in [60, 200, 500, 2_000, 8_000, 40_000] {
                    let mut expected = bundle(body.clone(), neighbours, tests);
                    let mut got = expected.clone();
                    fit_by_rendering(&mut expected, budget);
                    fit_to_budget(&mut got, budget);
                    assert_eq!(
                        got.agent_text(),
                        expected.agent_text(),
                        "budget {budget}, {neighbours} neighbours, {tests} tests"
                    );
                    assert_eq!(got.tokens, expected.tokens);
                    assert_eq!(got.tests, expected.tests);
                    assert_eq!(got.callers, expected.callers);
                    assert_eq!(got.dependencies, expected.dependencies);
                    assert_eq!(got.siblings, expected.siblings);
                    assert_eq!(got.body, expected.body);
                }
            }
        }
    }
}
