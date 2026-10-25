//! `tessera cache export` and `import`: an index snapshot CI moves between
//! machines. A cache from another version, schema, configuration or
//! repository is refused whole, never half-loaded.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};

use assert_cmd::Command;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    let output = StdCommand::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// `fixture` copied to `<temp>/<name>` and committed.
fn repo_in(temp: &TempDir, name: &str, fixture: &str) -> PathBuf {
    let root = temp.path().join(name);
    copy_dir(&fixture_dir(fixture), &root);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "base"]);
    root
}

fn tessera(root: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .env_remove("TESSERA_DB")
        .arg("--quiet")
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn export(root: &Path, file: &Path) -> Value {
    let output = tessera(root, &["--json", "cache", "export", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    json(&output)
}

fn import(root: &Path, file: &Path) -> Output {
    tessera(root, &["cache", "import", file.to_str().unwrap()])
}

fn db(root: &Path) -> PathBuf {
    root.join(".tessera/tessera.db")
}

/// The header line of a cache file, edited by `change`, written back.
fn edit_header(file: &Path, change: impl FnOnce(&mut Value)) {
    let bytes = fs::read(file).unwrap();
    let magic_end = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    let header_end = magic_end + bytes[magic_end..].iter().position(|b| *b == b'\n').unwrap();
    let mut header: Value = serde_json::from_slice(&bytes[magic_end..header_end]).unwrap();
    change(&mut header);
    let mut out = bytes[..magic_end].to_vec();
    out.extend_from_slice(serde_json::to_string(&header).unwrap().as_bytes());
    out.extend_from_slice(&bytes[header_end..]);
    fs::write(file, out).unwrap();
}

/// Refused: exit 3 with `expect` on stderr, and the index at `root` exactly
/// as it was (absent if it was absent).
fn assert_refused(root: &Path, file: &Path, expect: &str) {
    let before = fs::read(db(root)).ok();
    let output = import(root, file);
    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(expect),
        "expected {expect:?} in: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read(db(root)).ok(),
        before,
        "a refused cache changed the index"
    );
}

#[test]
fn export_then_import_on_another_checkout_reindexes_only_what_differs() {
    let temp = TempDir::new().unwrap();
    let origin = repo_in(&temp, "origin", "typescript");
    let file = temp.path().join("caches/index.cache");
    let exported = export(&origin, &file);
    assert_eq!(
        exported["tree"],
        git(&origin, &["rev-parse", "HEAD^{tree}"])
    );
    assert_eq!(exported["version"], env!("CARGO_PKG_VERSION"));
    assert!(exported["files"].as_u64().unwrap() >= 4, "{exported}");
    assert!(file.is_file());
    let leftovers: Vec<_> = fs::read_dir(file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "temp files left behind: {leftovers:?}");

    // Another machine: a fresh clone at another path, one file edited.
    let clone = temp.path().join("clone");
    git(
        temp.path(),
        &["clone", "-q", origin.to_str().unwrap(), "clone"],
    );
    let repo_ts = clone.join("src/users/repo.ts");
    let source = fs::read_to_string(&repo_ts).unwrap();
    fs::write(
        &repo_ts,
        source.replacen("findById(id: string)", "findOne(id: string)", 1),
    )
    .unwrap();

    let output = tessera(
        &clone,
        &["--json", "cache", "import", file.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let imported = json(&output);
    assert_eq!(imported["imported"], true, "{imported}");
    assert_eq!(imported["tree"], exported["tree"]);
    assert_eq!(imported["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(imported["reindexed"], 1, "only the edited file: {imported}");
    assert_eq!(imported["removed"], 0, "{imported}");

    let status = json(&tessera(&clone, &["--json", "status"]));
    assert_eq!(status["freshness"]["state"], "fresh", "{status}");
    let index = tessera_codegraph::Index::open(db(&clone)).unwrap();
    assert_eq!(
        index.root().canonicalize().unwrap(),
        clone.canonicalize().unwrap(),
        "the index reads files from this checkout, not the exporter's"
    );
    let found = json(&tessera(&clone, &["--json", "find", "findOne"]));
    assert!(
        found.to_string().contains("src/users/repo.ts"),
        "the edit is in the index: {found}"
    );
}

#[test]
fn import_replaces_an_existing_index_and_drops_what_is_gone() {
    let temp = TempDir::new().unwrap();
    let origin = repo_in(&temp, "origin", "typescript");
    let file = temp.path().join("index.cache");
    export(&origin, &file);
    // The local index knows a file the cache and the working tree do not.
    fs::write(origin.join("src/extra.ts"), "export function extra() {}\n").unwrap();
    assert_eq!(tessera(&origin, &["index"]).status.code(), Some(0));
    fs::remove_file(origin.join("src/extra.ts")).unwrap();
    let output = tessera(
        &origin,
        &["--json", "cache", "import", file.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(json(&output)["reindexed"], 0, "nothing differs");
    let found = json(&tessera(&origin, &["--json", "search", "extra"]));
    assert!(!found.to_string().contains("extra.ts"), "{found}");
}

#[test]
fn a_cache_outside_git_has_no_tree() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("plain");
    copy_dir(&fixture_dir("typescript"), &root);
    fs::create_dir_all(root.join(".tessera")).unwrap();
    let file = temp.path().join("index.cache");
    let exported = export(&root, &file);
    assert_eq!(exported["tree"], Value::Null, "{exported}");
    let output = tessera(
        &root,
        &["--json", "cache", "import", file.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn a_cache_from_another_tessera_version_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let file = temp.path().join("index.cache");
    export(&root, &file);
    edit_header(&file, |h| h["version"] = "0.0.1".into());
    assert_refused(&root, &file, "Tessera 0.0.1");
    // An index already there survives the refusal.
    assert_eq!(tessera(&root, &["index"]).status.code(), Some(0));
    assert_refused(&root, &file, "Tessera 0.0.1");
}

#[test]
fn a_cache_with_another_schema_or_format_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let file = temp.path().join("index.cache");
    export(&root, &file);
    let original = fs::read(&file).unwrap();
    edit_header(&file, |h| h["schema"] = 1.into());
    assert_refused(&root, &file, "schema v1");
    fs::write(&file, &original).unwrap();
    edit_header(&file, |h| h["format"] = 99.into());
    assert_refused(&root, &file, "format 99");
}

#[test]
fn a_cache_built_under_another_config_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let file = temp.path().join("index.cache");
    export(&root, &file);
    fs::create_dir_all(root.join(".tessera")).unwrap();
    fs::write(
        root.join(".tessera/config.toml"),
        "[exclude]\npaths = [\"test/\"]\n",
    )
    .unwrap();
    assert_refused(&root, &file, ".tessera/config.toml");
}

#[test]
fn a_cache_built_with_other_excludes_than_the_local_index_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let file = temp.path().join("index.cache");
    export(&root, &file);
    let indexed = tessera(&root, &["index", "--exclude", "test/**"]);
    assert_eq!(indexed.status.code(), Some(0), "{}", stderr(&indexed));
    assert_refused(&root, &file, "--exclude");
}

#[test]
fn a_cache_from_another_repository_is_refused() {
    let temp = TempDir::new().unwrap();
    let typescript = repo_in(&temp, "typescript", "typescript");
    let python = repo_in(&temp, "python", "python");
    let file = temp.path().join("index.cache");
    export(&typescript, &file);
    assert_refused(&python, &file, "another repository");
    assert_eq!(tessera(&python, &["index"]).status.code(), Some(0));
    assert_refused(&python, &file, "another repository");
}

#[test]
fn a_cache_of_another_directory_of_the_repository_is_refused() {
    let temp = TempDir::new().unwrap();
    let mono = temp.path().join("mono");
    copy_dir(&fixture_dir("typescript"), &mono.join("packages/a"));
    copy_dir(&fixture_dir("typescript"), &mono.join("packages/b"));
    fs::create_dir_all(mono.join("packages/a/.tessera")).unwrap();
    fs::create_dir_all(mono.join("packages/b/.tessera")).unwrap();
    git(&mono, &["init", "-q"]);
    git(&mono, &["add", "."]);
    git(&mono, &["commit", "-q", "-m", "base"]);
    let file = temp.path().join("index.cache");
    let exported = export(&mono.join("packages/a"), &file);
    assert_eq!(exported["root"], "packages/a", "{exported}");
    assert_refused(&mono.join("packages/b"), &file, "packages/a");
}

#[test]
fn a_damaged_missing_or_foreign_file_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let file = temp.path().join("index.cache");
    export(&root, &file);
    let bytes = fs::read(&file).unwrap();
    fs::write(&file, &bytes[..bytes.len() - 40]).unwrap();
    assert_refused(&root, &file, "damaged");
    fs::write(&file, b"PK\x03\x04 not a cache").unwrap();
    assert_refused(&root, &file, "not a Tessera index cache");
    assert_refused(&root, &temp.path().join("missing.cache"), "no cache file");
}

#[test]
fn a_refusal_is_reported_in_json_too() {
    let temp = TempDir::new().unwrap();
    let root = repo_in(&temp, "repo", "typescript");
    let missing = temp.path().join("missing.cache");
    let output = tessera(
        &root,
        &["--json", "cache", "import", missing.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(3));
    let value = json(&output);
    assert_eq!(value["imported"], false, "{value}");
    assert!(
        value["reason"].as_str().unwrap().contains("no cache file"),
        "{value}"
    );
}
