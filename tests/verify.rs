mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

/// The TypeScript fixture as a committed git repository.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    }
    temp
}

fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let file = root.join(path);
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains(from), "{path} lacks {from:?}");
    fs::write(&file, content.replacen(from, to, 1)).unwrap();
}

/// `(exit code, findings as "ID path:line")` for `verify --changed`.
fn verify_changed(root: &Path) -> (i32, Vec<String>, serde_json::Value) {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args(["--quiet", "--json", "verify", "--changed"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let findings = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{} {}:{}",
                f["id"].as_str().unwrap(),
                f["path"].as_str().unwrap_or("-"),
                f["line"].as_u64().unwrap_or(0)
            )
        })
        .collect();
    (output.status.code().unwrap(), findings, value)
}

#[test]
fn a_clean_change_passes_and_lists_tests_to_run() {
    let temp = repo();
    edit(
        temp.path(),
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    let (code, findings, value) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
    assert!(findings.iter().all(|f| f.starts_with("V6")), "{findings:?}");
    assert!(value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "test/repo.test.ts"));
}

#[test]
fn v1_calls_to_names_that_do_not_exist_where_they_point() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findByIdd(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V1 src/users/repo.ts:17".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v3_and_v4_a_renamed_export_breaks_its_importers() {
    let temp = repo();
    edit(
        temp.path(),
        "src/util/format.ts",
        "export function formatName(",
        "export function formatDisplayName(",
    );
    edit(
        temp.path(),
        "src/util/format.ts",
        "return formatName(id);",
        "return formatDisplayName(id);",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V4 src/users/repo.ts:1".to_string()),
        "{findings:?}"
    );
    assert!(
        findings.contains(&"V3 src/users/repo.ts:7".to_string()),
        "{findings:?}"
    );
}

#[test]
fn v2_a_new_required_parameter_breaks_existing_callers() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "export function loadUser(id: string): string {",
        "export function loadUser(id: string, tenant: string): string {",
    );
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 1);
    assert!(
        findings.contains(&"V2 src/api/handler.ts:4".to_string()),
        "{findings:?}"
    );
    assert!(
        findings.contains(&"V2 test/repo.test.ts:5".to_string()),
        "{findings:?}"
    );
}

#[test]
fn external_calls_and_inherited_members_never_block() {
    let temp = repo();
    fs::write(
        temp.path().join("src/view.ts"),
        "import { useState } from \"react\";\nimport { Component } from \"react\";\n\n\
         export class View extends Component {\n  render() {\n    this.setState({});\n    return useState(0).notAThing();\n  }\n}\n",
    )
    .unwrap();
    let (code, findings, _) = verify_changed(temp.path());
    assert_eq!(code, 0, "{findings:?}");
}

#[test]
fn standard_tier_findings_are_advisory() {
    let temp = repo();
    fs::create_dir_all(temp.path().join("py")).unwrap();
    fs::write(
        temp.path().join("py/util.py"),
        "def helper():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("py/main.py"),
        "from py.util import helperr\n\ndef run():\n    return helperr()\n",
    )
    .unwrap();
    let (code, _, value) = verify_changed(temp.path());
    assert_eq!(code, 0, "{value}");
}

#[test]
fn agent_format_is_terse_and_sarif_is_valid() {
    let temp = repo();
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findByIdd(id);",
    );
    let agent = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args(["--quiet", "verify", "--changed", "--format", "agent"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&agent.stdout);
    assert!(text.contains("src/users/repo.ts:17"), "{text}");
    assert!(text.lines().count() <= 6, "{text}");
    let sarif = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args(["--quiet", "verify", "--changed", "--format", "sarif"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    assert_eq!(value["version"], "2.1.0");
    assert_eq!(value["runs"][0]["results"][0]["ruleId"], "V1");
}
