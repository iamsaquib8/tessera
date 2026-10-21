use std::fmt;

use anyhow::{bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::common::known_global;
use super::legacy;
use crate::extract::parse_file;
use crate::output::{AgentText, Confidence};
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
        let lines: Vec<&str> = snippet.lines().collect();
        for reference in parsed.references {
            let name = &reference.symbol_name;
            let line_text = lines.get(reference.line - 1).copied().unwrap_or_default();
            let receiver = receiver_of(line_text, reference.column, name);
            let checkable = match receiver {
                None => !known_global(language, name),
                Some(receiver) => {
                    !known_global(language, receiver)
                        && receiver.chars().next().is_some_and(char::is_uppercase)
                        && legacy::validate_conn(conn, receiver)?.exists
                }
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

// The call node starts at the receiver, so the text between it and the callee
// name tells member calls (`x.f()`, `X::f()`, `x->f()`) from bare calls.
// `column` is tree-sitter's 1-based byte column.
fn receiver_of<'a>(line: &'a str, column: usize, name: &str) -> Option<&'a str> {
    let rest = line.get(column.saturating_sub(1)..)?;
    let end = rest.find(name)?;
    let prefix = rest[..end].trim_end_matches(['.', ':', '-', '>', '?', '!']);
    if prefix.len() == rest[..end].len() {
        return None;
    }
    let receiver = prefix
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .next()
        .filter(|r| !r.is_empty())?;
    Some(receiver)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_detection() {
        assert_eq!(receiver_of("JSON.parse(x);", 1, "parse"), Some("JSON"));
        assert_eq!(
            receiver_of("  repo.findById(1)", 3, "findById"),
            Some("repo")
        );
        assert_eq!(receiver_of("Foo::bar()", 1, "bar"), Some("Foo"));
        assert_eq!(receiver_of("findByIdd(1)", 1, "findByIdd"), None);
        assert_eq!(receiver_of("a(); findByIdd(1)", 6, "findByIdd"), None);
    }
}
