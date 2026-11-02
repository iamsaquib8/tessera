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
