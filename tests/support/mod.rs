#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::TempDir;
use tessera_codegraph::indexer::IndexOptions;
use tessera_codegraph::Index;

pub fn fixture_dir(lang: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(lang)
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn index_fixture(lang: &str) -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    copy_dir(&fixture_dir(lang), &root);
    let db = temp.path().join("tessera.db");
    Index::build(
        &root,
        &db,
        IndexOptions {
            full: true,
            ..IndexOptions::default()
        },
    )
    .unwrap();
    (temp, db)
}

fn rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    let mut out: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap().replace('\\', "/"))
        .collect();
    out.sort();
    out
}

pub fn graph_dump(db: &Path) -> Value {
    let conn = Connection::open(db).unwrap();
    json!({
        "symbols": rows(&conn, "
            SELECT f.path || ':' || s.start_line || '-' || s.end_line || ' ' || s.kind || ' '
                || s.qualified_name || CASE WHEN s.exported = 1 THEN ' [exported]' ELSE '' END
                || ' | ' || s.signature
            FROM symbols s JOIN files f ON f.id = s.file_id"),
        "refs": rows(&conn, "
            SELECT f.path || ':' || r.line || ':' || r.column || ' ' || r.kind || ' ' || r.symbol_name
                || ' <- ' || COALESCE(fs.qualified_name, '<none>')
            FROM refs r JOIN files f ON f.id = r.file_id
            LEFT JOIN symbols fs ON fs.id = r.from_symbol_id"),
        "imports": rows(&conn, "
            SELECT f.path || ':' || i.line || ' ' || i.kind || ' ' || i.source
            FROM imports i JOIN files f ON f.id = i.file_id"),
        "edges": rows(&conn, "
            SELECT ff.path || ' ' || fs.qualified_name || ' -' || e.kind || '-> ' || e.to_symbol_name
            FROM edges e JOIN symbols fs ON fs.id = e.from_symbol_id
            JOIN files ff ON ff.id = fs.file_id"),
    })
}
