mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The TypeScript fixture committed with one broken call already in it:
/// `loadUser` calls `findOne`, which `UserRepo` does not have.
fn legacy_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
    edit(
        temp.path(),
        "src/users/repo.ts",
        "return repo.findById(id);",
        "return repo.findOne(id);",
    );
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "legacy"]);
    temp
}

fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let file = root.join(path);
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains(from), "{path} lacks {from:?}");
    fs::write(&file, content.replacen(from, to, 1)).unwrap();
}

fn tessera(root: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .arg("--quiet")
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn verify(root: &Path, extra: &[&str]) -> (i32, Value) {
    let mut args = vec!["--json", "verify", "--changed"];
    args.extend_from_slice(extra);
    let (code, out, err) = tessera(root, &args);
    let value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}\n{err}"));
    (code, value)
}

fn problems(value: &Value) -> Vec<&Value> {
    value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["id"] != "V6")
        .collect()
}

/// Touch the file with the old problem, so `verify --changed` looks at it.
fn touch_legacy(root: &Path) {
    edit(
        root,
        "src/users/repo.ts",
        "export class UserRepo {",
        "// touched\nexport class UserRepo {",
    );
}

#[test]
fn a_baselined_finding_is_advisory_and_a_new_one_still_blocks() {
    let temp = legacy_repo();
    let root = temp.path();
    let (code, out, err) = tessera(root, &["--json", "baseline"]);
    assert_eq!(code, 0, "{out}{err}");
    let written: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(written["findings"], 1, "{written}");
    let file: Value =
        serde_json::from_str(&fs::read_to_string(root.join(".tessera/baseline.json")).unwrap())
            .unwrap();
    assert_eq!(file["findings"].as_array().unwrap().len(), 1, "{file}");

    touch_legacy(root);
    let (code, value) = verify(root, &[]);
    assert_eq!(code, 0, "the old problem no longer blocks: {value}");
    let old = problems(&value);
    assert_eq!(old.len(), 1, "{value}");
    assert_eq!(old[0]["blocking"], false);
    assert_eq!(old[0]["waived"]["by"], "baseline", "{value}");

    let (code, value) = verify(root, &["--no-baseline"]);
    assert_eq!(code, 1, "--no-baseline shows everything: {value}");

    edit(
        root,
        "src/api/handler.ts",
        "loadUser(req.id)",
        "loadUser(req.id, 2)",
    );
    let (code, value) = verify(root, &[]);
    assert_eq!(code, 1, "a new problem blocks: {value}");
}

#[test]
fn an_accepted_break_carries_its_reason_until_it_expires() {
    let temp = legacy_repo();
    let root = temp.path();
    touch_legacy(root);
    let (_, value) = verify(root, &[]);
    let print = problems(&value)[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();

    let (code, out, err) = tessera(
        root,
        &[
            "accept",
            &print,
            "--reason",
            "findOne lands in \"repo v2\"",
            "--expires",
            "2999-01-01",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    let (code, value) = verify(root, &[]);
    assert_eq!(code, 0, "{value}");
    let waived = &problems(&value)[0]["waived"];
    assert_eq!(waived["by"], "accepted", "{value}");
    assert_eq!(waived["reason"], "findOne lands in \"repo v2\"", "{value}");

    fs::write(
        root.join(".tessera/accepted.toml"),
        format!(
            "[[accept]]\nfingerprint = \"{print}\"\nreason = \"old\"\nexpires = \"2001-01-01\"\n"
        ),
    )
    .unwrap();
    let (code, value) = verify(root, &[]);
    assert_eq!(code, 1, "an expired acceptance blocks again: {value}");
    assert_eq!(problems(&value)[0]["waived"]["expired"], true, "{value}");

    let (code, _, err) = tessera(root, &["accept", "not-a-fingerprint", "--reason", "x"]);
    assert_eq!(code, 2, "{err}");
    let (code, _, err) = tessera(root, &["accept", &print]);
    assert_eq!(code, 2, "a reason is required: {err}");
}

#[test]
fn a_broken_waiver_file_means_everything_counts() {
    let temp = legacy_repo();
    let root = temp.path();
    touch_legacy(root);
    fs::create_dir_all(root.join(".tessera")).unwrap();
    fs::write(root.join(".tessera/baseline.json"), "{ not json").unwrap();
    fs::write(root.join(".tessera/accepted.toml"), "[[accept]\n").unwrap();
    let (code, out, err) = tessera(root, &["--json", "verify", "--changed"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        err.contains("baseline.json") && err.contains("accepted.toml"),
        "{err}"
    );
}

#[test]
fn pruning_the_baseline_drops_what_no_longer_fires() {
    let temp = legacy_repo();
    let root = temp.path();
    tessera(root, &["baseline"]);
    edit(
        root,
        "src/users/repo.ts",
        "return repo.findOne(id);",
        "return repo.findById(id);",
    );
    git(root, &["commit", "-qam", "fixed"]);
    let (code, out, err) = tessera(root, &["--json", "baseline", "--prune"]);
    assert_eq!(code, 0, "{out}{err}");
    let file: Value =
        serde_json::from_str(&fs::read_to_string(root.join(".tessera/baseline.json")).unwrap())
            .unwrap();
    assert!(file["findings"].as_array().unwrap().is_empty(), "{file}");
}

#[test]
fn the_baseline_and_accepted_files_can_be_committed() {
    let temp = legacy_repo();
    let root = temp.path();
    tessera(root, &["baseline"]);
    let ignore = fs::read_to_string(root.join(".tessera/.gitignore")).unwrap();
    assert!(
        ignore.contains("!baseline.json") && ignore.contains("!accepted.toml"),
        "{ignore}"
    );
    let status = StdCommand::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .unwrap();
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(status.contains(".tessera/baseline.json"), "{status}");
    assert!(!status.contains("tessera.db"), "{status}");
}

#[test]
fn an_expiry_that_is_not_a_date_blocks_and_a_toml_date_works() {
    let temp = legacy_repo();
    let root = temp.path();
    touch_legacy(root);
    let (_, value) = verify(root, &[]);
    let print = problems(&value)[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    let accepted = root.join(".tessera/accepted.toml");
    fs::create_dir_all(accepted.parent().unwrap()).unwrap();

    for expires in ["\"never\"", "\"31.12.2999\"", "\"2999-13-01\""] {
        fs::write(
            &accepted,
            format!("[[accept]]\nfingerprint = \"{print}\"\nreason = \"r\"\nexpires = {expires}\n"),
        )
        .unwrap();
        let (code, out, err) = tessera(root, &["--json", "verify", "--changed"]);
        assert_eq!(code, 1, "{expires} never expires otherwise: {out}");
        assert!(err.contains("is not a date"), "{expires}: {err}");
    }

    // A bare TOML date is a date.
    fs::write(
        &accepted,
        format!("[[accept]]\nfingerprint = \"{print}\"\nreason = \"r\"\nexpires = 2999-01-01\n"),
    )
    .unwrap();
    let (code, value) = verify(root, &[]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(problems(&value)[0]["waived"]["expires"], "2999-01-01");

    let (code, _, err) = tessera(
        root,
        &["accept", &print, "--reason", "r", "--expires", "2026-02-30"],
    );
    assert_eq!(code, 2, "an impossible date: {err}");
}
