use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::formats::uri;
use super::verify::{Finding, VerifyResult};

#[derive(Default)]
pub(super) struct ExecutionReport {
    checked: BTreeSet<(String, String)>,
    incomplete: BTreeMap<(String, String), BTreeSet<String>>,
    notes: BTreeSet<String>,
    parse_errors: BTreeSet<String>,
}

impl ExecutionReport {
    pub fn with_parse_errors(paths: Vec<String>) -> Self {
        Self {
            parse_errors: paths.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn checked(&mut self, id: &str, path: &str) {
        self.checked.insert((id.to_string(), path.to_string()));
        if self.parse_errors.contains(path) {
            self.incomplete(id, path, "current source has parse errors");
        }
    }

    pub fn checked_using(&mut self, id: &str, path: &str, target: Option<&str>) {
        self.checked(id, path);
        if let Some(target) = target {
            self.check_target(id, path, target);
        }
    }

    pub fn check_target(&mut self, id: &str, path: &str, target: &str) {
        if self.parse_errors.contains(target) {
            self.incomplete(id, path, "current target has parse errors");
        }
    }

    pub fn checked_contract(&mut self, path: &str) {
        self.checked("V5", path);
        if !self.parse_errors.is_empty() {
            self.incomplete(
                "V5",
                path,
                "contract index contains source with parse errors",
            );
        }
    }

    pub fn incomplete(&mut self, id: &str, path: &str, reason: &str) {
        self.incomplete
            .entry((id.to_string(), path.to_string()))
            .or_default()
            .insert(reason.to_string());
        self.note(&format!("{id} {path}: {reason}; coverage is incomplete"));
    }

    pub fn note(&mut self, note: &str) {
        self.notes.insert(note.to_string());
    }

    pub fn render(&self, result: &VerifyResult) -> String {
        let mut cases: BTreeMap<_, Vec<&Finding>> = self
            .checked
            .iter()
            .map(|key| (key.clone(), Vec::new()))
            .collect();
        for finding in result.findings.iter().filter(|f| f.id != "V6") {
            if let Some(path) = &finding.path {
                let key = (finding.id.clone(), path.clone());
                let case = cases.get_mut(&key).expect("finding has a completed check");
                case.push(finding);
            }
        }
        let failures = cases
            .values()
            .filter(|fs| fs.iter().any(|f| f.blocking))
            .count();
        let skipped = cases
            .iter()
            .filter(|(key, fs)| {
                self.incomplete.contains_key(*key) && !fs.iter().any(|f| f.blocking)
            })
            .count();
        let mut xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"tessera\" tests=\"{}\" failures=\"{failures}\" errors=\"0\" skipped=\"{skipped}\">\n", cases.len());
        xml.push_str("  <properties>\n");
        for ((id, path), findings) in &cases {
            for (n, finding) in findings.iter().enumerate() {
                let key = format!(
                    "tessera.finding|{}|{}|{}",
                    uri(id),
                    property_path(path),
                    n + 1
                );
                for (field, value) in [
                    ("fingerprint", finding.fingerprint.clone()),
                    (
                        "line",
                        finding.line.map(|n| n.to_string()).unwrap_or_default(),
                    ),
                ] {
                    writeln!(
                        xml,
                        "    <property name=\"{}|{field}\" value=\"{}\"/>",
                        escape(&key, true),
                        escape(&value, true)
                    )
                    .unwrap();
                }
            }
        }
        xml.push_str("  </properties>\n");
        for ((id, path), findings) in &cases {
            writeln!(
                xml,
                "  <testcase classname=\"{}\" name=\"{}\">",
                escape(id, true),
                escape(path, true)
            )
            .unwrap();
            if !findings.iter().any(|f| f.blocking) {
                if let Some(reason) = self.incomplete.get(&(id.clone(), path.clone())) {
                    writeln!(
                        xml,
                        "    <skipped message=\"{}\"/>",
                        escape(&reason.iter().cloned().collect::<Vec<_>>().join("; "), true)
                    )
                    .unwrap();
                }
            }
            let blocking: Vec<_> = findings.iter().copied().filter(|f| f.blocking).collect();
            if !blocking.is_empty() {
                let text = blocking
                    .iter()
                    .map(|f| describe(f))
                    .collect::<Vec<_>>()
                    .join("\n");
                writeln!(
                    xml,
                    "    <failure message=\"{}\">{}</failure>",
                    escape(&text, true),
                    escape(&text, false)
                )
                .unwrap();
            }
            let advisory = findings
                .iter()
                .copied()
                .filter(|f| !f.blocking)
                .map(describe)
                .collect::<Vec<_>>()
                .join("\n");
            if !advisory.is_empty() {
                writeln!(
                    xml,
                    "    <system-out>{}</system-out>",
                    escape(&advisory, false)
                )
                .unwrap();
            }
            xml.push_str("  </testcase>\n");
        }
        let mut notes = self.notes.iter().cloned().collect::<Vec<_>>();
        if cases.is_empty() {
            notes.insert(
                0,
                "No eligible checks completed; no passing tests were fabricated.".to_string(),
            );
        }
        if !notes.is_empty() {
            writeln!(
                xml,
                "  <system-out>{}</system-out>",
                escape(&notes.join("\n"), false)
            )
            .unwrap();
        }
        xml.push_str("</testsuite>\n");
        xml
    }
}

fn property_path(path: &str) -> String {
    // Property identities encode every byte, including separators and controls.
    path.bytes().map(|b| format!("{b:02X}")).collect()
}

fn describe(finding: &Finding) -> String {
    let mut text = format!(
        "{}:{}: {}",
        finding.path.as_deref().unwrap_or("?"),
        finding.line.unwrap_or(0),
        finding.message
    );
    if let Some(note) = finding.waiver_note() {
        write!(text, " ({note})").unwrap();
    }
    text
}

fn escape(value: &str, attribute: bool) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&apos;"),
            '\n' if attribute => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' if attribute => out.push_str("&#9;"),
            '\t'
            | '\n'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}' => out.push(c),
            _ => write!(out, "[U+{:04X}]", c as u32).unwrap(),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Confidence;

    #[test]
    fn xml_characters_and_attribute_whitespace() {
        let source = "&<>\"'\t\r\n\0\u{1f}\u{fffe}\u{ffff}😀";
        assert_eq!(
            escape(source, true),
            "&amp;&lt;&gt;&quot;&apos;&#9;&#13;&#10;[U+0000][U+001F][U+FFFE][U+FFFF]😀"
        );
        assert_eq!(
            escape(source, false),
            "&amp;&lt;&gt;\"'\t&#13;\n[U+0000][U+001F][U+FFFE][U+FFFF]😀"
        );
        for c in (0..32).filter_map(char::from_u32) {
            if !matches!(c, '\t' | '\r' | '\n') {
                assert!(!escape(&c.to_string(), true).contains(c));
                assert!(!escape(&c.to_string(), false).contains(c));
            }
        }
    }

    #[test]
    fn grouping_properties_and_incomplete_checks_are_deterministic() {
        let path = "a\t\r\n&<>\"'.ts";
        let mut report = ExecutionReport::default();
        report.checked("V1", path);
        report.checked("V1", path);
        report.incomplete("V1", path, "history unavailable");
        let finding = Finding {
            id: "V1".into(),
            confidence: Confidence::Medium,
            blocking: false,
            message: "a & b\u{1}".into(),
            path: Some(path.into()),
            line: Some(2),
            suggestions: Vec::new(),
            fingerprint: "exact-fingerprint".into(),
            waived: None,
        };
        let result = VerifyResult::new(vec![finding.clone()], Vec::new());
        let xml = report.render(&result);
        assert!(xml.contains("tests=\"1\" failures=\"0\" errors=\"0\" skipped=\"1\""));
        assert!(xml.contains("a&#9;&#13;&#10;&amp;&lt;&gt;&quot;&apos;.ts"));
        assert!(xml.contains("|1|fingerprint\" value=\"exact-fingerprint\""));
        assert!(xml.contains("a &amp; b[U+0001]"));
        assert!(xml.contains("<skipped"));
        assert_eq!(xml, report.render(&result));
        let mut blocking = finding;
        blocking.blocking = true;
        let xml = report.render(&VerifyResult::new(vec![blocking], Vec::new()));
        assert!(xml.contains("failures=\"1\" errors=\"0\" skipped=\"0\""));
        assert!(!xml.contains("<skipped"));
    }

    #[test]
    fn public_verify_result_literals_and_json_stay_unchanged() {
        let result = VerifyResult {
            ok: true,
            findings: Vec::new(),
            tests: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({"ok":true,"findings":[],"tests":[]})
        );
    }
}
