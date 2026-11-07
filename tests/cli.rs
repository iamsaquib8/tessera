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
        tessera_codegraph::indexer::INDEX_DIR_GITIGNORE
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
fn export_renders_mermaid() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["export", "--from", "loadUser"])
        .assert()
        .success()
        .stdout(predicate::str::contains("flowchart").or(predicate::str::contains("graph")));
}

#[test]
fn export_html_preview_loads_nothing_from_the_network() {
    let temp = repo("typescript");
    let out = temp.path().join("preview.html");
    tessera(temp.path())
        .args(["export", "--from", "loadUser", "--html-out"])
        .arg(&out)
        .assert()
        .success();
    let page = fs::read_to_string(&out).unwrap();
    assert!(page.contains("loadUser"), "preview should list the symbol");
    for needle in ["http://", "https://", "<script src", "<link", "import "] {
        assert!(!page.contains(needle), "preview contains `{needle}`");
    }
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

#[test]
fn watch_keeps_persisted_excludes() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["index", "--exclude", "test/**"])
        .assert()
        .success();
    tessera(temp.path())
        .args(["watch", "--once"])
        .assert()
        .success();
    let output = tessera(temp.path())
        .args(["--json", "refs", "loadUser", "--tests"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["page"]["total"], 0, "{value}");
}

#[test]
fn raising_max_file_bytes_reindexes_skipped_files() {
    let temp = repo("typescript");
    let mut big = String::from("export function bigFn7() { return 7; }\n");
    while big.len() < 1_200_000 {
        big.push_str("// padding padding padding padding padding padding padding\n");
    }
    fs::write(temp.path().join("src/big.ts"), big).unwrap();
    tessera(temp.path())
        .args(["find", "bigFn7"])
        .assert()
        .stdout(predicate::str::contains("src/big.ts").not());
    fs::write(
        temp.path().join(".tessera/config.toml"),
        "[index]\nmax_file_bytes = 8_000_000\n",
    )
    .unwrap();
    tessera(temp.path())
        .args(["find", "bigFn7"])
        .assert()
        .success()
        .stdout(predicate::str::contains("src/big.ts"));
}

#[cfg(unix)]
#[test]
fn queries_do_not_run_other_binaries_on_path() {
    use std::os::unix::fs::PermissionsExt;
    let temp = repo("typescript");
    let bin = TempDir::new().unwrap();
    let marker = bin.path().join("ran");
    let fake = bin.path().join("tessera");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\ntouch '{}'\necho tessera 0.0.1\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    tessera(temp.path())
        .env("PATH", path)
        .args(["find", "findById"])
        .assert()
        .success();
    assert!(!marker.exists(), "find ran the tessera found on PATH");
}

#[test]
fn refuses_to_index_the_home_directory() {
    let home = TempDir::new().unwrap();
    fs::write(
        home.path().join("notes.ts"),
        "export function note() { return 1; }\n",
    )
    .unwrap();
    tessera(home.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(["find", "note"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--root"));
    assert!(!home.path().join(".tessera").exists());
}

#[test]
fn missing_or_non_directory_root_is_usage_error() {
    let temp = TempDir::new().unwrap();
    let missing = temp.path().join("does/not/exist");
    tessera(temp.path())
        .args(["--root", missing.to_str().unwrap(), "status"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no such directory"));
    assert!(!missing.exists(), "missing --root must not create the tree");

    tessera(temp.path())
        .args(["--root", missing.to_str().unwrap(), "find", "x"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no such directory"));
    assert!(!missing.exists());

    let file = temp.path().join("some-file.ts");
    fs::write(&file, "export const x = 1;\n").unwrap();
    tessera(temp.path())
        .args(["--root", file.to_str().unwrap(), "find", "x"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not a directory"));

    let index_missing = temp.path().join("typo-dir/proj");
    tessera(temp.path())
        .args(["index", index_missing.to_str().unwrap()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no such directory"));
    assert!(
        !index_missing.exists(),
        "index of a missing path must not create the tree"
    );
}

fn git_in(dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

const FORMAT_V2: &str = "export function formatName(name: string): string {\n  return name.trim().toLowerCase();\n}\n\nexport function loadUser(id: string): string {\n  return formatName(id);\n}\n";

#[test]
fn find_body_reads_latin1_bom_and_moved_repositories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    copy_dir(&fixture_dir("typescript"), &root);
    fs::write(
        root.join("src/latin.py"),
        b"# caf\xe9\ndef cafe():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        root.join("src/bom.ts"),
        "\u{feff}export function bom() { return 1; }\n",
    )
    .unwrap();
    let body_of = |name: &str| {
        let output = tessera(&root)
            .args(["--json", "find", name, "--body"])
            .output()
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        value["matches"][0]["body"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert!(body_of("cafe").contains("return 1"));
    assert!(body_of("bom").starts_with("export function bom"));
    let moved = temp.path().join("moved");
    fs::rename(&root, &moved).unwrap();
    tessera(&moved)
        .args(["find", "findById", "--body"])
        .assert()
        .success()
        .stdout(predicate::str::contains("this.lookup(id)"));
    tessera(&moved)
        .args(["context", "findById"])
        .assert()
        .success()
        .stdout(predicate::str::contains("this.lookup(id)"));
}

#[test]
fn blast_diff_without_to_includes_uncommitted_changes() {
    let temp = repo("typescript");
    git_in(temp.path(), &["init", "-q"]);
    git_in(temp.path(), &["add", "."]);
    git_in(temp.path(), &["commit", "-q", "-m", "one"]);
    fs::write(temp.path().join("src/util/format.ts"), FORMAT_V2).unwrap();
    tessera(temp.path())
        .args(["blast", "--diff", "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("formatName"));
}

#[test]
fn blast_diff_works_when_the_root_is_a_subdirectory_of_the_repository() {
    let temp = TempDir::new().unwrap();
    let app = temp.path().join("pkg/app");
    copy_dir(&fixture_dir("typescript"), &app);
    fs::create_dir_all(app.join(".tessera")).unwrap();
    git_in(temp.path(), &["init", "-q"]);
    git_in(temp.path(), &["add", "."]);
    git_in(temp.path(), &["commit", "-q", "-m", "one"]);
    fs::write(app.join("src/util/format.ts"), FORMAT_V2).unwrap();
    git_in(temp.path(), &["commit", "-q", "-am", "two"]);
    tessera(&app)
        .args(["blast", "--diff", "HEAD~1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("formatName"));
}

#[test]
fn verify_handles_non_ascii_text_before_a_member_call() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["verify", "--snippet", "-", "--language", "typescript"])
        .write_stdin("const s = \"ééééé\"; const t = JSON.stringify(s);\n")
        .assert()
        .code(0);
}

#[test]
fn status_counts_files_with_syntax_errors() {
    let temp = repo("typescript");
    fs::write(
        temp.path().join("src/broken.ts"),
        "export function ok() { return 1; }\nexport function broken( {\n",
    )
    .unwrap();
    tessera(temp.path()).args(["find", "ok"]).assert().success();
    let output = tessera(temp.path())
        .args(["--json", "status"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["parse_errors"], 1, "{value}");
    tessera(temp.path())
        .arg("status")
        .assert()
        .stdout(predicate::str::contains("1 file has syntax errors"));
}

#[test]
fn tessera_db_environment_variable_sets_the_index_path() {
    let temp = repo("typescript");
    let elsewhere = TempDir::new().unwrap();
    let db = elsewhere.path().join("index.db");
    tessera(temp.path())
        .env("TESSERA_DB", &db)
        .args(["find", "findById"])
        .assert()
        .success();
    assert!(db.exists());
    assert!(!temp.path().join(".tessera").exists());
}

#[test]
fn init_names_a_custom_index_in_mcp_configs() {
    let temp = repo("go");
    tessera(temp.path())
        .args([
            "--db",
            "we\"ird\\dir/t.db",
            "init",
            "--yes",
            "--agent",
            "claude",
        ])
        .assert()
        .success();
    let claude: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".mcp.json")).unwrap()).unwrap();
    let args = claude["mcpServers"]["tessera"]["args"].as_array().unwrap();
    // The launcher is `tessera` or `npx -y tessera-codegraph@…`, by PATH.
    let db = args.iter().position(|a| a == "--db").expect("--db in args");
    assert!(args[db + 1]
        .as_str()
        .unwrap()
        .ends_with("we\"ird\\dir/t.db"));
}

#[cfg(unix)]
#[test]
fn init_git_hooks_are_executable_and_work_in_worktrees() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let main = temp.path().join("main");
    copy_dir(&fixture_dir("go"), &main);
    git_in(&main, &["init", "-q"]);
    git_in(&main, &["add", "."]);
    git_in(&main, &["commit", "-q", "-m", "one"]);
    git_in(&main, &["worktree", "add", "-q", "../wt"]);
    for dir in [main.clone(), temp.path().join("wt")] {
        tessera(&dir)
            .args(["init", "--yes", "--git-hook"])
            .assert()
            .success();
        let hooks = StdCommand::new("git")
            .args(["rev-parse", "--path-format=absolute", "--git-path", "hooks"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let hooks = std::path::PathBuf::from(String::from_utf8_lossy(&hooks.stdout).trim());
        let mode = fs::metadata(hooks.join("pre-commit"))
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "hook mode {mode:o}");
    }
}

#[test]
fn project_config_can_be_committed() {
    let temp = repo("typescript");
    git_in(temp.path(), &["init", "-q"]);
    tessera(temp.path())
        .args(["init", "--yes"])
        .assert()
        .success();
    tessera(temp.path())
        .args(["find", "findById"])
        .assert()
        .success();
    let ignored = |path: &str| {
        StdCommand::new("git")
            .args(["check-ignore", "-q", path])
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success()
    };
    assert!(!ignored(".tessera/config.toml"));
    assert!(ignored(".tessera/tessera.db"));
}

#[test]
fn blast_diff_does_not_depend_on_the_users_diff_prefix_config() {
    for config in [
        &[][..],
        &[("diff.mnemonicPrefix", "true")][..],
        &[("diff.noprefix", "true")][..],
        &[("diff.srcPrefix", "left/"), ("diff.dstPrefix", "right/")][..],
    ] {
        let temp = repo("typescript");
        git_in(temp.path(), &["init", "-q"]);
        git_in(temp.path(), &["add", "."]);
        git_in(temp.path(), &["commit", "-q", "-m", "one"]);
        for (key, value) in config {
            git_in(temp.path(), &["config", key, value]);
        }
        fs::write(temp.path().join("src/util/format.ts"), FORMAT_V2).unwrap();
        tessera(temp.path())
            .args(["blast", "--diff", "HEAD"])
            .assert()
            .success()
            .stdout(predicate::str::contains("formatName"));
    }
}

#[test]
fn blast_diff_keeps_a_top_level_directory_named_a_or_b() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("a");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("m.ts"), "export function one() { return 1; }\n").unwrap();
    git_in(temp.path(), &["init", "-q"]);
    git_in(temp.path(), &["config", "diff.noprefix", "true"]);
    git_in(temp.path(), &["add", "."]);
    git_in(temp.path(), &["commit", "-q", "-m", "one"]);
    fs::write(dir.join("m.ts"), "export function one() { return 2; }\n").unwrap();
    let output = tessera(temp.path())
        .args(["--json", "blast", "--diff", "HEAD"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["changed"][0]["name"], "one", "{value}");
}

/// `a` (lines 1-20), `b` (21-30) and `c` (31-40), after `header` and before `trailer`.
fn m_ts(header: &str, trailer: &str) -> String {
    let function = |name: &str, body_lines: usize| {
        let mut text = format!("export function {name}() {{\n");
        for i in 0..body_lines {
            text.push_str(&format!("  const {name}{i} = {i};\n"));
        }
        text.push_str("}\n");
        text
    };
    format!(
        "{header}{}{}{}{trailer}",
        function("a", 18),
        function("b", 8),
        function("c", 8)
    )
}

fn deletion_repo(header: &str, trailer: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/m.ts"), m_ts(header, trailer)).unwrap();
    fs::write(
        temp.path().join("src/use.ts"),
        "import { b, c } from \"./m\";\nexport function useB() { return b(); }\nexport function useC() { return c(); }\n",
    )
    .unwrap();
    git_in(temp.path(), &["init", "-q"]);
    git_in(temp.path(), &["add", "."]);
    git_in(temp.path(), &["commit", "-q", "-m", "one"]);
    temp
}

fn changed_names(root: &Path) -> (Vec<String>, usize) {
    let output = tessera(root)
        .args(["--json", "blast", "--diff", "HEAD"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut names: Vec<String> = value["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    (names, value["callers"].as_array().unwrap().len())
}

fn drop_lines(root: &Path, from: usize, to: usize) {
    let path = root.join("src/m.ts");
    let kept: Vec<String> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .enumerate()
        .filter(|(i, _)| !(from..=to).contains(&(i + 1)))
        .map(|(_, line)| format!("{line}\n"))
        .collect();
    fs::write(path, kept.concat()).unwrap();
}

#[test]
fn a_pure_deletion_flags_the_symbol_it_happened_in() {
    let temp = deletion_repo("", "");
    drop_lines(temp.path(), 3, 17);
    assert_eq!(changed_names(temp.path()), (vec!["a".to_string()], 0));
}

#[test]
fn a_deletion_at_the_top_or_end_of_a_file_flags_the_adjacent_symbol() {
    let comments = |n: usize| "// note\n".repeat(n);
    let temp = deletion_repo(&comments(25), "");
    drop_lines(temp.path(), 1, 25);
    assert_eq!(changed_names(temp.path()).0, vec!["a".to_string()]);

    let temp = deletion_repo("", &comments(25));
    drop_lines(temp.path(), 41, 65);
    assert_eq!(changed_names(temp.path()).0, vec!["c".to_string()]);
}

#[test]
fn a_large_deletion_before_a_small_edit_flags_both_symbols_and_nothing_else() {
    let temp = deletion_repo("", "");
    drop_lines(temp.path(), 3, 17);
    // `c` now spans lines 16-25.
    edit_line(temp.path(), "const c3 = 3;", "const c3 = 33;");
    assert_eq!(
        changed_names(temp.path()),
        (vec!["a".to_string(), "c".to_string()], 1)
    );
}

fn edit_line(root: &Path, from: &str, to: &str) {
    let path = root.join("src/m.ts");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from));
    fs::write(path, text.replacen(from, to, 1)).unwrap();
}

fn write_repo(files: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().unwrap();
    for (path, body) in files {
        let full = temp.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    temp
}

fn json_of(cwd: &Path, args: &[&str]) -> serde_json::Value {
    let out = tessera(cwd).args(args).arg("--json").output().unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out.stdout)))
}

const CASE_REPO: &[(&str, &str)] = &[(
    "src/a.ts",
    "export class A { getUser() { return 1 } }\n\
     export class B { GetUser() { return 2 } }\n\
     export class C { getXuser() { return 3 } }\n",
)];

fn matches_of(cwd: &Path, symbol: &str) -> Vec<(String, String)> {
    json_of(cwd, &["find", symbol])["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["symbol"]["qualified_name"].as_str().unwrap().to_string(),
                m["match_kind"]["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn symbol_lookup_is_case_sensitive() {
    let temp = write_repo(CASE_REPO);
    let exact = |symbol: &str| -> Vec<String> {
        matches_of(temp.path(), symbol)
            .into_iter()
            .filter(|(_, kind)| kind == "exact")
            .map(|(name, _)| name)
            .collect()
    };
    assert_eq!(exact("getUser"), ["A.getUser"]);
    assert_eq!(exact("B.GetUser"), ["B.GetUser"]);
    assert!(exact("getuser").is_empty(), "{:?}", exact("getuser"));
    assert!(exact("a.getUser").is_empty());
}

#[test]
fn underscore_in_a_symbol_name_is_not_a_wildcard() {
    let temp = write_repo(CASE_REPO);
    assert!(!matches_of(temp.path(), "get_user")
        .iter()
        .any(|(name, kind)| name == "C.getXuser" && kind == "exact"));
    // context must not return the body of a different method either
    tessera(temp.path())
        .args(["context", "get_user"])
        .assert()
        .success()
        .stdout(predicate::str::contains("getXuser").not());
}

#[test]
fn verify_reports_a_wrong_case_name_as_missing() {
    let temp = write_repo(CASE_REPO);
    tessera(temp.path())
        .args(["verify", "getuser"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("getuser"))
        .stdout(predicate::str::contains("A.getUser"));
    tessera(temp.path())
        .args(["verify", "--snippet", "-", "--language", "typescript"])
        .write_stdin("export function go() { return getuser(); }\n")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("V1"));
    tessera(temp.path())
        .args(["verify", "getUser"])
        .assert()
        .success();
}

const SCOPE_REPO: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "pub struct Index;\n\
         impl Index { pub fn find(&self) -> i32 { 1 } }\n\
         pub struct Other;\n\
         impl Other { pub fn find(&self) -> i32 { 2 } }\n\
         pub fn caller() -> i32 { Index.find() }\n",
    ),
    (
        "lib/billing.rb",
        "module Billing\n  class Invoice\n    def total\n      1\n    end\n  end\nend\n",
    ),
];

/// A result with the echoed query removed, so two spellings compare equal.
fn without_query(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(map) = value.as_object_mut() {
        for key in ["query", "target", "from", "to"] {
            map.remove(key);
        }
    }
    value
}

#[test]
fn double_colon_names_resolve_like_dotted_names() {
    let temp = write_repo(SCOPE_REPO);
    let cwd = temp.path();
    for cmd in [&["find"][..], &["context"], &["blast"], &["refs"]] {
        let dotted: Vec<&str> = cmd.iter().copied().chain(["Index.find"]).collect();
        let colons: Vec<&str> = cmd.iter().copied().chain(["Index::find"]).collect();
        let expected = without_query(json_of(cwd, &dotted));
        assert_eq!(
            without_query(json_of(cwd, &colons)),
            expected,
            "{cmd:?} differs between Index.find and Index::find"
        );
    }
    let dotted = without_query(json_of(cwd, &["path", "caller", "Index.find"]));
    assert_eq!(
        without_query(json_of(cwd, &["path", "caller", "Index::find"])),
        dotted
    );
    assert!(dotted.to_string().contains("Index.find"));
}

#[test]
fn double_colon_find_is_an_exact_match() {
    let temp = write_repo(SCOPE_REPO);
    assert_eq!(
        matches_of(temp.path(), "Index::find"),
        [("Index.find".to_string(), "exact".to_string())]
    );
}

#[test]
fn blast_of_a_scoped_name_does_not_widen_to_other_owners() {
    let temp = write_repo(SCOPE_REPO);
    let refs = json_of(temp.path(), &["refs", "Index::find"]);
    assert_eq!(refs["definitions"], 1, "{refs}");
}

#[test]
fn hash_and_double_colon_name_a_ruby_method() {
    let temp = write_repo(SCOPE_REPO);
    for name in [
        "Billing::Invoice#total",
        "Billing::Invoice.total",
        "Invoice#total",
    ] {
        assert_eq!(
            matches_of(temp.path(), name)
                .into_iter()
                .map(|(n, k)| (n, k == "exact"))
                .collect::<Vec<_>>(),
            [("Billing.Invoice.total".to_string(), true)],
            "{name}"
        );
        tessera(temp.path())
            .args(["context", name])
            .assert()
            .success()
            .stdout(predicate::str::contains("Billing.Invoice.total"));
    }
}
