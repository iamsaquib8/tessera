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

/// Appended to a body that was cut to fit the budget.
const BODY_NOTE: &str = "\n… (body truncated to fit budget)";
/// The most of the budget the body keeps when the lists need room; a shorter
/// body keeps all of itself.
const BODY_SHARE_PERCENT: usize = 55;
/// How the room left by the body is split between callers, tests, callees and
/// siblings, with unused room passed on to the next list. Callers come first:
/// they say what an edit can break.
const LIST_SHARES: [usize; 4] = [40, 20, 30, 10];
const CALLERS: usize = 0;
const TESTS: usize = 1;

/// Cuts the bundle to `budget` tokens. Everything stays when it fits. Else the
/// body keeps up to its share, the lists split the rest (at least the first
/// caller and the first test when they fit), and the body takes what the lists
/// leave, so the result fills the budget instead of stopping short of it.
/// Renders twice, however many items there are.
fn fit_to_budget(result: &mut ContextResult, budget: usize) {
    let cap = budget.saturating_mul(4);
    let total = result.agent_text().chars().count();
    if total.div_ceil(4) <= budget {
        result.tokens = total.div_ceil(4);
        return;
    }
    // Sizes in characters, from the rendering rules of `agent_text`: a line
    // is `"  "`, the item and `"\n"`; a section adds `"{title}:\n"`.
    let titles = [CALLED_BY_TITLE, TESTS_TITLE, CALLS_TITLE, SIBLINGS_TITLE];
    let sizes: [Vec<usize>; 4] = [
        &result.callers,
        &result.tests,
        &result.dependencies,
        &result.siblings,
    ]
    .map(|items| items.iter().map(|i| i.chars().count() + 3).collect());
    let body_chars = result.body.as_ref().map_or(0, |b| b.chars().count() + 1);
    let lists_chars: usize = (0..4)
        .map(|k| section_chars(titles[k], &sizes[k], sizes[k].len()))
        .sum();
    let header = total - body_chars - lists_chars;

    let body_floor = body_chars.min((cap * BODY_SHARE_PERCENT / 100).saturating_sub(header));
    let kept = share_room(&titles, &sizes, cap.saturating_sub(header + body_floor));
    result.callers.truncate(kept[0]);
    result.tests.truncate(kept[1]);
    result.dependencies.truncate(kept[2]);
    result.siblings.truncate(kept[3]);

    let lists_kept: usize = (0..4)
        .map(|k| section_chars(titles[k], &sizes[k], kept[k]))
        .sum();
    let body_room = cap.saturating_sub(header + lists_kept);
    if let Some(body) = &mut result.body {
        if body_chars > body_room {
            let keep = body_room.saturating_sub(1 + BODY_NOTE.chars().count());
            *body = body.chars().take(keep).collect::<String>() + BODY_NOTE;
        }
    }
    result.tokens = estimate_tokens(&result.agent_text());
}

/// Characters the first `kept` items of a section take, header included.
fn section_chars(title: &str, sizes: &[usize], kept: usize) -> usize {
    if kept == 0 {
        0
    } else {
        title.chars().count() + 2 + sizes[..kept].iter().sum::<usize>()
    }
}

/// How many items of each list (callers, tests, callees, siblings) fit in
/// `room` characters: the first caller and the first test, then each list's
/// share in order, then the leftover room in the same order.
fn share_room(titles: &[&str; 4], sizes: &[Vec<usize>; 4], room: usize) -> [usize; 4] {
    let mut kept = [0usize; 4];
    let mut used = [0usize; 4];
    let mut spent = 0usize;
    let cost = |k: usize, i: usize| {
        sizes[k][i]
            + if i == 0 {
                titles[k].chars().count() + 2
            } else {
                0
            }
    };
    for k in [CALLERS, TESTS] {
        if !sizes[k].is_empty() && spent + cost(k, 0) <= room {
            kept[k] = 1;
            used[k] = cost(k, 0);
            spent += used[k];
        }
    }
    let mut carry = 0usize;
    for k in 0..4 {
        let allowance = room * LIST_SHARES[k] / 100 + carry;
        while kept[k] < sizes[k].len() {
            let c = cost(k, kept[k]);
            if used[k] + c > allowance || spent + c > room {
                break;
            }
            kept[k] += 1;
            used[k] += c;
            spent += c;
        }
        carry = allowance.saturating_sub(used[k]);
    }
    for k in 0..4 {
        while kept[k] < sizes[k].len() {
            let c = cost(k, kept[k]);
            if spent + c > room {
                break;
            }
            kept[k] += 1;
            spent += c;
        }
    }
    kept
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

    fn long_body() -> Option<String> {
        Some("    let value = compute_something(first, second, third);\n".repeat(100))
    }

    #[test]
    fn a_bundle_that_fits_is_left_alone() {
        let mut result = bundle(Some("fn short() {}".to_string()), 3, 2);
        let before = result.agent_text();
        fit_to_budget(&mut result, 100_000);
        assert_eq!(result.agent_text(), before);
        assert_eq!(result.tokens, estimate_tokens(&before));
    }

    #[test]
    fn a_long_body_keeps_callers_tests_and_most_of_the_budget() {
        // A 100-line body, 2 callers and 2 tests: before, the budget below
        // dropped every caller and test, then cut the body to half the budget.
        for budget in [400, 800, 1_500] {
            let mut result = bundle(long_body(), 2, 2);
            fit_to_budget(&mut result, budget);
            assert!(!result.callers.is_empty(), "budget {budget}: no callers");
            assert!(!result.tests.is_empty(), "budget {budget}: no tests");
            assert!(
                result.tokens <= budget,
                "budget {budget}: {}",
                result.tokens
            );
            assert!(
                result.tokens * 100 >= budget * 90,
                "budget {budget} left unused: {}",
                result.tokens
            );
            assert_eq!(result.tokens, estimate_tokens(&result.agent_text()));
        }
    }

    #[test]
    fn a_body_that_fits_stays_whole_while_the_lists_are_trimmed() {
        let mut result = bundle(Some("fn small() {}".to_string()), 60, 700);
        fit_to_budget(&mut result, 800);
        assert_eq!(result.body.as_deref(), Some("fn small() {}"));
        assert!(!result.callers.is_empty() && !result.tests.is_empty());
        assert!(result.callers.len() < 60 && result.tests.len() < 700);
        assert!(result.tokens <= 800 && result.tokens >= 720);
    }

    #[test]
    fn lists_share_the_room_in_order() {
        let mut result = bundle(long_body(), 60, 700);
        fit_to_budget(&mut result, 2_000);
        // Every list is represented, and callers get the largest share.
        assert!(!result.callers.is_empty());
        assert!(!result.tests.is_empty());
        assert!(!result.dependencies.is_empty());
        assert!(result.callers.len() >= result.tests.len());
        assert!(result.tokens <= 2_000);
    }

    #[test]
    fn every_budget_gives_a_bundle_within_it_or_the_smallest_one() {
        let bodies = [
            None,
            Some("fn short() {}".to_string()),
            long_body(),
            Some("ñandú ".repeat(900)),
        ];
        for body in &bodies {
            for (neighbours, tests) in [(0, 0), (3, 0), (60, 700), (60, 5)] {
                for budget in [1, 10, 40, 100, 200, 500, 2_000, 8_000] {
                    let mut result = bundle(body.clone(), neighbours, tests);
                    fit_to_budget(&mut result, budget);
                    assert_eq!(result.tokens, estimate_tokens(&result.agent_text()));
                    // From 200 tokens up, the header, one caller, one test and
                    // the note always fit.
                    if budget >= 200 {
                        assert!(
                            result.tokens <= budget,
                            "budget {budget}: {}",
                            result.tokens
                        );
                    }
                    if budget >= 500 && neighbours > 0 {
                        assert!(!result.callers.is_empty(), "budget {budget}");
                    }
                    if budget >= 500 && tests > 0 {
                        assert!(!result.tests.is_empty(), "budget {budget}");
                    }
                }
            }
        }
    }
}
