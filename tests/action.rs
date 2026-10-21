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

#[test]
fn the_action_installs_its_own_release_from_its_own_checkout() {
    let action = repo_file("action/action.yml");
    assert!(
        action.contains(&format!("default: v{}", env!("CARGO_PKG_VERSION"))),
        "the action defaults to the release it ships with"
    );
    assert!(action.contains("\"$GITHUB_ACTION_PATH/../install.sh\""));
    assert!(!action.contains("raw.githubusercontent.com"));
}

/// The comment lookup, run the way `gh api --paginate --jq` runs it: once
/// per page.
#[test]
fn the_comment_lookup_takes_one_comment_across_pages() {
    let action = repo_file("action/action.yml");
    let program = action
        .split("--jq '")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .expect("a --jq program");
    assert!(action.contains("| head -n1"), "one id, not one per page");
    let Ok(jq) = which_jq() else {
        return;
    };
    let pages = [
        r#"[{"id":1,"user":{"login":"someone"},"body":"<!-- tessera --> copied"}]"#,
        r#"[{"id":2,"user":{"login":"github-actions[bot]"},"body":"<!-- tessera -->\n### Tessera"}]"#,
        r#"[{"id":3,"user":{"login":"github-actions[bot]"},"body":"other"}]"#,
    ];
    let mut ids = String::new();
    for page in pages {
        let mut child = std::process::Command::new(&jq)
            .args(["-r", program])
            .env("AUTHOR", "github-actions[bot]")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(page.as_bytes())
            .unwrap();
        ids.push_str(&String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap());
    }
    assert_eq!(ids.trim(), "2");
}

fn which_jq() -> Result<String, ()> {
    std::process::Command::new("jq")
        .arg("--version")
        .output()
        .map(|_| "jq".to_string())
        .map_err(|_| ())
}
