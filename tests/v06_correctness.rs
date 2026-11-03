use std::fs;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn config_include_exclude_and_extra_ignores_shape_index() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".tessera")).unwrap();
    fs::create_dir_all(temp.path().join("src/generated")).unwrap();
    fs::create_dir_all(temp.path().join("vendor")).unwrap();
    fs::write(
        temp.path().join(".tessera/config.toml"),
        r#"
[include]
paths = ["src/"]

[exclude]
paths = ["src/generated/"]

[ignore]
extra = ["vendor"]
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("src/app.ts"),
        "export function keepMe() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("src/generated/app.ts"),
        "export function generated() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("vendor/app.ts"),
        "export function vendored() { return 1; }\n",
    )
    .unwrap();

    let db = temp.path().join(".tessera/config.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let conn = Connection::open(db).unwrap();
    let symbols: Vec<String> = conn
        .prepare("SELECT qualified_name FROM symbols ORDER BY qualified_name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(symbols, vec!["keepMe"]);
}

#[test]
fn unreadable_utf8_source_warns_without_aborting_index() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("ok.ts"),
        "export function stillIndexes() { return 1; }\n",
    )
    .unwrap();
    fs::write(temp.path().join("bad.ts"), [0xff, 0xfe, 0xfd]).unwrap();

    let db = temp.path().join(".tessera/warn.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success()
        .stderr(predicates::str::contains("warning"))
        .stderr(predicates::str::contains("bad.ts"));

    let conn = Connection::open(db).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE qualified_name = 'stillIndexes'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn multibyte_utf8_signature_exceeding_180_bytes_does_not_panic() {
    let temp = TempDir::new().unwrap();
    let prefix_len = 179 - "pub fn test_/* ".len();
    let prefix = "a".repeat(prefix_len);
    let source = format!("pub fn test_{}/* 🦀 */() {{}}\n", prefix);
    assert_eq!(source.find('🦀'), Some(179));
    fs::write(temp.path().join("lib.rs"), source).unwrap();

    let db = temp.path().join(".tessera/multibyte.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let conn = Connection::open(&db).unwrap();
    let sig: String = conn
        .query_row(
            "SELECT signature FROM symbols WHERE name LIKE 'test_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(sig.contains("pub fn test_"));

    // Also verify that signatures exceeding 180 characters truncate with '…' without panicking
    let long_fn = format!("test_long_{}🦀", "b".repeat(200));
    let source2 = format!("pub fn {}() {{}}\n", long_fn);
    fs::write(temp.path().join("long.rs"), source2).unwrap();

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    let sig2: String = conn
        .query_row(
            "SELECT signature FROM symbols WHERE name LIKE 'test_long_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(sig2.ends_with('…'));
    assert!(sig2.chars().count() <= 181);
}

#[test]
fn fixture_snapshot_covers_symbols_refs_imports_edges_and_exports() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("util.ts"),
        "export function helper() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("app.ts"),
        r#"import { helper } from "./util";

export function entry() {
    return helper();
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/snapshot.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let conn = Connection::open(db).unwrap();
    let symbols: Vec<(String, String, bool)> = conn
        .prepare(
            "
            SELECT qualified_name, kind, exported
            FROM symbols
            ORDER BY qualified_name
            ",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        symbols,
        vec![
            ("entry".to_string(), "function".to_string(), true),
            ("helper".to_string(), "function".to_string(), true),
        ]
    );

    let refs: Vec<String> = conn
        .prepare("SELECT symbol_name FROM refs ORDER BY symbol_name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(refs, vec!["helper"]);

    let imports: Vec<String> = conn
        .prepare("SELECT source FROM imports ORDER BY source")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(imports, vec!["./util"]);

    let edges: Vec<(String, String)> = conn
        .prepare(
            "
            SELECT s.qualified_name, e.to_symbol_name
            FROM edges e
            JOIN symbols s ON s.id = e.from_symbol_id
            ORDER BY s.qualified_name, e.to_symbol_name
            ",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(edges, vec![("entry".to_string(), "helper".to_string())]);
}

#[test]
fn rust_trait_methods_and_go_interface_methods_indexed_and_queried() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("greeter.rs"),
        r#"
pub trait Greeter {
    fn greet(&self, name: &str) -> String;
    fn default_greet(&self) -> String {
        "hello".to_string()
    }
}

trait InternalTrait {
    fn internal_method(&self);
}
"#,
    )
    .unwrap();

    fs::write(
        src.join("service.go"),
        r#"
package service

type Greeter interface {
    Greet(name string) string
    greetInternal()
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/tessera.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    let conn = Connection::open(&db).unwrap();
    let symbols: Vec<(String, String, String, Option<String>, bool)> = conn
        .prepare(
            "
            SELECT s.name, s.qualified_name, s.kind, s.container, s.exported != 0
            FROM symbols s
            WHERE s.kind IN ('trait', 'interface', 'function', 'method')
            ORDER BY s.qualified_name, s.kind
            ",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(
        symbols,
        vec![
            (
                "Greeter".to_string(),
                "Greeter".to_string(),
                "interface".to_string(),
                None,
                true
            ),
            (
                "Greeter".to_string(),
                "Greeter".to_string(),
                "trait".to_string(),
                None,
                true
            ),
            (
                "Greet".to_string(),
                "Greeter.Greet".to_string(),
                "method".to_string(),
                Some("Greeter".to_string()),
                true
            ),
            (
                "default_greet".to_string(),
                "Greeter.default_greet".to_string(),
                "function".to_string(),
                Some("Greeter".to_string()),
                true
            ),
            (
                "greet".to_string(),
                "Greeter.greet".to_string(),
                "function".to_string(),
                Some("Greeter".to_string()),
                true
            ),
            (
                "greetInternal".to_string(),
                "Greeter.greetInternal".to_string(),
                "method".to_string(),
                Some("Greeter".to_string()),
                false
            ),
            (
                "InternalTrait".to_string(),
                "InternalTrait".to_string(),
                "trait".to_string(),
                None,
                false
            ),
            (
                "internal_method".to_string(),
                "InternalTrait.internal_method".to_string(),
                "function".to_string(),
                Some("InternalTrait".to_string()),
                false
            ),
        ]
    );

    // Verify finding Greeter returns members
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "Greeter",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let matches = json["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 2);
    for m in matches {
        let members = m["members"].as_array().expect("members array");
        assert_eq!(members.len(), 2, "match: {m:#?}");
    }

    // Verify find on trait method and interface method
    let output_greet = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "Greeter.greet",
        ])
        .output()
        .unwrap();
    assert!(output_greet.status.success());
    let json_greet: serde_json::Value = serde_json::from_slice(&output_greet.stdout).unwrap();
    assert_eq!(json_greet["matches"][0]["symbol"]["name"], "greet");

    let output_go_greet = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "Greeter.Greet",
        ])
        .output()
        .unwrap();
    assert!(output_go_greet.status.success());
    let json_go_greet: serde_json::Value = serde_json::from_slice(&output_go_greet.stdout).unwrap();
    assert_eq!(json_go_greet["matches"][0]["symbol"]["name"], "Greet");
}

#[test]
fn c_and_cpp_declarations_indexed_and_queried() {
    let temp = TempDir::new().unwrap();

    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("math.h"),
        r#"
int add(int a, int b);
static int internal_add(int a, int b);
"#,
    )
    .unwrap();

    fs::write(
        src.join("widget.hpp"),
        r#"
namespace ui {
class Widget {
public:
    int render() const;
    virtual void show() = 0;
};
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/tessera.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    let conn = Connection::open(&db).unwrap();
    let symbols: Vec<(String, String, String, Option<String>, bool)> = conn
        .prepare(
            "
            SELECT s.name, s.qualified_name, s.kind, s.container, s.exported != 0
            FROM symbols s
            WHERE s.kind IN ('class', 'namespace', 'function', 'method')
            ORDER BY s.qualified_name, s.name
            ",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(
        symbols,
        vec![
            (
                "add".to_string(),
                "add".to_string(),
                "function".to_string(),
                None,
                true
            ),
            (
                "internal_add".to_string(),
                "internal_add".to_string(),
                "function".to_string(),
                None,
                false
            ),
            (
                "ui".to_string(),
                "ui".to_string(),
                "namespace".to_string(),
                None,
                true
            ),
            (
                "Widget".to_string(),
                "ui.Widget".to_string(),
                "class".to_string(),
                Some("ui".to_string()),
                true
            ),
            (
                "render".to_string(),
                "ui.Widget.render".to_string(),
                "function".to_string(),
                Some("ui.Widget".to_string()),
                true
            ),
            (
                "show".to_string(),
                "ui.Widget.show".to_string(),
                "function".to_string(),
                Some("ui.Widget".to_string()),
                true
            ),
        ]
    );

    // Verify finding Widget returns its member functions
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "Widget",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let matches = json["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 1);
    let members = matches[0]["members"].as_array().expect("members array");
    assert_eq!(members.len(), 2, "members: {members:#?}");
    let member_sigs: Vec<&str> = members.iter().map(|m| m.as_str().unwrap()).collect();
    assert_eq!(
        member_sigs,
        vec!["int render() const;", "virtual void show() = 0;"]
    );

    // Verify find on member function directly
    let output_render = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "ui.Widget.render",
        ])
        .output()
        .unwrap();
    assert!(output_render.status.success());
    let json_render: serde_json::Value = serde_json::from_slice(&output_render.stdout).unwrap();
    assert_eq!(json_render["matches"][0]["symbol"]["name"], "render");

    // Verify find on C header function
    let output_add = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
            "find",
            "add",
        ])
        .output()
        .unwrap();
    assert!(output_add.status.success());
    let json_add: serde_json::Value = serde_json::from_slice(&output_add.stdout).unwrap();
    assert_eq!(json_add["matches"][0]["symbol"]["name"], "add");
}

fn run_git_cmd(root: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn diff_impact_path_preservation_and_deletions() {
    let temp = TempDir::new().unwrap();
    run_git_cmd(temp.path(), &["init", "-q"]);

    let b_dir = temp.path().join("b/b");
    fs::create_dir_all(&b_dir).unwrap();

    fs::write(
        b_dir.join("service.rs"),
        r#"pub fn calculate(x: i32) -> i32 {
    let a = 1;
    let b = 2;
    x + a + b
}

pub fn untouched() -> i32 {
    100
}

pub fn to_be_deleted() -> i32 {
    42
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("caller.rs"),
        r#"pub fn run() -> i32 {
    calculate(10) + to_be_deleted()
}
"#,
    )
    .unwrap();

    run_git_cmd(temp.path(), &["add", "."]);
    run_git_cmd(temp.path(), &["commit", "-q", "-m", "initial"]);

    let db = temp.path().join(".tessera/tessera.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Modify calculate (delete 3 lines, add 2 lines) and delete to_be_deleted completely
    fs::write(
        b_dir.join("service.rs"),
        r#"pub fn calculate(x: i32) -> i32 {
    let c = 3;
    x + c
}

pub fn untouched() -> i32 {
    100
}
"#,
    )
    .unwrap();

    let conn = Connection::open(&db).unwrap();
    let diff =
        tessera_codegraph::query::diff_impact_conn(&conn, temp.path(), "HEAD", None, 2).unwrap();

    assert_eq!(diff.changed_files, 1);
    let changed_names: Vec<(&str, usize, usize)> = diff
        .changed_symbols
        .iter()
        .map(|c| (c.symbol.name.as_str(), c.added_lines, c.removed_lines))
        .collect();

    // calculate has 2 added lines and 3 removed lines
    let calc = changed_names
        .iter()
        .find(|(name, _, _)| *name == "calculate")
        .expect("calculate changed");
    assert_eq!(calc.1, 2, "calculate added_lines");
    assert_eq!(calc.2, 3, "calculate removed_lines");

    // to_be_deleted is recognized as changed with removed_lines > 0
    let del = changed_names
        .iter()
        .find(|(name, _, _)| *name == "to_be_deleted")
        .expect("to_be_deleted changed");
    assert_eq!(del.1, 0, "to_be_deleted added_lines");
    assert_eq!(del.2, 4, "to_be_deleted removed_lines");

    // untouched is not changed
    assert!(!changed_names
        .iter()
        .any(|(name, _, _)| *name == "untouched"));

    // Callers are impacted
    let impacted_names: Vec<&str> = diff
        .impacted
        .iter()
        .map(|i| i.symbol.name.as_str())
        .collect();
    assert!(impacted_names.contains(&"run"));

    // Now test a completely deleted file
    fs::write(
        temp.path().join("extra.rs"),
        "pub fn extra_helper() -> i32 { 100 }\n",
    )
    .unwrap();
    run_git_cmd(temp.path(), &["add", "."]);
    run_git_cmd(temp.path(), &["commit", "-q", "-m", "add extra"]);

    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Delete extra.rs
    fs::remove_file(temp.path().join("extra.rs")).unwrap();
    let diff_after_delete =
        tessera_codegraph::query::diff_impact_conn(&conn, temp.path(), "HEAD", None, 2).unwrap();
    assert!(diff_after_delete
        .changed_symbols
        .iter()
        .any(|c| c.symbol.name == "extra_helper" && c.removed_lines > 0));
}

#[test]
fn search_and_export_apply_filters_before_sql_limit() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create a large number of functions with prefix `handle_` in src/handlers.rs
    // that outnumber the default limit / candidate limit (80 functions).
    fs::create_dir_all(root.join("src/services")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();

    let mut handlers = String::new();
    for i in 0..80 {
        handlers.push_str(&format!("fn handle_{:02}() -> i32 {{ {} }}\n", i, i));
    }
    fs::write(root.join("src/handlers.rs"), &handlers).unwrap();

    // Create 3 exported classes in src/services/service.ts
    fs::write(
        root.join("src/services/service.ts"),
        r#"export class HandleService {
    run() { return 1; }
}

export class HandleController {
    serve() { return new HandleService().run(); }
}

export class HandleManager {
    manage() { return 3; }
}

export function serviceEntry() { return serviceHelper(); }
export function serviceHelper() { return 42; }
"#,
    )
    .unwrap();

    // Create 30 test edges in tests/test_flow.ts
    let mut test_flow = String::new();
    for i in 0..30 {
        test_flow.push_str(&format!(
            "function test_fn_{i}() {{ return test_callee_{i}(); }}\nfunction test_callee_{i}() {{ return {i}; }}\n"
        ));
    }
    fs::write(root.join("tests/test_flow.ts"), &test_flow).unwrap();

    let db = root.join(".tessera/index.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args([
            "index",
            root.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let conn = Connection::open(&db).unwrap();

    // 1. Search glob pattern `handle*` with `--kind class`
    // Before fix: 80 functions in handlers.rs exhausted the candidate scan limit (50),
    // returning 0 classes.
    let glob_res = tessera_codegraph::query::search_conn(
        &conn,
        "handle*",
        tessera_codegraph::types::SearchOptions {
            kinds: vec!["class".to_string()],
            languages: vec![],
            exported: None,
            path_prefix: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        glob_res.hits.len(),
        3,
        "glob search with kind filter returns all 3 classes"
    );
    let names: Vec<&str> = glob_res
        .hits
        .iter()
        .map(|h| h.symbol.name.as_str())
        .collect();
    assert!(names.contains(&"HandleService"));
    assert!(names.contains(&"HandleController"));
    assert!(names.contains(&"HandleManager"));

    // 2. Search with path_prefix
    let path_res = tessera_codegraph::query::search_conn(
        &conn,
        "handle*",
        tessera_codegraph::types::SearchOptions {
            kinds: vec![],
            languages: vec![],
            exported: None,
            path_prefix: Some("src/services".to_string()),
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        path_res.hits.len(),
        6,
        "path prefix returns all symbols under src/services"
    );
    assert!(path_res
        .hits
        .iter()
        .all(|h| h.symbol.path.starts_with("src/services")));

    // 3. Search with language filter
    let lang_res = tessera_codegraph::query::search_conn(
        &conn,
        "handle*",
        tessera_codegraph::types::SearchOptions {
            kinds: vec![],
            languages: vec!["typescript".to_string()],
            exported: None,
            path_prefix: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        lang_res.hits.len(),
        6,
        "language filter returns typescript symbols"
    );
    assert!(lang_res
        .hits
        .iter()
        .all(|h| h.symbol.language == "typescript"));

    // 4. Search with exported filter
    let exp_res = tessera_codegraph::query::search_conn(
        &conn,
        "handle*",
        tessera_codegraph::types::SearchOptions {
            kinds: vec![],
            languages: vec![],
            exported: Some(true),
            path_prefix: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        exp_res.hits.len(),
        6,
        "exported filter returns only exported symbols"
    );
    assert!(exp_res.hits.iter().all(|h| h.symbol.exported));

    // 5. Search with empty pattern (listing) and kind filter
    let list_res = tessera_codegraph::query::search_conn(
        &conn,
        "",
        tessera_codegraph::types::SearchOptions {
            kinds: vec!["class".to_string()],
            languages: vec![],
            exported: None,
            path_prefix: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        list_res.hits.len(),
        3,
        "empty pattern search with kind filter returns 3 classes"
    );

    // 6. Fuzzy search `Handle` with kind filter
    let fuzzy_res = tessera_codegraph::query::search_conn(
        &conn,
        "Handle",
        tessera_codegraph::types::SearchOptions {
            kinds: vec!["class".to_string()],
            languages: vec![],
            exported: None,
            path_prefix: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(
        fuzzy_res.hits.len(),
        3,
        "fuzzy search with kind filter returns 3 classes"
    );

    // 7. CLI command search test
    Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(root)
        .args([
            "search",
            "handle*",
            "--kind",
            "class",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("HandleService"))
        .stdout(predicates::str::contains("HandleController"))
        .stdout(predicates::str::contains("HandleManager"));

    // 8. Export with collapse_tests:
    // tests/test_flow.ts has 30 edges. If SQL limit was 10 (or 11), they would exhaust the limit,
    // leaving 0 edges from src/services.
    let export_res = tessera_codegraph::query::export_conn_with_options(
        &conn,
        tessera_codegraph::query::ExportOptions {
            format: "mermaid".to_string(),
            from: None,
            depth: 1,
            limit: 10,
            group_by: tessera_codegraph::query::ExportGroupBy::None,
            collapse_tests: true,
            exported_only: false,
            html_out: None,
        },
    )
    .unwrap();
    assert!(
        !export_res.diagram.is_empty(),
        "diagram should not be empty"
    );
    assert!(
        export_res.edges > 0,
        "collapse_tests should not cause empty export due to test edges"
    );

    // 9. Export with exported_only:
    // tests/test_flow.ts has 30 non-exported edges. If SQL limit was 10 (or 11), they would exhaust the limit.
    let export_exp = tessera_codegraph::query::export_conn_with_options(
        &conn,
        tessera_codegraph::query::ExportOptions {
            format: "mermaid".to_string(),
            from: None,
            depth: 1,
            limit: 10,
            group_by: tessera_codegraph::query::ExportGroupBy::None,
            collapse_tests: false,
            exported_only: true,
            html_out: None,
        },
    )
    .unwrap();
    assert!(
        !export_exp.diagram.is_empty(),
        "diagram should not be empty"
    );
    assert!(
        export_exp.edges > 0,
        "exported_only should not cause empty export due to non-exported edges"
    );
}

#[test]
fn typescript_interface_type_alias_and_enum_indexed_and_queried() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("types.ts"),
        r#"
export interface User {
    id: string;
    name: string;
    greet(): string;
}

interface InternalConfig {
    secret: string;
}

export type UserId = string;

type InternalId = number;

export enum Status {
    Active,
    Inactive = "INACTIVE",
}

enum LogLevel {
    Debug,
    Info,
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/tessera.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success();

    let conn = Connection::open(&db).unwrap();

    // 1. Verify symbols table in SQLite
    let symbols: Vec<(String, String, String, Option<String>, bool)> = conn
        .prepare(
            "
            SELECT s.name, s.qualified_name, s.kind, s.container, s.exported != 0
            FROM symbols s
            ORDER BY s.qualified_name, s.kind
            ",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(
        symbols,
        vec![
            (
                "InternalConfig".to_string(),
                "InternalConfig".to_string(),
                "interface".to_string(),
                None,
                false,
            ),
            (
                "InternalId".to_string(),
                "InternalId".to_string(),
                "type".to_string(),
                None,
                false,
            ),
            (
                "LogLevel".to_string(),
                "LogLevel".to_string(),
                "enum".to_string(),
                None,
                false,
            ),
            (
                "Status".to_string(),
                "Status".to_string(),
                "enum".to_string(),
                None,
                true,
            ),
            (
                "User".to_string(),
                "User".to_string(),
                "interface".to_string(),
                None,
                true,
            ),
            (
                "greet".to_string(),
                "User.greet".to_string(),
                "method".to_string(),
                Some("User".to_string()),
                true,
            ),
            (
                "UserId".to_string(),
                "UserId".to_string(),
                "type".to_string(),
                None,
                true,
            ),
        ]
    );

    // 2. Verify find_definition_conn
    let def_user = tessera_codegraph::query::find_definition_conn(&conn, "User").unwrap();
    assert_eq!(def_user.matches.len(), 1);
    assert_eq!(def_user.matches[0].kind, "interface");

    let def_userid = tessera_codegraph::query::find_definition_conn(&conn, "UserId").unwrap();
    assert_eq!(def_userid.matches.len(), 1);
    assert_eq!(def_userid.matches[0].kind, "type");

    let def_status = tessera_codegraph::query::find_definition_conn(&conn, "Status").unwrap();
    assert_eq!(def_status.matches.len(), 1);
    assert_eq!(def_status.matches[0].kind, "enum");

    // 3. Verify validate_conn
    assert!(
        tessera_codegraph::query::validate_conn(&conn, "User")
            .unwrap()
            .exists
    );
    assert!(
        tessera_codegraph::query::validate_conn(&conn, "UserId")
            .unwrap()
            .exists
    );
    assert!(
        tessera_codegraph::query::validate_conn(&conn, "Status")
            .unwrap()
            .exists
    );
    assert!(
        !tessera_codegraph::query::validate_conn(&conn, "NonExistent")
            .unwrap()
            .exists
    );

    // 4. Verify CLI find with --body
    let output_user = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "find",
            "User",
            "--body",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output_user.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output_user.stdout).unwrap();
    let body = parsed["matches"][0]["body"].as_str().unwrap_or_default();
    assert!(body.contains("export interface User"));

    // 5. Verify CLI search by kind
    let output_interfaces = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "search",
            "*",
            "--kind",
            "interface",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output_interfaces.status.success());
    let parsed_interfaces: serde_json::Value =
        serde_json::from_slice(&output_interfaces.stdout).unwrap();
    assert_eq!(parsed_interfaces["hits"].as_array().unwrap().len(), 2);

    let output_types = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "search",
            "*",
            "--kind",
            "type",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output_types.status.success());
    let parsed_types: serde_json::Value = serde_json::from_slice(&output_types.stdout).unwrap();
    assert_eq!(parsed_types["hits"].as_array().unwrap().len(), 2);

    let output_enums = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "search",
            "*",
            "--kind",
            "enum",
            "--json",
        ])
        .output()
        .unwrap();
    let parsed_enums: serde_json::Value = serde_json::from_slice(&output_enums.stdout).unwrap();
    assert_eq!(parsed_enums["hits"].as_array().unwrap().len(), 2);

    // 6. Verify CLI search exported only
    let output_exported_types = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--root",
            temp.path().to_str().unwrap(),
            "search",
            "*",
            "--kind",
            "type",
            "--exported",
            "--json",
        ])
        .output()
        .unwrap();
    let parsed_exp_types: serde_json::Value =
        serde_json::from_slice(&output_exported_types.stdout).unwrap();
    assert_eq!(parsed_exp_types["hits"].as_array().unwrap().len(), 1);
    assert_eq!(
        parsed_exp_types["hits"][0]["symbol"]["name"]
            .as_str()
            .unwrap(),
        "UserId"
    );
}
