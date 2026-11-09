use std::env;
use std::fmt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::db;
use crate::fresh::{self, Freshness};
use crate::output::AgentText;
use crate::resolve::{self, ResolveReport};
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
    pub unreadable: Vec<String>,
    pub parse_errors: usize,
    /// The last resolve pass: how many references resolved at each confidence.
    pub resolution: Option<ResolveReport>,
    #[serde(skip)]
    parse_error_first: Option<String>,
    pub hints: Vec<String>,
}

pub fn run(ws: &Workspace) -> Result<StatusResult> {
    let freshness = fresh::check(ws)?;
    let binary = env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok());
    // PATH discovery is reserved for the explicit CLI status command.
    let shadowing = Vec::new();
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
        unreadable: Vec::new(),
        parse_errors: 0,
        resolution: None,
        parse_error_first: None,
        hints: Vec::new(),
    };
    if status.freshness == Freshness::Corrupt {
        // The tables cannot be read; the index is rebuilt by the next run.
    } else if let Ok(conn) = db::open_ro(&ws.db_path) {
        fill_counts(&mut status, &conn)?;
    }
    status.hints = hints(&status);
    Ok(status)
}

/// The interactive CLI may report PATH shadowing; MCP and UI status never
/// inspect PATH or trigger another executable.
pub fn run_with_shadowing(ws: &Workspace) -> Result<StatusResult> {
    let mut status = run(ws)?;
    status.shadowing = shadowing_binaries(status.binary.as_deref(), &ws.root);
    status.hints = hints(&status);
    Ok(status)
}

fn fill_counts(status: &mut StatusResult, conn: &Connection) -> Result<()> {
    status.schema = db::schema_version(conn)?;
    status.files = db::count_files(conn)?;
    status.symbols = db::count_symbols(conn)?;
    status.refs = db::count_refs(conn)?;
    status.languages = language_counts(conn)?;
    status.skipped = conn.query_row(
        "SELECT COUNT(*) FROM skipped_files WHERE reason != 'excluded'",
        [],
        |row| row.get::<_, i64>(0),
    )? as usize;
    status.unreadable = db::unreadable_paths(conn)?;
    let parse_errors = db::parse_error_paths(conn)?;
    status.parse_errors = parse_errors.len();
    status.parse_error_first = parse_errors.into_iter().next();
    status.resolution = resolve::last_report(conn)?;
    Ok(())
}

fn language_counts(conn: &Connection) -> Result<Vec<LanguageCount>> {
    let mut stmt =
        conn.prepare("SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY 2 DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(LanguageCount {
            language: row.get::<_, String>(0)?,
            count: row.get::<_, i64>(1)? as usize,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
        Freshness::Outdated => hints.push(
            "Index was built by another tessera version or config; the next query rebuilds it."
                .to_string(),
        ),
        Freshness::Corrupt => hints.push(format!(
            "The index file is damaged. The next query or `tessera index` moves it to {}.corrupt and rebuilds it.",
            status.db_path.display()
        )),
        Freshness::Pending => hints.push(format!(
            "An edit changed code that more than {} files use; they are re-resolved by the next query or `tessera index`, and results about them may be stale until then.",
            resolve::MAX_INCREMENTAL_FILES
        )),
        Freshness::Fresh => {}
    }
    if status.files == 0 && !matches!(status.freshness, Freshness::Missing | Freshness::Corrupt) {
        hints.push("No supported source files found under the root.".to_string());
    }
    if let Some(first) = status.unreadable.first() {
        let count = status.unreadable.len();
        hints.push(format!(
            "{count} file{} could not be read (first: {first}); check its permissions.",
            if count == 1 { "" } else { "s" }
        ));
    }
    if let Some(first) = &status.parse_error_first {
        let count = status.parse_errors;
        hints.push(format!(
            "{count} file{} syntax errors (first: {first}); symbols after an error may be missing.",
            if count == 1 { " has" } else { "s have" }
        ));
    }
    for shadow in &status.shadowing {
        hints.push(format!(
            "`tessera` on PATH resolves to {} ({}), not this binary; check which executable your shell selects.",
            shadow.path.display(),
            shadow.version
        ));
    }
    hints
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn shadowing_binaries(current: Option<&Path>, root: &Path) -> Vec<ShadowBinary> {
    let exe_name = if cfg!(windows) {
        "tessera.exe"
    } else {
        "tessera"
    };
    let Some(path_var) = env::var_os("PATH") else {
        return Vec::new();
    };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut found = Vec::new();
    for dir in env::split_paths(&path_var) {
        // Empty and relative entries resolve against the current directory.
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(exe_name);
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        if !candidate
            .metadata()
            .is_ok_and(|file| file.permissions().mode() & 0o111 != 0)
        {
            continue;
        }
        let canonical = candidate.canonicalize().unwrap_or(candidate.clone());
        // A repository may contain a deliberately malicious or test executable.
        // Check both lexical containment (including symlinks out) and resolved
        // containment (including symlinks in), without misclassifying parent paths.
        if lexical_normalize(&candidate).starts_with(&root) || canonical.starts_with(&root) {
            continue;
        }
        if Some(canonical.as_path()) == current {
            break;
        }
        if found.iter().any(|s: &ShadowBinary| s.path == canonical) {
            continue;
        }
        // Never run a PATH-discovered executable from a read-only status call.
        found.push(ShadowBinary {
            path: canonical,
            version: "not checked".to_string(),
        });
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
            Freshness::Outdated => "outdated".to_string(),
            Freshness::Corrupt => "corrupt".to_string(),
            Freshness::Pending => "re-resolve pending".to_string(),
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
        if let Some(r) = &self.resolution {
            out.push_str(&format!(
                "refs   {} high, {} medium, {} low confidence, {} unresolved\n",
                r.high, r.medium, r.low, r.unresolved
            ));
        }
        for hint in &self.hints {
            out.push_str(&format!("hint: {hint}\n"));
        }
        out
    }
}
