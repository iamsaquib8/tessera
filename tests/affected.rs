//! `tessera affected`: the tests a change reaches, each with a reason, and
//! the safety rails that widen the selection.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::{json, Value};
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;
use tessera_codegraph::query::AffectedArgs;
use tessera_codegraph::{Index, IndexOptions};

const I18N_TEST: &str =
    "import { greet } from \"../src/util/i18n\";\n\ntest(\"greets\", () => {\n  expect(greet()).toBe(\"héllo\");\n});\n";

fn temp() -> TempDir {
    tempfile::Builder::new()
        .prefix("tessera-affected-")
        .tempdir()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn write(root: &Path, path: &str, content: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let file = root.join(path);
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains(from), "{path} lacks {from:?}");
    fs::write(&file, content.replacen(from, to, 1)).unwrap();
}

/// A fixture plus `extra` files, committed.
fn repo(fixture: &str, extra: &[(&str, &str)]) -> TempDir {
    let temp = temp();
    copy_dir(&fixture_dir(fixture), temp.path());
    for (path, content) in extra {
        write(temp.path(), path, content);
    }
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-q", "-m", "base"]);
    temp
}

/// The TypeScript fixture with a second test that reaches only `greet`.
fn ts_repo() -> TempDir {
    repo("typescript", &[("test/i18n.test.ts", I18N_TEST)])
}

/// One package per ecosystem, each with a test that reaches one function.
fn polyglot() -> TempDir {
    let temp = temp();
    let files: &[(&str, &str)] = &[
        ("web/package.json", "{ \"name\": \"@acme/web\", \"private\": true }\n"),
        ("web/src/math.ts", "export function add(a: number, b: number): number {\n  return a + b;\n}\n"),
        ("web/test/math.test.ts", "import { add } from \"../src/math\";\n\ntest(\"adds\", () => {\n  expect(add(1, 2)).toBe(3);\n});\n"),
        ("web/test/other.test.ts", "test(\"other\", () => {\n  expect(1).toBe(1);\n});\n"),
        ("py/pyproject.toml", "[project]\nname = \"acme-py\"\nversion = \"0.1.0\"\n"),
        ("py/app/__init__.py", ""),
        ("py/app/calc.py", "def double(x):\n    return x * 2\n"),
        ("py/tests/test_calc.py", "from app.calc import double\n\n\nclass TestCalc:\n    def test_double(self):\n        assert double(2) == 4\n\n\ndef test_other():\n    assert True\n"),
        ("go/go.mod", "module example.com/acme\n\ngo 1.22\n"),
        ("go/calc/calc.go", "package calc\n\nfunc Triple(x int) int {\n\treturn x * 3\n}\n"),
        ("go/calc/calc_test.go", "package calc\n\nimport \"testing\"\n\nfunc TestTriple(t *testing.T) {\n\tif Triple(1) != 3 {\n\t\tt.Fatal(\"triple\")\n\t}\n}\n\nfunc TestNothing(t *testing.T) {}\n"),
        ("rs/Cargo.toml", "[package]\nname = \"acme-rs\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        ("rs/src/lib.rs", "pub fn quad(x: i32) -> i32 {\n    x * 4\n}\n\nfn half(x: i32) -> i32 {\n    x / 2\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn halves() {\n        let got = half(4);\n        assert_eq!(got, 2);\n    }\n}\n"),
        ("rs/tests/quad.rs", "use acme_rs::quad;\n\n#[test]\nfn quads() {\n    let got = quad(1);\n    assert_eq!(got, 4);\n}\n"),
        ("java/pom.xml", "<project>\n  <parent>\n    <artifactId>acme-parent</artifactId>\n  </parent>\n  <artifactId>acme-java</artifactId>\n</project>\n"),
        ("java/src/main/java/acme/Calc.java", "package acme;\n\npublic class Calc {\n  public static int neg(int x) {\n    return -x;\n  }\n}\n"),
        ("java/src/test/java/acme/CalcTest.java", "package acme;\n\nclass CalcTest {\n  void negates() {\n    Calc.neg(1);\n  }\n}\n"),
    ];
    for (path, content) in files {
        write(temp.path(), path, content);
    }
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-q", "-m", "base"]);
    temp
}

/// Edits the function each polyglot test reaches.
fn edit_polyglot(root: &Path) {
    edit(root, "web/src/math.ts", "a + b", "b + a");
    edit(root, "py/app/calc.py", "x * 2", "2 * x");
    edit(root, "go/calc/calc.go", "x * 3", "3 * x");
    edit(root, "rs/src/lib.rs", "x * 4", "4 * x");
    edit(
        root,
        "java/src/main/java/acme/Calc.java",
        "return -x;",
        "return 0 - x;",
    );
}

fn tessera(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("tessera").unwrap();
    cmd.current_dir(root).env_remove("TESSERA_DB");
    cmd
}

/// `(exit code, stdout, stderr)` of `tessera --quiet affected <args>`.
fn affected(root: &Path, args: &[&str]) -> (i32, String, String) {
    let output = tessera(root)
        .arg("--quiet")
        .arg("affected")
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn affected_json(root: &Path, args: &[&str]) -> Value {
    let (code, stdout, stderr) = affected(root, &[args, &["--format", "json"]].concat());
    assert_eq!(code, 0, "{stderr}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}\n{stderr}"))
}

/// `path kind` for each selected test.
fn selected(value: &Value) -> Vec<String> {
    value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            format!(
                "{} {}",
                t["path"].as_str().unwrap(),
                t["kind"].as_str().unwrap()
            )
        })
        .collect()
}

fn test<'a>(value: &'a Value, path: &str) -> &'a Value {
    value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["path"] == path)
        .unwrap_or_else(|| panic!("{path} not selected: {value:#}"))
}

fn chain(test: &Value) -> Vec<String> {
    test["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["symbol"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_changed_function_selects_the_tests_that_reach_it_with_the_path() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    let value = affected_json(root, &[]);
    assert_eq!(selected(&value), vec!["test/repo.test.ts reached"]);
    let repo = test(&value, "test/repo.test.ts");
    assert_eq!(
        chain(repo),
        vec![
            "describe(\"loadUser\").it(\"returns the user\")",
            "loadUser",
            "UserRepo.findById",
            "formatName"
        ]
    );
    assert_eq!(repo["changed"], "formatName");
    let reason = repo["reason"].as_str().unwrap();
    assert!(
        reason.contains("loadUser → UserRepo.findById → formatName"),
        "{reason}"
    );
    assert_eq!(
        value["totals"],
        json!({ "selected": 1, "total": 2, "skipped": 1 })
    );
    assert!(value["full_run"].is_null());

    let (code, stdout, _) = affected(root, &[]);
    assert_eq!((code, stdout.as_str()), (0, "test/repo.test.ts\n"));
}

#[test]
fn new_and_changed_tests_always_run() {
    let temp = ts_repo();
    let root = temp.path();
    edit(root, "test/i18n.test.ts", "\"greets\"", "\"says hello\"");
    write(
        root,
        "test/new.test.ts",
        "test(\"new\", () => {\n  expect(2).toBe(2);\n});\n",
    );
    let value = affected_json(root, &[]);
    assert_eq!(
        selected(&value),
        vec!["test/i18n.test.ts changed", "test/new.test.ts changed"]
    );
    let new = test(&value, "test/new.test.ts")["reason"].as_str().unwrap();
    assert!(new.contains("new"), "{new}");
    assert_eq!(value["totals"]["total"], 3);
}

#[test]
fn a_renamed_function_selects_the_tests_of_its_broken_callers() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/format.ts",
        "export function formatName(",
        "export function formatDisplayName(",
    );
    let value = affected_json(root, &[]);
    assert_eq!(selected(&value), vec!["test/repo.test.ts reached"]);
    let repo = test(&value, "test/repo.test.ts");
    assert_eq!(chain(repo).last().unwrap(), "formatName");
    let reason = repo["reason"].as_str().unwrap();
    assert!(
        reason.contains("UserRepo.findById → formatName"),
        "{reason}"
    );
}

#[test]
fn a_deleted_file_selects_the_tests_of_its_importers() {
    let temp = ts_repo();
    let root = temp.path();
    fs::remove_file(root.join("src/util/format.ts")).unwrap();
    let value = affected_json(root, &[]);
    assert_eq!(selected(&value), vec!["test/repo.test.ts reached"]);
}

#[test]
fn a_changed_route_handler_selects_the_client_tests_in_another_service() {
    let client_test = "import { loadUser } from \"../src/client\";\n\ntest(\"loads\", async () => {\n  await loadUser(\"1\");\n});\n";
    let temp = repo(
        "contracts-http",
        &[("web/test/client.test.ts", client_test)],
    );
    let root = temp.path();
    edit(
        root,
        "api/src/app.js",
        "res.json({ id: req.params.id });",
        "res.json({ id: req.params.id, ok: true });",
    );
    let value = affected_json(root, &[]);
    assert_eq!(selected(&value), vec!["web/test/client.test.ts reached"]);
    let client = test(&value, "web/test/client.test.ts");
    assert_eq!(
        chain(client),
        vec!["test(\"loads\")", "loadUser", "getUser"]
    );
    assert_eq!(client["chain"][2]["via"], "route GET /api/users/{}");
    let reason = client["reason"].as_str().unwrap();
    assert!(
        reason.contains("loadUser → getUser via route GET /api/users/{}"),
        "{reason}"
    );
    assert_eq!(client["package"], "web");
}

#[test]
fn a_changed_client_selects_the_tests_of_the_service_it_calls() {
    let temp = repo("contracts-http", &[]);
    let root = temp.path();
    edit(
        root,
        "billing/src/main/java/billing/Invoices.java",
        "\"http://owners-service/owners/\" + id",
        "\"http://owners-service/owners/\" + (id + 0)",
    );
    let value = affected_json(root, &[]);
    assert_eq!(
        selected(&value),
        vec!["owners/src/test/java/petclinic/OwnerControllerTests.java reached"]
    );
    let owners = test(
        &value,
        "owners/src/test/java/petclinic/OwnerControllerTests.java",
    );
    assert_eq!(
        chain(owners),
        vec![
            "OwnerControllerTests.showsOwner",
            "OwnerController.showOwner",
            "Invoices.owner"
        ]
    );
    let reason = owners["reason"].as_str().unwrap();
    assert!(reason.contains("via route GET /owners/{}"), "{reason}");
}

#[test]
fn lockfiles_build_files_config_and_fixtures_run_the_full_suite() {
    let temp = ts_repo();
    let root = temp.path();
    for path in [
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "Cargo.lock",
        "go.sum",
        "poetry.lock",
        "uv.lock",
        "Gemfile.lock",
        "pom.xml",
        "build.gradle.kts",
        "package.json",
        "Cargo.toml",
        "go.mod",
        "pyproject.toml",
        "requirements-dev.txt",
        "tsconfig.build.json",
        "jest.config.js",
        "vitest.config.ts",
        "pytest.ini",
        ".tessera/config.toml",
        ".github/workflows/ci.yml",
        "test/fixtures/user.json",
        "test/__snapshots__/repo.test.ts.snap",
        "pkg/testdata/input.txt",
    ] {
        write(root, path, "# changed\n");
        let value = affected_json(root, &[]);
        let reason = value["full_run"]
            .as_str()
            .unwrap_or_else(|| panic!("{path}: {value:#}"));
        assert!(reason.contains(path), "{path}: {reason}");
        assert_eq!(
            value["totals"],
            json!({ "selected": 2, "total": 2, "skipped": 0 }),
            "{path}"
        );
        assert!(
            selected(&value).iter().all(|t| t.ends_with(" full-run")),
            "{path}: {value:#}"
        );
        fs::remove_file(root.join(path)).unwrap();
    }
    write(root, "README.md", "# changed\n");
    write(root, "docs/notes.md", "notes\n");
    let value = affected_json(root, &[]);
    assert!(value["full_run"].is_null(), "{value:#}");
    assert_eq!(value["totals"]["selected"], 0);
}

#[test]
fn a_no_skip_trailer_in_the_range_selects_everything() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/i18n.ts",
        "return \"héllo\";",
        "return \"hello\";",
    );
    git(
        root,
        &[
            "commit",
            "-q",
            "-am",
            "docs: mention Tessera-No-Skip: in the subject only",
        ],
    );
    let value = affected_json(root, &["HEAD~1"]);
    assert!(value["full_run"].is_null(), "{value:#}");
    assert_eq!(selected(&value), vec!["test/i18n.test.ts reached"]);

    edit(
        root,
        "src/util/i18n.ts",
        "return \"hello\";",
        "return \"hi\";",
    );
    git(
        root,
        &["commit", "-q", "-am", "fix: tweak the greeting\n\nIt is used everywhere.\n\nTessera-No-Skip: shared copy\nRefs: #12"],
    );
    let value = affected_json(root, &["HEAD~2"]);
    let reason = value["full_run"].as_str().unwrap();
    assert!(reason.contains("Tessera-No-Skip"), "{reason}");
    assert_eq!(
        value["totals"],
        json!({ "selected": 2, "total": 2, "skipped": 0 })
    );
}

#[test]
fn previously_failed_tests_always_run() {
    let temp = ts_repo();
    let root = temp.path();
    let lists = temp_dir_outside();
    let failed = lists.path().join("failed.txt");
    fs::write(
        &failed,
        "test/i18n.test.ts\n\n# from the last run\n./test/repo.test.ts\ntest/gone.test.ts\n",
    )
    .unwrap();
    let value = affected_json(root, &["--failed", failed.to_str().unwrap()]);
    assert_eq!(
        selected(&value),
        vec!["test/i18n.test.ts always", "test/repo.test.ts always"]
    );
    let notes = value["notes"].to_string();
    assert!(notes.contains("test/gone.test.ts"), "{notes}");

    let missing = lists.path().join("missing.txt");
    let (code, _, stderr) = affected(root, &["--failed", missing.to_str().unwrap()]);
    assert_eq!(code, 2, "{stderr}");
}

fn temp_dir_outside() -> TempDir {
    tempfile::Builder::new()
        .prefix("tessera-affected-lists-")
        .tempdir()
        .unwrap()
}

#[test]
fn explain_says_why_a_test_is_or_is_not_selected() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    let (code, stdout, stderr) = affected(root, &["--explain", "test/repo.test.ts"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.starts_with("test/repo.test.ts is selected"),
        "{stdout}"
    );
    assert!(
        stdout.contains("loadUser → UserRepo.findById → formatName"),
        "{stdout}"
    );

    let (_, stdout, _) = affected(root, &["--explain", "./test/i18n.test.ts"]);
    assert_eq!(
        stdout,
        "test/i18n.test.ts is not selected: no path from the changed symbols within depth 6\n"
    );
    let (_, stdout, _) = affected(root, &["--explain", "test/i18n.test.ts", "--depth", "2"]);
    assert!(stdout.contains("within depth 2"), "{stdout}");
    let (_, stdout, _) = affected(root, &["--explain", "src/util/format.ts"]);
    assert!(stdout.contains("is not a test file"), "{stdout}");

    let value = affected_json(root, &["--explain", "test/repo.test.ts"]);
    assert_eq!(value["explain"]["selected"], true);
    assert_eq!(value["explain"]["test"], "test/repo.test.ts");
}

#[test]
fn runner_formats_name_the_tests_each_runner_takes() {
    let temp = polyglot();
    let root = temp.path();
    edit_polyglot(root);
    let (code, stdout, stderr) = affected(root, &[]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        "go/calc/calc_test.go\njava/src/test/java/acme/CalcTest.java\npy/tests/test_calc.py\nrs/tests/quad.rs\nweb/test/math.test.ts\n"
    );
    for (format, expected) in [
        ("jest", "jest --runTestsByPath web/test/math.test.ts\n"),
        ("vitest", "vitest run web/test/math.test.ts\n"),
        (
            "pytest",
            "pytest py/tests/test_calc.py::TestCalc::test_double\n",
        ),
        ("go-test", "go -C go test ./calc -run '^(TestTriple)$'\n"),
        ("cargo", "cargo test -p acme-rs --test quad\n"),
    ] {
        let (code, stdout, stderr) = affected(root, &["--format", format]);
        assert_eq!(code, 0, "{format}: {stderr}");
        assert_eq!(stdout, expected, "{format}");
    }
    let value = affected_json(root, &[]);
    for (path, package) in [
        ("web/test/math.test.ts", "@acme/web"),
        ("py/tests/test_calc.py", "acme-py"),
        ("go/calc/calc_test.go", "example.com/acme"),
        ("rs/tests/quad.rs", "acme-rs"),
        ("java/src/test/java/acme/CalcTest.java", "acme-java"),
    ] {
        assert_eq!(test(&value, path)["package"], package, "{path}");
    }

    // A unit test inside the crate's sources runs the whole package.
    edit(root, "rs/src/lib.rs", "x / 2", "x >> 1");
    let (_, stdout, _) = affected(root, &["--format", "cargo"]);
    assert_eq!(stdout, "cargo test -p acme-rs\n");
    let value = affected_json(root, &[]);
    assert_eq!(test(&value, "rs/src/lib.rs")["kind"], "reached");

    // A full run lists whole files and packages.
    write(root, "web/package-lock.json", "{}\n");
    for (format, expected) in [
        (
            "jest",
            "jest --runTestsByPath web/test/math.test.ts web/test/other.test.ts\n",
        ),
        ("pytest", "pytest py/tests/test_calc.py\n"),
        ("go-test", "go -C go test ./calc\n"),
        ("cargo", "cargo test -p acme-rs\n"),
    ] {
        let (_, stdout, _) = affected(root, &["--format", format]);
        assert_eq!(stdout, expected, "full run {format}");
    }
}

#[test]
fn package_restricts_the_selection_to_one_workspace_package() {
    let temp = polyglot();
    let root = temp.path();
    edit_polyglot(root);
    let (code, stdout, stderr) = affected(root, &["--package", "@acme/web"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "web/test/math.test.ts\n");
    let value = affected_json(root, &["--package", "@acme/web"]);
    assert_eq!(
        value["totals"],
        json!({ "selected": 1, "total": 2, "skipped": 1 })
    );

    let (_, stdout, _) = affected(root, &["--package", "acme-java"]);
    assert_eq!(stdout, "java/src/test/java/acme/CalcTest.java\n");

    let (code, _, stderr) = affected(root, &["--package", "no-such-package"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("no-such-package"), "{stderr}");
}

#[test]
fn staged_selects_for_the_staged_changes_only() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    git(root, &["add", "src/util/format.ts"]);
    edit(
        root,
        "src/util/i18n.ts",
        "return \"héllo\";",
        "return \"hello\";",
    );
    let (code, stdout, stderr) = affected(root, &["--staged"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "test/repo.test.ts\n");
}

#[test]
fn no_tests_no_git_and_a_detached_head() {
    let temp = temp();
    let root = temp.path();
    write(root, "src/a.ts", "export const a = 1;\n");
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    edit(root, "src/a.ts", "= 1", "= 2");
    let output = tessera(root).arg("affected").output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no test files"), "{stderr}");
    let value = affected_json(root, &[]);
    assert!(
        value["notes"].to_string().contains("no test files"),
        "{value:#}"
    );
    assert_eq!(value["totals"]["total"], 0);

    let plain = temp_dir_outside();
    copy_dir(&fixture_dir("typescript"), plain.path());
    let (code, _, stderr) = affected(plain.path(), &[]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("git repository"), "{stderr}");

    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/i18n.ts",
        "return \"héllo\";",
        "return \"hello\";",
    );
    git(root, &["commit", "-q", "-am", "second"]);
    git(root, &["checkout", "-q", "--detach", "HEAD~1"]);
    edit(
        root,
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    for args in [&[][..], &["HEAD"]] {
        let (code, stdout, stderr) = affected(root, args);
        assert_eq!(
            (code, stdout.as_str()),
            (0, "test/repo.test.ts\n"),
            "{stderr}"
        );
    }
    let (code, _, stderr) = affected(root, &["no-such-rev"]);
    assert_eq!(code, 2, "{stderr}");
}

#[test]
fn output_is_byte_identical_across_runs() {
    let temp = polyglot();
    let root = temp.path();
    edit_polyglot(root);
    write(
        root,
        "py/tests/test_more.py",
        "def test_more():\n    assert True\n",
    );
    for args in [&["--format", "json"][..], &[], &["--format", "go-test"]] {
        let first = affected(root, args);
        let second = affected(root, args);
        assert_eq!(first, second, "{args:?}");
    }
}

#[test]
fn the_library_pages_the_selection() {
    let temp = ts_repo();
    let root = temp.path();
    let dir = temp_dir_outside();
    let db = dir.path().join("tessera.db");
    Index::build(root, &db, IndexOptions::default()).unwrap();
    let index = Index::open(&db).unwrap();
    let args = AffectedArgs {
        failed: vec!["test/i18n.test.ts".into(), "test/repo.test.ts".into()],
        limit: 1,
        ..AffectedArgs::default()
    };
    let result = index.affected(&args).unwrap();
    assert_eq!(result.tests.len(), 1);
    assert_eq!(result.page.total, 2);
    assert_eq!(result.totals.selected, 2);
    assert!(result.page.next_cursor.is_some());
}

#[test]
fn the_mcp_tool_selects_and_explains() {
    let temp = ts_repo();
    let root = temp.path();
    edit(
        root,
        "src/util/format.ts",
        "name.trim()",
        "name.trim().toLowerCase()",
    );
    let dir = temp_dir_outside();
    let db = dir.path().join("tessera.db");
    Index::build(root, &db, IndexOptions::default()).unwrap();
    let index = Index::open(&db).unwrap();
    let tool = tessera_codegraph::mcp::registry()
        .iter()
        .find(|tool| tool.name == "affected")
        .expect("an `affected` MCP tool");
    let output = (tool.run)(
        &index,
        json!({ "failed": ["test/i18n.test.ts"], "explain": "test/repo.test.ts" }),
    )
    .unwrap_or_else(|_| panic!("the affected tool failed"));
    assert!(
        output
            .text
            .starts_with("test/repo.test.ts is selected (reached)"),
        "{}",
        output.text
    );
    assert!(
        output.text.contains("2 of 2 test files selected"),
        "{}",
        output.text
    );
    assert_eq!(output.json["totals"]["selected"], 2);

    let tools = tessera_codegraph::mcp::registry();
    let schema = (tools
        .iter()
        .find(|t| t.name == "affected")
        .unwrap()
        .input_schema)();
    assert_eq!(schema["required"], json!([]));
}
