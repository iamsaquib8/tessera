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

pub fn synth_repo(root: &Path, files: usize) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("util.ts"),
        "export function sharedHelper(x: number): number {\n  return (x * 13 + 7) ^ 0;\n}\n\n\
         export function formatRow(row: { id: number; name: string }): string {\n  return `${row.id}\\t${row.name}`;\n}\n\n\
         export function parseInput(raw: string): number {\n  return Number.parseInt(raw, 10);\n}\n",
    )
    .unwrap();
    for i in 0..files {
        let prev = (i + files - 1) % files;
        let next = (i + 1) % files;
        fs::write(
            root.join(format!("module_{i:05}.ts")),
            format!(
                "import {{ sharedHelper, formatRow, parseInput }} from \"./util\";\n\n\
                 export function moduleEntry{i}(raw: string, label: string): string {{\n\
                 \x20 const value = helper{i}(parseInput(raw));\n\
                 \x20 return formatRow({{ id: value, name: label }});\n}}\n\n\
                 function helper{i}(x: number): number {{\n\
                 \x20 return sharedHelper(x) + bridge{prev}(x) + bridge{next}(x);\n}}\n\n\
                 export function bridge{i}(x: number): number {{\n  return sharedHelper(x + {i});\n}}\n"
            ),
        )
        .unwrap();
    }
}
