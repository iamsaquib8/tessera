use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use tessera_codegraph::query::report::MARKER;

fn repo_file(path: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

#[test]
fn the_action_runs_commands_the_cli_has() {
    let action = repo_file("action/action.yml");
    for input in [
        "version:", "binary:", "base:", "fail-on:", "comment:", "token:",
    ] {
        assert!(action.contains(&format!("  {input}")), "input {input}");
    }
    assert!(action.contains("verify --diff \"$BASE\" --format markdown"));
    let help = Command::cargo_bin("tessera")
        .unwrap()
        .args(["verify", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("--diff") && help.contains("markdown"),
        "{help}"
    );
}

#[test]
fn the_action_finds_its_comment_by_the_report_marker() {
    let action = repo_file("action/action.yml");
    assert!(
        action.contains(&format!("startswith(\"{MARKER}\")")),
        "marker drift"
    );
}

#[test]
fn this_repository_runs_the_action_on_its_pull_requests() {
    let workflow = repo_file(".github/workflows/tessera.yml");
    assert!(workflow.contains("uses: ./action"));
    assert!(workflow.contains("binary: target/release/tessera"));
}
