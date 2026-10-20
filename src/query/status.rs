use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use super::legacy;
use crate::db;
use crate::fresh::{self, Freshness};
use crate::output::AgentText;
use crate::types::LanguageCount;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize)]
pub struct ShadowBinary {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResult {
    pub version: String,
    pub binary: Option<PathBuf>,
    pub shadowing: Vec<ShadowBinary>,
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub schema: i64,
    pub freshness: Freshness,
    pub files: usize,
    pub symbols: usize,
    pub refs: usize,
    pub languages: Vec<LanguageCount>,
    pub skipped: usize,
    pub hints: Vec<String>,
}

pub fn run(ws: &Workspace) -> Result<StatusResult> {
    let freshness = fresh::check(ws)?;
    let binary = env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok());
    let shadowing = shadowing_binaries(binary.as_deref());
    let mut status = StatusResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        binary,
        shadowing,
        root: ws.root.clone(),
        db_path: ws.db_path.clone(),
        schema: 0,
        freshness,
        files: 0,
        symbols: 0,
        refs: 0,
        languages: Vec::new(),
        skipped: 0,
        hints: Vec::new(),
    };
    if let Ok(conn) = db::open_ro(&ws.db_path) {
        fill_counts(&mut status, &conn)?;
    }
    status.hints = hints(&status);
    Ok(status)
}

fn fill_counts(status: &mut StatusResult, conn: &Connection) -> Result<()> {
    let stats = legacy::stats_conn(conn, &status.db_path)?;
    status.schema = db::schema_version(conn)?;
    status.files = stats.files;
    status.symbols = stats.symbols;
    status.refs = stats.references;
    status.languages = stats.languages;
    status.skipped = conn.query_row(
        "SELECT COUNT(*) FROM skipped_files WHERE reason != 'excluded'",
        [],
        |row| row.get::<_, i64>(0),
    )? as usize;
    Ok(())
}

fn hints(status: &StatusResult) -> Vec<String> {
    let mut hints = Vec::new();
    match status.freshness {
        Freshness::Missing => hints.push(format!(
            "No index yet. Any query builds it, or run `tessera index {}`.",
            status.root.display()
        )),
        Freshness::Stale { .. } => {
            hints.push("Index is stale; the next query re-indexes changed files.".to_string())
        }
        Freshness::Fresh => {}
    }
    if status.files == 0 && status.freshness != Freshness::Missing {
        hints.push("No supported source files found under the root.".to_string());
    }
    for shadow in &status.shadowing {
        hints.push(format!(
            "`tessera` on PATH resolves to {} ({}), not this binary; agents may run the older one.",
            shadow.path.display(),
            shadow.version
        ));
    }
    hints
}

fn shadowing_binaries(current: Option<&Path>) -> Vec<ShadowBinary> {
    let exe_name = if cfg!(windows) {
        "tessera.exe"
    } else {
        "tessera"
    };
    let Some(path_var) = env::var_os("PATH") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate.canonicalize().unwrap_or(candidate.clone());
        if Some(canonical.as_path()) == current {
            break;
        }
        if found.iter().any(|s: &ShadowBinary| s.path == canonical) {
            continue;
        }
        let version = Command::new(&canonical)
            .arg("--version")
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown version".to_string());
        if version.trim_start_matches("tessera ") != env!("CARGO_PKG_VERSION") {
            found.push(ShadowBinary {
                path: canonical,
                version,
            });
        }
        break;
    }
    found
}

impl fmt::Display for StatusResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for StatusResult {
    fn agent_text(&self) -> String {
        let freshness = match self.freshness {
            Freshness::Fresh => "fresh".to_string(),
            Freshness::Missing => "missing".to_string(),
            Freshness::Stale {
                changed,
                added,
                removed,
            } => format!("stale ({changed} changed, {added} added, {removed} removed)"),
        };
        let languages = self
            .languages
            .iter()
            .map(|l| format!("{} {}", l.language, l.count))
            .collect::<Vec<_>>()
            .join(", ");
        let mut out = format!(
            "tessera {}\nroot   {}\nindex  {} (schema v{}, {freshness})\nfiles  {} indexed, {} skipped | symbols {} | refs {}\n",
            self.version,
            self.root.display(),
            self.db_path.display(),
            self.schema,
            self.files,
            self.skipped,
            self.symbols,
            self.refs
        );
        if !languages.is_empty() {
            out.push_str(&format!("langs  {languages}\n"));
        }
        for hint in &self.hints {
            out.push_str(&format!("hint: {hint}\n"));
        }
        out
    }
}
