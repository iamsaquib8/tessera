mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn repo(lang: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(lang), temp.path());
    temp
}

fn tessera(cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("tessera").unwrap();
    cmd.current_dir(cwd).arg("--quiet");
    cmd
}

#[test]
fn first_query_builds_the_index() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["find", "findById"])
        .assert()
        .success()
        .stdout(predicate::str::contains("UserRepo.findById"));
    assert!(temp.path().join(".tessera/tessera.db").exists());
    assert_eq!(
        fs::read_to_string(temp.path().join(".tessera/.gitignore")).unwrap(),
        "*\n"
    );
}

#[test]
fn queries_from_a_subdirectory_use_the_repo_root() {
    let temp = repo("typescript");
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    tessera(&temp.path().join("src/users"))
        .args(["refs", "formatName"])
        .assert()
        .success()
        .stdout(predicate::str::contains("src/users/repo.ts"));
    assert!(temp.path().join(".tessera/tessera.db").exists());
    assert!(!temp.path().join("src/users/.tessera").exists());
}

#[test]
fn verify_exit_codes() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["verify", "--snippet", "-", "--language", "typescript"])
        .write_stdin("findByIdd(1);\n")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("findByIdd"))
        .stdout(predicate::str::contains("UserRepo.findById"));
    tessera(temp.path())
        .args(["verify", "findById"])
        .assert()
        .success();
}

#[test]
fn empty_directory_is_not_an_error() {
    let temp = TempDir::new().unwrap();
    tessera(temp.path())
        .args(["find", "anything"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No supported source files found under",
        ));
}

#[test]
fn missing_arguments_exit_with_usage_code() {
    let temp = repo("typescript");
    tessera(temp.path()).arg("find").assert().code(2);
    tessera(temp.path()).arg("blast").assert().code(2);
    tessera(temp.path()).arg("verify").assert().code(2);
    tessera(temp.path()).arg("no-such-command").assert().code(2);
}

#[test]
fn every_command_supports_json() {
    let temp = repo("typescript");
    let commands: &[&[&str]] = &[
        &["status"],
        &["find", "findById"],
        &["find", "--path", "src/users/repo.ts"],
        &["search", "load*"],
        &["refs", "loadUser"],
        &["blast", "formatName"],
        &["context", "findById"],
        &["path", "handler", "formatName"],
        &["export", "--from", "loadUser"],
        &["index"],
    ];
    for args in commands {
        let output = tessera(temp.path())
            .arg("--json")
            .args(*args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?} failed");
        serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .unwrap_or_else(|e| panic!("{args:?} did not print JSON: {e}"));
    }
    let verify = tessera(temp.path())
        .args(["--json", "verify", "nopeNope"])
        .output()
        .unwrap();
    assert_eq!(verify.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(value["ok"], false);
}

#[test]
fn indexes_every_supported_language() {
    let cases = [
        ("typescript", "loadUser"),
        ("tsx", "Avatar"),
        ("javascript", "slugify"),
        ("python", "format_name"),
        ("go", "Load"),
        ("rust", "handle"),
        ("java", "findById"),
        ("c", "store_load"),
        ("cpp", "load"),
        ("csharp", "FindById"),
        ("ruby", "display_name"),
        ("php", "findById"),
    ];
    for (lang, symbol) in cases {
        let temp = repo(lang);
        let output = tessera(temp.path())
            .args(["--json", "find", symbol])
            .output()
            .unwrap();
        assert!(output.status.success(), "{lang}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let first = &value["matches"][0];
        assert_eq!(first["match_kind"]["kind"], "exact", "{lang}: {value}");
        assert_eq!(first["symbol"]["name"], symbol, "{lang}");
    }
}

#[test]
fn index_exclude_flag_is_remembered() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["index", "--exclude", "test/**"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"));
    fs::write(
        temp.path().join("src/extra.ts"),
        "export function extra() { return 1; }\n",
    )
    .unwrap();
    let output = tessera(temp.path())
        .args(["--json", "refs", "loadUser", "--tests"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["page"]["total"], 0, "{value}");
}

#[test]
fn blast_diff_reports_changed_symbols() {
    let temp = repo("typescript");
    let git = |args: &[&str]| {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(temp.path())
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "one"]);
    fs::write(
        temp.path().join("src/util/format.ts"),
        "export function formatName(name: string): string {\n  return name.trim().toLowerCase();\n}\n\nexport function loadUser(id: string): string {\n  return formatName(id);\n}\n",
    )
    .unwrap();
    git(&["commit", "-q", "-am", "two"]);
    tessera(temp.path())
        .args(["blast", "--diff", "HEAD~1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("formatName"));
}

#[test]
fn watch_once_indexes_and_exits() {
    let temp = repo("go");
    tessera(temp.path())
        .args(["watch", "--once"])
        .assert()
        .success();
    assert!(temp.path().join(".tessera/tessera.db").exists());
}

#[test]
fn init_writes_mcp_snippets() {
    let temp = repo("go");
    tessera(temp.path())
        .args(["init", "--mcp-configs"])
        .assert()
        .success();
    assert!(temp.path().join(".tessera/config.toml").exists());
    assert!(temp.path().join(".tessera/mcp").is_dir());
}

#[test]
fn export_renders_mermaid() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["export", "--from", "loadUser"])
        .assert()
        .success()
        .stdout(predicate::str::contains("flowchart").or(predicate::str::contains("graph")));
}

#[test]
fn completions_zsh_lists_commands() {
    let temp = TempDir::new().unwrap();
    tessera(temp.path())
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef tessera"))
        .stdout(predicate::str::contains("blast"))
        .stdout(predicate::str::contains("verify"));
}

#[test]
fn status_on_a_fresh_checkout_explains_what_to_do() {
    let temp = repo("go");
    tessera(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("missing"))
        .stdout(predicate::str::contains("Any query builds it"));
}

#[test]
fn first_index_is_reported_as_full() {
    let temp = repo("go");
    tessera(temp.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[full]"));
    tessera(temp.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[incremental]"));
}
