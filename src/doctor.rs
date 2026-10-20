use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::indexer;
use crate::types::Language;

#[derive(Debug, Clone)]
pub struct DoctorOptions {
    pub root: PathBuf,
    pub db_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorResult {
    pub ok: bool,
    pub root: String,
    pub db_path: String,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Error,
}

impl Display for DoctorResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Tessera doctor for {} (db={})", self.root, self.db_path)?;
        for check in &self.checks {
            let marker = match check.status {
                DoctorStatus::Ok => "ok",
                DoctorStatus::Warn => "warn",
                DoctorStatus::Error => "error",
            };
            writeln!(f, "  [{marker}] {}: {}", check.name, check.message)?;
            if let Some(hint) = &check.hint {
                writeln!(f, "        hint: {hint}")?;
            }
        }
        if self.ok {
            writeln!(f, "Doctor passed.")
        } else {
            writeln!(f, "Doctor found issues.")
        }
    }
}

pub fn run(options: DoctorOptions) -> Result<DoctorResult> {
    let root = options
        .root
        .canonicalize()
        .unwrap_or_else(|_| options.root.clone());
    let db_path = options.db_path;
    let mut checks = Vec::new();

    check_root(&root, &mut checks);
    check_db(&db_path, &mut checks)?;
    check_parsers(&mut checks);
    check_ignored_paths(&mut checks);
    check_mcp_config(&db_path, &mut checks);

    let ok = !checks
        .iter()
        .any(|check| matches!(check.status, DoctorStatus::Error));

    Ok(DoctorResult {
        ok,
        root: root.display().to_string(),
        db_path: db_path.display().to_string(),
        checks,
    })
}

fn check_root(root: &Path, checks: &mut Vec<DoctorCheck>) {
    if root.exists() && root.is_dir() {
        push_ok(checks, "root", format!("{} exists", root.display()), None);
    } else {
        push_error(
            checks,
            "root",
            format!("{} is not a directory", root.display()),
            Some("Run from a repository root or pass `--root <path>`.".to_string()),
        );
    }
}

fn check_db(db_path: &Path, checks: &mut Vec<DoctorCheck>) -> Result<()> {
    if !db_path.exists() {
        push_error(
            checks,
            "database",
            format!("{} does not exist", db_path.display()),
            Some(format!(
                "Run `tessera index . --db {}` first.",
                db_path.display()
            )),
        );
        return Ok(());
    }

    let conn = match db::open_ro(db_path) {
        Ok(conn) => conn,
        Err(error) => {
            push_error(
                checks,
                "schema",
                error.to_string(),
                Some(
                    "Run `tessera index . --full` to rebuild with the current schema.".to_string(),
                ),
            );
            return Ok(());
        }
    };
    push_ok(
        checks,
        "schema",
        format!("schema version {}", db::SCHEMA_VERSION),
        None,
    );

    let files = count_table(&conn, "files")?;
    let symbols = count_table(&conn, "symbols")?;
    push_ok(
        checks,
        "index",
        format!("{files} files, {symbols} symbols"),
        if symbols == 0 {
            Some("The DB is valid but empty; run `tessera index . --full`.".to_string())
        } else {
            None
        },
    );
    Ok(())
}

fn check_parsers(checks: &mut Vec<DoctorCheck>) {
    let snippets = [
        (Language::JavaScript, "function f() { return 1; }\n"),
        (
            Language::TypeScript,
            "export function f(x: string): string { return x; }\n",
        ),
        (Language::Tsx, "export function C() { return <div />; }\n"),
        (Language::Python, "def f():\n    return 1\n"),
        (Language::Go, "package sample\nfunc F() int { return 1 }\n"),
        (Language::Rust, "pub fn f() -> i32 { 1 }\n"),
        (Language::Java, "class A { int f() { return 1; } }\n"),
        (Language::C, "int f() { return 1; }\n"),
        (
            Language::Cpp,
            "class A { public: int f() { return 1; } };\n",
        ),
        (Language::CSharp, "class A { int F() { return 1; } }\n"),
        (Language::Ruby, "def f\n  1\nend\n"),
        (Language::Php, "<?php function f() { return 1; }\n"),
    ];

    let mut failed = Vec::new();
    for (language, snippet) in snippets {
        if indexer::parse_file(language, snippet).is_err() {
            failed.push(language.to_string());
        }
    }

    if failed.is_empty() {
        push_ok(
            checks,
            "parsers",
            "all bundled parsers load".to_string(),
            None,
        );
    } else {
        push_error(
            checks,
            "parsers",
            format!("failed parser smoke tests: {}", failed.join(", ")),
            Some("Reinstall Tessera or rebuild from a clean checkout.".to_string()),
        );
    }
}

fn check_ignored_paths(checks: &mut Vec<DoctorCheck>) {
    push_ok(
        checks,
        "ignored-paths",
        "skips .git, node_modules, target, dist, build, .next, virtualenvs, __pycache__, and .tessera".to_string(),
        None,
    );
}

fn check_mcp_config(db_path: &Path, checks: &mut Vec<DoctorCheck>) {
    push_ok(
        checks,
        "mcp",
        format!("stdio command: tessera mcp --db {}", db_path.display()),
        Some("Add this command to your agent MCP config after indexing.".to_string()),
    );
}

fn count_table(conn: &Connection, table: &str) -> Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count as usize)
}

fn push_ok(checks: &mut Vec<DoctorCheck>, name: &str, message: String, hint: Option<String>) {
    checks.push(DoctorCheck {
        name: name.to_string(),
        status: DoctorStatus::Ok,
        message,
        hint,
    });
}

fn push_error(checks: &mut Vec<DoctorCheck>, name: &str, message: String, hint: Option<String>) {
    checks.push(DoctorCheck {
        name: name.to_string(),
        status: DoctorStatus::Error,
        message,
        hint,
    });
}
