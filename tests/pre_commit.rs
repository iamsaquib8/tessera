//! `.pre-commit-hooks.yaml` and the recipes in docs/integrations.md: the hook
//! stops a commit that breaks a call, and only that.

mod support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use support::{copy_dir, fixture_dir};
use tempfile::TempDir;
use yaml_rust2::YamlLoader;

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The TypeScript fixture as a committed repository.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-q", "-m", "base"]);
    temp
}

/// A directory holding only a `tessera` that is the binary under test.
fn path_with_tessera() -> std::ffi::OsString {
    let dir = Path::new(env!("CARGO_BIN_EXE_tessera")).parent().unwrap();
    let mut paths = vec![dir.to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    std::env::join_paths(paths).unwrap()
}

fn entry() -> String {
    let docs =
        YamlLoader::load_from_str(&fs::read_to_string(".pre-commit-hooks.yaml").unwrap()).unwrap();
    let hook = &docs[0][0];
    assert_eq!(hook["id"].as_str(), Some("tessera-verify"));
    assert_eq!(hook["language"].as_str(), Some("system"));
    assert_eq!(hook["pass_filenames"].as_bool(), Some(false));
    assert_eq!(hook["always_run"].as_bool(), Some(true));
    hook["entry"].as_str().unwrap().to_string()
}

fn run_entry(root: &Path) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg(entry())
        .current_dir(root)
        .env("PATH", path_with_tessera())
        .output()
        .unwrap()
}

fn break_a_call(root: &Path) {
    let file = root.join("src/users/repo.ts");
    let source = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        source.replacen("findById(id: string)", "findOne(id: string)", 1),
    )
    .unwrap();
    git(root, &["add", "src/users/repo.ts"]);
}

#[test]
fn the_hook_passes_a_clean_change_and_stops_a_broken_one() {
    let temp = repo();
    let clean = run_entry(temp.path());
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );

    break_a_call(temp.path());
    let broken = run_entry(temp.path());
    assert_eq!(
        broken.status.code(),
        Some(1),
        "a broken call stops the commit"
    );
}

#[test]
fn an_index_error_does_not_stop_a_commit() {
    let temp = repo();
    break_a_call(temp.path());
    // A database that is not an index makes `verify` exit with an index error.
    fs::create_dir_all(temp.path().join(".tessera")).unwrap();
    fs::write(temp.path().join(".tessera/tessera.db"), "not a database").unwrap();
    let verify = Command::new(env!("CARGO_BIN_EXE_tessera"))
        .args(["verify", "--staged", "--format", "agent", "--quiet"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_ne!(verify.status.code(), Some(1), "setup: not a blocking exit");
    assert!(verify.status.code() != Some(0), "setup: an error exit");
    let hook = run_entry(temp.path());
    assert!(
        hook.status.success(),
        "an index error is not a blocking problem"
    );
}

#[test]
fn the_pre_commit_framework_runs_it_when_installed() {
    if Command::new("pre-commit")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let temp = repo();
    let hooks = std::env::current_dir().unwrap();
    // try-repo needs a committed source repository: copy the hooks file into one.
    let source = TempDir::new().unwrap();
    fs::copy(
        hooks.join(".pre-commit-hooks.yaml"),
        source.path().join(".pre-commit-hooks.yaml"),
    )
    .unwrap();
    git(source.path(), &["init", "-q"]);
    git(source.path(), &["add", "."]);
    git(source.path(), &["commit", "-q", "-m", "hooks"]);
    let run = |expect_ok: bool| {
        let out = Command::new("pre-commit")
            .args(["try-repo"])
            .arg(source.path())
            .arg("tessera-verify")
            .current_dir(temp.path())
            .env("PATH", path_with_tessera())
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            expect_ok,
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    };
    run(true);
    break_a_call(temp.path());
    run(false);
}

#[test]
fn the_docs_have_the_three_recipes() {
    let docs = fs::read_to_string("docs/integrations.md")
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(docs.contains("id: tessera-verify"), "pre-commit recipe");
    assert!(docs.contains("lefthook.yml"), "lefthook recipe");
    assert!(docs.contains(".husky/pre-commit"), "husky recipe");
    assert!(
        docs.contains("`tessera` on your `PATH`"),
        "says tessera must be on PATH"
    );
}
