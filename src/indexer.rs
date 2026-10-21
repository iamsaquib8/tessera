use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::config::TesseraConfig;
use crate::db;
use crate::resolve::{self, ResolveReport, ResolveScope};
use crate::walk::{self, FileStatus, SkipReason, SourceFile, WalkOptions};
use crate::workspace::Workspace;

#[derive(Debug, Clone)]
pub struct IndexReport {
    pub files_indexed: usize,
    pub files_reused: usize,
    pub files_removed: usize,
    pub symbols_indexed: usize,
    pub references_indexed: usize,
    pub files_skipped: usize,
    pub elapsed_ms: u128,
    pub mode: IndexMode,
    pub warnings: Vec<IndexWarning>,
    pub resolve: Option<ResolveReport>,
}

#[derive(Debug, Clone)]
pub struct IndexWarning {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexMode {
    Full,
    Incremental,
}

#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub full: bool,
    /// `None` keeps the excludes persisted by an earlier run; `Some` replaces them.
    pub excludes: Option<Vec<String>>,
}

/// Exclusive advisory lock that serialises writers of one index across processes.
pub struct IndexLock {
    _file: File,
}

impl IndexLock {
    pub fn path(db_path: &Path) -> PathBuf {
        let name = db_path.file_name().unwrap_or_default().to_string_lossy();
        db_path.with_file_name(format!("{name}.lock"))
    }

    /// Waits until no other writer holds the lock.
    pub fn acquire(db_path: &Path) -> Result<Self> {
        let file = Self::open(db_path)?;
        file.lock()
            .with_context(|| format!("failed to lock {}", Self::path(db_path).display()))?;
        Ok(Self { _file: file })
    }

    /// Returns `None` when another writer holds the lock.
    pub fn try_acquire(db_path: &Path) -> Result<Option<Self>> {
        let file = Self::open(db_path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error)
                .with_context(|| format!("failed to lock {}", Self::path(db_path).display())),
        }
    }

    fn open(db_path: &Path) -> Result<File> {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let path = Self::path(db_path);
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))
    }
}

pub fn index_path(root: &Path, db_path: &Path) -> Result<IndexReport> {
    index_path_with(root, db_path, IndexOptions::default())
}

const EXCLUDES_META_KEY: &str = "excludes";
const FINGERPRINT_META_KEY: &str = "fingerprint";

pub fn index_path_with(root: &Path, db_path: &Path, options: IndexOptions) -> Result<IndexReport> {
    let lock = IndexLock::acquire(db_path)?;
    index_path_locked(&lock, root, db_path, options)
}

/// Indexes with `lock` already held by the caller.
pub fn index_path_locked(
    _lock: &IndexLock,
    root: &Path,
    db_path: &Path,
    options: IndexOptions,
) -> Result<IndexReport> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let conn = db::open_rw(db_path)?;
    ignore_index_dir(db_path)?;
    let config = TesseraConfig::load(&root);
    let excludes = match options.excludes {
        Some(excludes) => excludes,
        None => persisted_excludes(&conn)?,
    };
    let fingerprint = fingerprint(&root, &excludes);
    let outdated = db::get_meta(&conn, FINGERPRINT_META_KEY)?.as_deref() != Some(&fingerprint);
    let full = options.full || outdated || db::count_files(&conn)? == 0;

    let mut report = IndexReport::new(if full {
        IndexMode::Full
    } else {
        IndexMode::Incremental
    });
    let mut visited_ids: Vec<i64> = Vec::new();
    let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
    if full {
        db::reset(&tx)?;
    }
    db::set_meta(&tx, "root", &root.to_string_lossy())?;
    db::set_meta(&tx, EXCLUDES_META_KEY, &serde_json::to_string(&excludes)?)?;
    db::set_meta(&tx, FINGERPRINT_META_KEY, &fingerprint)?;

    let discovery = walk::discover(&root, &config, &walk_options(&config, &excludes))?;
    report.warnings.extend(discovery.warnings);
    let mut skipped: Vec<(String, u64, i64, &str)> = discovery
        .skipped
        .iter()
        .map(|s| (s.rel_path.clone(), s.size, s.mtime_ns, s.reason.as_str()))
        .collect();

    let mut changed_ids: Vec<i64> = Vec::new();
    for file in &discovery.files {
        match index_one(&tx, file, full, &mut report)? {
            FileOutcome::Indexed(id) => {
                visited_ids.push(id);
                changed_ids.push(id);
            }
            FileOutcome::Reused(id) => visited_ids.push(id),
            FileOutcome::Skipped(reason) => skipped.push((
                file.rel_path.clone(),
                file.size,
                file.mtime_ns,
                reason.as_str(),
            )),
        }
    }

    if !full {
        report.files_removed = db::delete_files_not_in(&tx, &visited_ids)?;
    }
    report.files_skipped = skipped.len();
    db::replace_skipped(&tx, &skipped)?;
    let scope = if full {
        Some(ResolveScope::All)
    } else if !changed_ids.is_empty() || report.files_removed > 0 {
        Some(ResolveScope::Changed(changed_ids))
    } else {
        None
    };
    if let Some(scope) = scope {
        report.resolve = Some(resolve::resolve(&tx, &root, scope)?);
    }
    tx.commit()?;

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

/// Changes whenever the binary, `.tessera/config.toml`, or the excludes change,
/// so an index built under different rules is rebuilt rather than trusted.
pub fn fingerprint(root: &Path, excludes: &[String]) -> String {
    let config = fs::read(root.join(".tessera/config.toml")).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(env!("CARGO_PKG_VERSION"));
    hasher.update([0]);
    hasher.update(&config);
    hasher.update([0]);
    hasher.update(excludes.join("\n"));
    format!("{:x}", hasher.finalize())
}

pub fn stored_fingerprint(conn: &Connection) -> Result<Option<String>> {
    db::get_meta(conn, FINGERPRINT_META_KEY)
}

pub fn index_files(ws: &Workspace, rel_paths: &[String]) -> Result<IndexReport> {
    let _lock = IndexLock::acquire(&ws.db_path)?;
    let started = Instant::now();
    let root = ws
        .root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", ws.root.display()))?;
    let conn = db::open_rw(&ws.db_path)?;
    let config = TesseraConfig::load(&root);
    let options = walk_options(&config, &persisted_excludes(&conn)?);
    let mut report = IndexReport::new(IndexMode::Incremental);
    let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
    let mut changed_ids: Vec<i64> = Vec::new();
    for rel_path in rel_paths {
        let rel_path = rel_path.trim_start_matches("./").replace('\\', "/");
        let status = walk::describe(&root, &rel_path, &config, &options)?;
        let FileStatus::Source(file) = status else {
            if let Some((id, _)) = db::file_sha(&tx, &rel_path)? {
                db::delete_file_cascade(&tx, id)?;
                report.files_removed += 1;
            }
            match status {
                FileStatus::Skipped(s) => {
                    db::upsert_skipped(&tx, &s.rel_path, s.size, s.mtime_ns, s.reason.as_str())?;
                    report.files_skipped += 1;
                }
                _ => db::delete_skipped(&tx, &rel_path)?,
            }
            continue;
        };
        db::delete_skipped(&tx, &rel_path)?;
        match index_one(&tx, &file, false, &mut report)? {
            FileOutcome::Skipped(reason) => {
                if let Some((id, _)) = db::file_sha(&tx, &rel_path)? {
                    db::delete_file_cascade(&tx, id)?;
                }
                db::upsert_skipped(&tx, &rel_path, file.size, file.mtime_ns, reason.as_str())?;
                report.files_skipped += 1;
            }
            FileOutcome::Indexed(id) => changed_ids.push(id),
            FileOutcome::Reused(_) => {}
        }
    }
    if !changed_ids.is_empty() || report.files_removed > 0 {
        report.resolve = Some(resolve::resolve(
            &tx,
            &root,
            ResolveScope::Changed(changed_ids),
        )?);
    }
    tx.commit()?;
    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

pub fn persisted_excludes(conn: &Connection) -> Result<Vec<String>> {
    Ok(db::get_meta(conn, EXCLUDES_META_KEY)?
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default())
}

pub fn walk_options(config: &TesseraConfig, excludes: &[String]) -> WalkOptions {
    WalkOptions {
        max_file_bytes: config
            .max_file_bytes
            .unwrap_or(WalkOptions::default().max_file_bytes),
        excludes: excludes.to_vec(),
        sniff_content: false,
    }
}

impl IndexReport {
    fn new(mode: IndexMode) -> Self {
        Self {
            files_indexed: 0,
            files_reused: 0,
            files_removed: 0,
            symbols_indexed: 0,
            references_indexed: 0,
            files_skipped: 0,
            elapsed_ms: 0,
            mode,
            warnings: Vec::new(),
            resolve: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileOutcome {
    Indexed(i64),
    Reused(i64),
    Skipped(SkipReason),
}

fn index_one(
    tx: &Connection,
    file: &SourceFile,
    full: bool,
    report: &mut IndexReport,
) -> Result<FileOutcome> {
    let rel_path = &file.rel_path;
    if !full {
        if let Some((id, size, mtime_ns)) = db::file_stat(tx, rel_path)? {
            if size == file.size && mtime_ns == file.mtime_ns {
                report.files_reused += 1;
                return Ok(FileOutcome::Reused(id));
            }
        }
    }
    let bytes = match fs::read(&file.abs_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.warnings.push(IndexWarning {
                path: rel_path.clone(),
                message: format!("failed to read file: {error}"),
            });
            return Ok(FileOutcome::Skipped(SkipReason::Unreadable));
        }
    };
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(error) => {
            report.warnings.push(IndexWarning {
                path: rel_path.clone(),
                message: "not valid UTF-8; indexed with replacement characters".to_string(),
            });
            String::from_utf8_lossy(error.as_bytes()).into_owned()
        }
    };
    let content = content
        .strip_prefix('\u{feff}')
        .map(ToOwned::to_owned)
        .unwrap_or(content);
    let name = file
        .abs_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if walk::is_minified(&name, content.as_bytes()) {
        return Ok(FileOutcome::Skipped(SkipReason::Minified));
    }
    let sha = format!("{:x}", Sha256::digest(content.as_bytes()));
    if !full {
        if let Some((existing_id, existing_sha)) = db::file_sha(tx, rel_path)? {
            if existing_sha == sha {
                db::touch_file(tx, existing_id, file.size, file.mtime_ns)?;
                report.files_reused += 1;
                return Ok(FileOutcome::Reused(existing_id));
            }
            db::delete_file_cascade(tx, existing_id)?;
        }
    }
    let parsed = match parse_file(file.language, &content) {
        Ok(parsed) => parsed,
        Err(error) => {
            report.warnings.push(IndexWarning {
                path: rel_path.clone(),
                message: format!("failed to parse {}: {error}", file.language),
            });
            return Ok(FileOutcome::Skipped(SkipReason::Unreadable));
        }
    };
    let file_id = db::insert_file(
        tx,
        rel_path,
        file.language,
        &sha,
        content.lines().count(),
        file.size,
        file.mtime_ns,
    )?;
    if parsed.has_errors {
        db::mark_parse_errors(tx, file_id)?;
    }
    let symbol_ids = db::insert_symbols(tx, file_id, &parsed.symbols)?;
    let mut ids_by_name: HashMap<String, i64> = HashMap::with_capacity(symbol_ids.len());
    for (symbol, id) in parsed.symbols.iter().zip(&symbol_ids) {
        ids_by_name
            .entry(symbol.qualified_name.clone())
            .or_insert(*id);
    }
    report.references_indexed +=
        db::insert_references(tx, file_id, &parsed.references, &symbol_ids, &ids_by_name)?;
    db::insert_imports(tx, file_id, &parsed.imports)?;
    db::insert_bindings(tx, file_id, &parsed.bindings)?;
    db::insert_hints(tx, file_id, &parsed.hints)?;
    db::set_file_module(tx, file_id, parsed.module.as_deref())?;
    report.files_indexed += 1;
    report.symbols_indexed += symbol_ids.len();
    Ok(FileOutcome::Indexed(file_id))
}

/// Ignores the index but keeps the shareable project config committable.
pub const INDEX_DIR_GITIGNORE: &str = "*\n!.gitignore\n!config.toml\n";

fn ignore_index_dir(db_path: &Path) -> Result<()> {
    let Some(dir) = db_path.parent() else {
        return Ok(());
    };
    let ignore_file = dir.join(".gitignore");
    let outdated = fs::read_to_string(&ignore_file).map_or(true, |content| content == "*\n");
    if dir.file_name().is_some_and(|name| name == ".tessera") && outdated {
        fs::write(ignore_file, INDEX_DIR_GITIGNORE)?;
    }
    Ok(())
}

pub use crate::extract::{parse_file, ParsedFile};
