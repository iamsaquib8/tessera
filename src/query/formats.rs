//! `verify` findings in the formats review tools read: GitLab Code Quality,
//! Azure DevOps logging commands, Bitbucket Code Insights, and NDJSON for
//! agent loops. SARIF is in `verify.rs`. Each format escapes paths and
//! messages for its own target.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::verify::{Finding, VerifyResult};

/// Bitbucket rejects an annotation summary longer than this.
const BITBUCKET_SUMMARY_CHARS: usize = 450;

/// A per-issue id for each finding: its fingerprint, with `-2`, `-3` … for
/// later occurrences of the same problem (one missing name called on two
/// lines), so a review tool keeps them apart. Findings are sorted, so the
/// suffixes are deterministic.
pub(super) fn occurrence_ids(findings: &[Finding]) -> Vec<String> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    findings
        .iter()
        .map(|finding| {
            let count = seen.entry(finding.fingerprint.as_str()).or_insert(0);
            *count += 1;
            if *count == 1 {
                finding.fingerprint.clone()
            } else {
                format!("{}-{count}", finding.fingerprint)
            }
        })
        .collect()
}

/// A repository-relative path as a relative URI reference: every byte but
/// an unreserved character or `/` percent-encoded (RFC 3986).
pub(super) fn uri(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The findings a review tool shows as issues: V6 lists a test to run,
/// not a problem.
fn issues(result: &VerifyResult) -> impl Iterator<Item = (&Finding, String)> {
    result
        .findings
        .iter()
        .zip(occurrence_ids(&result.findings))
        .filter(|(finding, _)| finding.id != "V6")
}

/// Azure DevOps logging-command escaping. Property values are split on
/// `;` and end at the first `]`, so both are escaped there. The message
/// after that `]` is never split: it escapes only line breaks and `%`
/// (as `%AZP25`), and a raw `;` or `]` in it reads the same whether or
/// not the agent decodes `%3B` and `%5D` there. Each character is
/// escaped once, so `%0A` in the text stays `%0A`.
fn azure_escape(value: &str, property: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => out.push_str("%AZP25"),
            '\r' => out.push_str("%0D"),
            '\n' => out.push_str("%0A"),
            ';' if property => out.push_str("%3B"),
            ']' if property => out.push_str("%5D"),
            c => out.push(c),
        }
    }
    out
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

impl VerifyResult {
    /// GitLab Code Quality: a JSON array of issues (the `codequality`
    /// report artifact).
    pub fn gitlab(&self) -> Value {
        Value::Array(
            issues(self)
                .map(|(finding, id)| {
                    json!({
                        "description": finding.message,
                        "check_name": finding.id,
                        "fingerprint": id,
                        "severity": if finding.blocking { "major" } else { "minor" },
                        "location": {
                            "path": finding.path.clone().unwrap_or_default(),
                            "lines": { "begin": finding.line.unwrap_or(1) }
                        }
                    })
                })
                .collect(),
        )
    }

    /// Azure DevOps: one `##vso[task.logissue]` logging command per
    /// problem, each on its own line.
    pub fn azure(&self) -> String {
        let mut out = String::new();
        for (finding, _) in issues(self) {
            let kind = if finding.blocking { "error" } else { "warning" };
            out.push_str(&format!("##vso[task.logissue type={kind}"));
            if let Some(path) = &finding.path {
                out.push_str(&format!(";sourcepath={}", azure_escape(path, true)));
            }
            if let Some(line) = finding.line {
                out.push_str(&format!(";linenumber={line}"));
            }
            out.push_str(&format!(
                ";code={}]{}\n",
                azure_escape(&finding.id, true),
                azure_escape(&finding.message, false)
            ));
        }
        out
    }

    /// Bitbucket Code Insights: the report and its annotations, for the
    /// two API calls that create them.
    pub fn bitbucket(&self) -> Value {
        let blocking = self.blocking();
        let advisory = self
            .findings
            .iter()
            .filter(|f| !f.blocking && f.id != "V6")
            .count();
        let details = if blocking + advisory == 0 {
            "No problems found.".to_string()
        } else {
            format!("{blocking} blocking and {advisory} advisory problem(s).")
        };
        let details = if self.tests.is_empty() {
            details
        } else {
            format!(
                "{details} {} test file(s) reach the change.",
                self.tests.len()
            )
        };
        let annotations: Vec<Value> = issues(self)
            .map(|(finding, id)| {
                let mut annotation = json!({
                    "external_id": id,
                    "annotation_type": "BUG",
                    "summary": truncate(&finding.message, BITBUCKET_SUMMARY_CHARS),
                    "severity": if finding.blocking { "HIGH" } else { "LOW" },
                });
                if let Some(path) = &finding.path {
                    annotation["path"] = json!(path);
                }
                if let Some(line) = finding.line {
                    annotation["line"] = json!(line);
                }
                annotation
            })
            .collect();
        json!({
            "report": {
                "title": "Tessera",
                "details": details,
                "report_type": "BUG",
                "result": if self.ok { "PASSED" } else { "FAILED" },
                "data": [
                    { "title": "Blocking problems", "type": "NUMBER", "value": blocking },
                    { "title": "Advisory problems", "type": "NUMBER", "value": advisory },
                    { "title": "Tests to run", "type": "NUMBER", "value": self.tests.len() }
                ]
            },
            "annotations": annotations
        })
    }

    /// One finding per line, as in `--json`, then a `{"summary": …}` line.
    pub fn ndjson(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            out.push_str(&serde_json::to_string(finding).unwrap_or_default());
            out.push('\n');
        }
        let summary = json!({
            "summary": {
                "ok": self.ok,
                "findings": self.findings.len(),
                "blocking": self.blocking(),
                "tests": self.tests,
            }
        });
        out.push_str(&summary.to_string());
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_escape_everything_but_unreserved_characters() {
        assert_eq!(uri("src/a-b_c.d~e.ts"), "src/a-b_c.d~e.ts");
        assert_eq!(
            uri("src/my dir/a#b?c[d].ts"),
            "src/my%20dir/a%23b%3Fc%5Bd%5D.ts"
        );
        assert_eq!(uri("src/ü.ts"), "src/%C3%BC.ts");
    }

    #[test]
    fn azure_escapes_percent_once() {
        assert_eq!(azure_escape("%0A", true), "%AZP250A");
        assert_eq!(azure_escape("a;b]c", false), "a;b]c");
        assert_eq!(azure_escape("a;b]c", true), "a%3Bb%5Dc");
    }
}
