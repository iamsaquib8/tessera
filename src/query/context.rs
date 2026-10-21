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
        section("Calls", &self.dependencies);
        section("Called by", &self.callers);
        section("Tests", &self.tests);
        section("Related (shared callers)", &self.siblings);
        out
    }
}
