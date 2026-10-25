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

/// The `run` script of the step with `id`, and its `if` condition.
fn step(id: &str) -> (String, Option<String>) {
    let action = repo_file("action/action.yml");
    let docs = yaml_rust2::YamlLoader::load_from_str(&action).unwrap();
    let steps = docs[0]["runs"]["steps"].as_vec().unwrap();
    let step = steps
        .iter()
        .find(|step| step["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("no step with id {id}"));
    (
        step["run"].as_str().unwrap_or_default().to_string(),
        step["if"].as_str().map(str::to_string),
    )
}

fn git(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn the_action_keys_the_index_cache_on_the_merge_base_tree() {
    let action = repo_file("action/action.yml");
    assert!(
        !action.contains("path: .tessera/tessera.db"),
        "the database is moved as a cache file, not copied raw"
    );
    assert!(action.contains("uses: actions/cache/restore@v4"));
    assert!(action.contains("uses: actions/cache/save@v4"));
    assert!(action.contains(
        "key: tessera-${{ runner.os }}-${{ steps.key.outputs.version }}-${{ steps.key.outputs.tree }}"
    ));
    assert!(action.contains("cache import \"$CACHE\""));
    assert!(action.contains("cache export \"$CACHE\""));

    // The key step, run in a repository whose branch is one commit past
    // the merge base, while the base moved on.
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "merge base"]);
    let merge_base_tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    fs::write(root.join("b.ts"), "export const b = 2;\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "change"]);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("c.ts"), "export const c = 3;\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base moved on"]);
    let base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-q", "feature"]);

    let (script, _) = step("key");
    let outputs = root.join("outputs");
    let run = std::process::Command::new("bash")
        .args(["-e", "-c", &script])
        .current_dir(root)
        .env("BASE", &base)
        .env("TESSERA", env!("CARGO_BIN_EXE_tessera"))
        .env("GITHUB_OUTPUT", &outputs)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let outputs = fs::read_to_string(outputs).unwrap();
    assert!(
        outputs.contains(&format!("tree={merge_base_tree}\n")),
        "{outputs}"
    );
    assert!(
        outputs.contains(&format!("version={}\n", env!("CARGO_PKG_VERSION"))),
        "{outputs}"
    );

    // Saved only when the exact key missed, and before the job can fail.
    let (_, export_if) = step("export");
    assert_eq!(
        export_if.as_deref(),
        Some("steps.restore.outputs.cache-hit != 'true'")
    );
    let save = action.find("actions/cache/save@v4").unwrap();
    assert!(save < action.find("Fail on blocking problems").unwrap());
}

#[test]
fn the_action_uploads_sarif_to_code_scanning_when_asked() {
    let action = repo_file("action/action.yml");
    let input = action
        .split("\n  upload-sarif:\n")
        .nth(1)
        .expect("an upload-sarif input");
    assert!(
        input
            .lines()
            .take(3)
            .any(|l| l.trim() == "default: \"false\""),
        "off by default: the token needs security-events: write"
    );
    assert!(action.contains("--format sarif > \"$RUNNER_TEMP/tessera.sarif\""));
    assert!(action.contains("uses: github/codeql-action/upload-sarif@v3"));
    assert!(action.contains("category: tessera"));
    let (_, upload_if) = step("sarif");
    assert_eq!(upload_if.as_deref(), Some("inputs.upload-sarif == 'true'"));
    let readme = repo_file("action/README.md");
    assert!(readme.contains("`upload-sarif`"));
    assert!(readme.contains("security-events: write"));
}

/// `GITHUB_OUTPUT` as the runner reads it: `name=value` lines and
/// `name<<DELIMITER` blocks.
fn outputs(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some((name, delimiter)) = line.split_once("<<") {
            let value: Vec<&str> = lines.by_ref().take_while(|l| *l != delimiter).collect();
            out.insert(name.to_string(), value.join("\n"));
        } else if let Some((name, value)) = line.split_once('=') {
            out.insert(name.to_string(), value.to_string());
        }
    }
    out
}

/// Runs the `affected` step against `base` in `root`.
fn affected_step(root: &std::path::Path, base: &str) -> std::collections::BTreeMap<String, String> {
    let (script, _) = step("affected");
    let runner_temp = tempfile::TempDir::new().unwrap();
    let output = runner_temp.path().join("outputs");
    let run = std::process::Command::new("bash")
        .args(["-e", "-c", &script])
        .current_dir(root)
        .env("BASE", base)
        .env("TESSERA", env!("CARGO_BIN_EXE_tessera"))
        .env("RUNNER_TEMP", runner_temp.path())
        .env("GITHUB_OUTPUT", &output)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let mut values = outputs(&fs::read_to_string(&output).unwrap());
    let file = values.get("tests-file").cloned().unwrap_or_default();
    let listed = fs::read_to_string(&file).unwrap_or_else(|_| panic!("tests-file {file}"));
    assert_eq!(
        listed.trim_end(),
        values["tests"],
        "the file and the output agree"
    );
    values.remove("tests-file");
    values
}

#[test]
fn the_action_outputs_the_tests_to_run_and_when_to_run_them_all() {
    let action = repo_file("action/action.yml");
    for output in ["tests:", "tests-file:", "full-run:"] {
        assert!(
            action.contains(&format!("\n  {output}\n")),
            "output {output}"
        );
    }

    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typescript");
    let copy = std::process::Command::new("cp")
        .arg("-R")
        .arg(fixture.join("."))
        .arg(root)
        .status()
        .unwrap();
    assert!(copy.success());
    fs::write(root.join("package-lock.json"), "{}\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]);

    let file = root.join("src/users/repo.ts");
    let source = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        source.replacen("findById(id: string)", "findOne(id: string)", 1),
    )
    .unwrap();
    let out = affected_step(root, &base);
    assert_eq!(out["tests"], "test/repo.test.ts", "{out:?}");
    assert_eq!(out["full-run"], "", "{out:?}");

    fs::write(root.join("package-lock.json"), "{ \"v\": 2 }\n").unwrap();
    let out = affected_step(root, &base);
    assert!(out["full-run"].contains("lockfile"), "{out:?}");
    assert!(out["tests"].contains("test/repo.test.ts"), "{out:?}");

    // Tessera failing never skips tests: it says to run them all.
    let out = affected_step(root, "no-such-revision");
    assert_eq!(out["tests"], "", "{out:?}");
    assert!(out["full-run"].contains("could not select"), "{out:?}");

    // Nor does a repository whose tests Tessera does not know.
    let bare = tempfile::TempDir::new().unwrap();
    fs::write(bare.path().join("a.ts"), "export const a = 1;\n").unwrap();
    git(bare.path(), &["init", "-q"]);
    git(bare.path(), &["add", "."]);
    git(bare.path(), &["commit", "-q", "-m", "base"]);
    let head = git(bare.path(), &["rev-parse", "HEAD"]);
    fs::write(bare.path().join("a.ts"), "export const a = 2;\n").unwrap();
    let out = affected_step(bare.path(), &head);
    assert!(out["full-run"].contains("no test files"), "{out:?}");
}

fn run_step(id: &str, env: &[(&str, &str)]) -> std::process::Output {
    let (script, _) = step(id);
    let mut command = std::process::Command::new("bash");
    command.args(["-e", "-c", &script]);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

#[test]
fn an_unknown_fail_on_stops_the_job_instead_of_never_failing() {
    for value in ["never", "blocking", "new"] {
        let run = run_step("inputs", &[("FAIL_ON", value)]);
        assert!(run.status.success(), "{value}");
    }
    let run = run_step("inputs", &[("FAIL_ON", "blockng")]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("blockng"));
    // The input check runs before anything else.
    let action = repo_file("action/action.yml");
    let docs = yaml_rust2::YamlLoader::load_from_str(&action).unwrap();
    assert_eq!(docs[0]["runs"]["steps"][0]["id"].as_str(), Some("inputs"));

    // `new` is `blocking`: baselined and accepted findings never block.
    let (_, gate_if) = step("gate");
    assert_eq!(
        gate_if.as_deref(),
        Some("inputs.fail-on != 'never' && steps.report.outputs.blocking == 'true'")
    );
}
