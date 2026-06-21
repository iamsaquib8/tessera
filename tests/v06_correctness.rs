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
