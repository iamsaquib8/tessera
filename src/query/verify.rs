use std::fmt;

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::known_global;
use super::legacy;
use crate::extract::parse_file;
use crate::output::{AgentText, Confidence};
use crate::types::Receiver;
use crate::types::{Language, SymbolSuggestion};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerifyArgs {
    pub symbol: Option<String>,
    pub snippet: Option<String>,
    #[serde(deserialize_with = "language_by_name")]
    pub language: Option<Language>,
}

fn language_by_name<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Language>, D::Error> {
    let name: Option<String> = Option::deserialize(deserializer)?;
    name.map(|n| {
        Language::from_name(&n)
            .ok_or_else(|| serde::de::Error::custom(format!("unsupported language `{n}`")))
    })
    .transpose()
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: String,
    pub confidence: Confidence,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub suggestions: Vec<SymbolSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub ok: bool,
    pub findings: Vec<Finding>,
}

pub fn run(conn: &Connection, args: &VerifyArgs) -> Result<VerifyResult> {
    let mut findings = Vec::new();
    if let Some(symbol) = &args.symbol {
        let check = legacy::validate_conn(conn, symbol)?;
        if !check.exists {
            findings.push(unresolved(symbol, None, check.candidates));
        }
    }
    if let Some(snippet) = &args.snippet {
        let Some(language) = args.language else {
            bail!("--language is required with --snippet");
        };
        let parsed = parse_file(language, snippet)?;
        for reference in parsed.references {
            let name = &reference.symbol_name;
            let checkable = match reference.receiver {
                Receiver::None => !known_global(language, name),
                Receiver::Name => {
                    let receiver = reference.receiver_name.as_deref().unwrap_or_default();
                    // Only a static call on a class the index knows is checkable;
                    // a variable's type is unknown in a snippet.
                    !receiver.contains('.')
                        && !known_global(language, receiver)
                        && receiver.chars().next().is_some_and(char::is_uppercase)
                        && legacy::validate_conn(conn, receiver)?.exists
                }
                Receiver::This | Receiver::Super | Receiver::Other => false,
            };
            if !checkable {
                continue;
            }
            let check = legacy::validate_conn(conn, name)?;
            if !check.exists {
                findings.push(unresolved(name, Some(reference.line), check.candidates));
            }
        }
    }
    if args.symbol.is_none() && args.snippet.is_none() {
        bail!("pass a symbol, or --snippet with --language");
    }
    Ok(VerifyResult {
        ok: findings.is_empty(),
        findings,
    })
}

fn unresolved(name: &str, line: Option<usize>, suggestions: Vec<SymbolSuggestion>) -> Finding {
    let hint = suggestions
        .first()
        .map(|s| format!("; did you mean {}?", s.qualified_name))
        .unwrap_or_default();
    Finding {
        id: "V1".to_string(),
        confidence: Confidence::Name,
        message: format!("`{name}` is not defined anywhere in the index{hint}"),
        path: None,
        line,
        suggestions,
    }
}

impl fmt::Display for VerifyResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for VerifyResult {
    fn agent_text(&self) -> String {
        if self.ok {
            return "✓ verify: no problems found.\n".to_string();
        }
        let mut out = format!("✗ verify: {} problem(s)\n", self.findings.len());
        for finding in &self.findings {
            let at = finding
                .line
                .map(|line| format!(" (line {line})"))
                .unwrap_or_default();
            out.push_str(&format!("  [{}] {}{at}\n", finding.id, finding.message));
        }
        out
    }
}
