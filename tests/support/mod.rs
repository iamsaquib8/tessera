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
                || CASE WHEN s.default_export = 1 THEN ' [default]' ELSE '' END
                || CASE WHEN s.arity_min IS NOT NULL
                        THEN ' (' || s.arity_min || '..' || COALESCE(s.arity_max, '*') || ')' ELSE '' END
                || ' | ' || s.signature
            FROM symbols s JOIN files f ON f.id = s.file_id"),
        "refs": rows(&conn, "
            SELECT f.path || ':' || r.line || ':' || r.column || ' ' || r.kind || ' '
                || CASE r.receiver WHEN 'none' THEN '' WHEN 'name' THEN r.receiver_name || '.'
                        ELSE r.receiver || '.' END
                || r.symbol_name
                || CASE WHEN r.arg_count IS NOT NULL THEN '/' || r.arg_count ELSE '' END
                || ' <- ' || COALESCE(fs.qualified_name, '<none>')
            FROM refs r JOIN files f ON f.id = r.file_id
            LEFT JOIN symbols fs ON fs.id = r.from_symbol_id"),
        "bindings": rows(&conn, "
            SELECT f.path || ':' || b.line || ' ' || b.kind || ' ' || b.local
                || COALESCE(' = ' || b.imported, '') || ' from ' || b.source
            FROM import_bindings b JOIN files f ON f.id = b.file_id"),
        "hints": rows(&conn, "
            SELECT f.path || ' ' || COALESCE(h.scope || '::', '') || h.name || ': ' || h.type_name
                || ' (' || h.origin || ')'
            FROM type_hints h JOIN files f ON f.id = h.file_id"),
        "modules": rows(&conn, "SELECT path || ' -> ' || module FROM files WHERE module IS NOT NULL"),
        "imports": rows(&conn, "
            SELECT f.path || ':' || i.line || ' ' || i.kind || ' ' || i.source
            FROM imports i JOIN files f ON f.id = i.file_id"),
        "edges": rows(&conn, "
            SELECT ff.path || ' ' || fs.qualified_name || ' -' || e.kind || '-> '
                || COALESCE(tf.path || ':' || ts.qualified_name, e.to_symbol_name)
                || ' [' || e.confidence || COALESCE('/' || e.resolver, '') || ']'
            FROM edges e JOIN symbols fs ON fs.id = e.from_symbol_id
            JOIN files ff ON ff.id = fs.file_id
            LEFT JOIN symbols ts ON ts.id = e.to_symbol_id
            LEFT JOIN files tf ON tf.id = ts.file_id"),
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
