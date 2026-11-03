use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::config::TesseraConfig;
use crate::db;
use crate::resolve::{self, Before, ResolveReport, ResolveScope};
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
/// Parsing runs on this many threads at most; storing stays on one.
const MAX_WORKERS: usize = 8;
/// Fewer files than this are parsed on the calling thread.
const PARALLEL_THRESHOLD: usize = 32;
/// tree-sitter recursion on deeply nested input needs more than the default.
const WORKER_STACK_BYTES: usize = 8 << 20;
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
    let mut before = Before::default();
    let outcomes = index_discovered(&tx, &discovery.files, full, &mut report, &mut before)?;
    for (file, outcome) in discovery.files.iter().zip(outcomes) {
        match outcome {
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
        let visited: HashSet<i64> = visited_ids.iter().copied().collect();
        let indexed: Vec<i64> = tx
            .prepare("SELECT id FROM files")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in indexed.into_iter().filter(|id| !visited.contains(id)) {
            before.record(&tx, id)?;
        }
        report.files_removed = db::delete_files_not_in(&tx, &visited_ids)?;
    }
    report.files_skipped = skipped.len();
    db::replace_skipped(&tx, &skipped)?;
    if full {
        db::fts_rebuild(&tx)?;
    }
    let scope = if full || resolve::pending(&tx)? {
        Some(ResolveScope::All)
    } else if !changed_ids.is_empty() || report.files_removed > 0 {
        Some(ResolveScope::Changed(changed_ids, before))
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
    let mut before = Before::default();
    for rel_path in rel_paths {
        let rel_path = rel_path.trim_start_matches("./").replace('\\', "/");
        let status = walk::describe(&root, &rel_path, &config, &options)?;
        let FileStatus::Source(file) = status else {
            if let Some((id, _)) = db::file_sha(&tx, &rel_path)? {
                before.record(&tx, id)?;
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
        let known_sha = db::file_sha(&tx, &rel_path)?.map(|(_, sha)| sha);
        let prepared = prepare(&file, known_sha.as_deref());
        match store(&tx, &file, prepared, false, &mut report, &mut before)? {
            FileOutcome::Skipped(reason) => {
                if let Some((id, _)) = db::file_sha(&tx, &rel_path)? {
                    before.record(&tx, id)?;
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
            ResolveScope::Changed(changed_ids, before),
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

/// What a worker learns about a file without touching the database.
enum Prepared {
    Unreadable(String),
    Minified,
    /// The content hash matches the indexed copy; only the stat changed.
    SameContent,
    Parsed {
        sha: String,
        loc: usize,
        lossy: bool,
        parsed: Box<std::result::Result<ParsedFile, String>>,
    },
}

/// Reads, decodes and parses `file`. Pure, so it runs on worker threads.
fn prepare(file: &SourceFile, known_sha: Option<&str>) -> Prepared {
    let bytes = match fs::read(&file.abs_path) {
        Ok(bytes) => bytes,
        Err(error) => return Prepared::Unreadable(error.to_string()),
    };
    let (content, lossy) = match String::from_utf8(bytes) {
        Ok(content) => (content, false),
        Err(error) => (String::from_utf8_lossy(error.as_bytes()).into_owned(), true),
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
        return Prepared::Minified;
    }
    let sha = format!("{:x}", Sha256::digest(content.as_bytes()));
    if known_sha == Some(sha.as_str()) {
        return Prepared::SameContent;
    }
    Prepared::Parsed {
        loc: content.lines().count(),
        parsed: Box::new(
            parse_file_at(file.language, &file.rel_path, &content)
                .map_err(|error| error.to_string()),
        ),
        sha,
        lossy,
    }
}

/// Prepares `pending` files on worker threads and hands each result to
/// `store` in the order given, so ids and output stay deterministic.
fn prepare_all(
    files: &[SourceFile],
    pending: &[(usize, Option<String>)],
    mut store: impl FnMut(usize, Prepared) -> Result<()>,
) -> Result<()> {
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_WORKERS);
    if pending.len() < PARALLEL_THRESHOLD || workers < 2 {
        for (index, known_sha) in pending {
            store(*index, prepare(&files[*index], known_sha.as_deref()))?;
        }
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    // Each prepared file holds a permit until it is stored, so one slow file
    // cannot let the others pile up parsed results in memory.
    let window = workers * 16;
    let (permit_sender, permits) = mpsc::sync_channel::<()>(window);
    for _ in 0..window {
        permit_sender.send(())?;
    }
    let permits = std::sync::Mutex::new(permits);
    std::thread::scope(|scope| -> Result<()> {
        // Owned here so an early error drops it and unblocks waiting workers.
        let permit_sender = permit_sender;
        let (sender, receiver) = mpsc::sync_channel::<(usize, Prepared)>(window);
        for _ in 0..workers {
            let sender = sender.clone();
            let next = &next;
            let permits = &permits;
            std::thread::Builder::new()
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || loop {
                    let permit = permits.lock().map(|p| p.recv());
                    if !matches!(permit, Ok(Ok(()))) {
                        return;
                    }
                    let slot = next.fetch_add(1, Ordering::Relaxed);
                    let Some((index, known_sha)) = pending.get(slot) else {
                        return;
                    };
                    let prepared = prepare(&files[*index], known_sha.as_deref());
                    if sender.send((slot, prepared)).is_err() {
                        return;
                    }
                })?;
        }
        drop(sender);
        let mut waiting: BTreeMap<usize, Prepared> = BTreeMap::new();
        let mut expected = 0;
        for (slot, prepared) in receiver.iter() {
            waiting.insert(slot, prepared);
            while let Some(prepared) = waiting.remove(&expected) {
                store(pending[expected].0, prepared)?;
                expected += 1;
                let _ = permit_sender.send(());
            }
        }
        drop(permit_sender);
        Ok(())
    })
}

/// Writes one prepared file. `full` means the index was emptied first;
/// otherwise the rows it replaces are recorded in `before`.
fn store(
    tx: &Connection,
    file: &SourceFile,
    prepared: Prepared,
    full: bool,
    report: &mut IndexReport,
    before: &mut Before,
) -> Result<FileOutcome> {
    let rel_path = &file.rel_path;
    let warn = |report: &mut IndexReport, message: String| {
        report.warnings.push(IndexWarning {
            path: rel_path.clone(),
            message,
        })
    };
    let (sha, loc, parsed) = match prepared {
        Prepared::Unreadable(error) => {
            warn(report, format!("failed to read file: {error}"));
            return Ok(FileOutcome::Skipped(SkipReason::Unreadable));
        }
        Prepared::Minified => return Ok(FileOutcome::Skipped(SkipReason::Minified)),
        Prepared::SameContent => {
            if let Some((existing_id, _)) = db::file_sha(tx, rel_path)? {
                db::touch_file(tx, existing_id, file.size, file.mtime_ns)?;
                report.files_reused += 1;
                return Ok(FileOutcome::Reused(existing_id));
            }
            return Ok(FileOutcome::Skipped(SkipReason::Unreadable));
        }
        Prepared::Parsed {
            sha,
            loc,
            lossy,
            parsed,
        } => {
            if lossy {
                warn(
                    report,
                    "not valid UTF-8; indexed with replacement characters".to_string(),
                );
            }
            (sha, loc, parsed)
        }
    };
    if !full {
        if let Some((existing_id, _)) = db::file_sha(tx, rel_path)? {
            before.record(tx, existing_id)?;
            db::delete_file_cascade(tx, existing_id)?;
        }
    }
    let parsed = match *parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            warn(
                report,
                format!("failed to parse {}: {error}", file.language),
            );
            return Ok(FileOutcome::Skipped(SkipReason::Unreadable));
        }
    };
    let file_id = db::insert_file(
        tx,
        rel_path,
        file.language,
        &sha,
        loc,
        file.size,
        file.mtime_ns,
    )?;
    if parsed.has_errors {
        db::mark_parse_errors(tx, file_id)?;
    }
    let symbol_ids = db::insert_symbols(tx, file_id, &parsed.symbols)?;
    if !full {
        db::fts_insert_file(tx, file_id)?;
    }
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
    db::insert_locals(tx, file_id, &parsed.locals)?;
    db::set_file_module(tx, file_id, parsed.module.as_deref())?;
    db::insert_facts(tx, file_id, &parsed.facts, &parsed.symbols, &symbol_ids)?;
    report.files_indexed += 1;
    report.symbols_indexed += symbol_ids.len();
    Ok(FileOutcome::Indexed(file_id))
}

/// Unchanged files keep their rows; others are prepared and stored.
fn index_discovered(
    tx: &Connection,
    files: &[SourceFile],
    full: bool,
    report: &mut IndexReport,
    before: &mut Before,
) -> Result<Vec<FileOutcome>> {
    let mut outcomes: Vec<Option<FileOutcome>> = vec![None; files.len()];
    let mut pending = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let mut known_sha = None;
        if !full {
            if let Some((id, size, mtime_ns)) = db::file_stat(tx, &file.rel_path)? {
                if size == file.size && mtime_ns == file.mtime_ns {
                    report.files_reused += 1;
                    outcomes[index] = Some(FileOutcome::Reused(id));
                    continue;
                }
                known_sha = db::file_sha(tx, &file.rel_path)?.map(|(_, sha)| sha);
            }
        }
        pending.push((index, known_sha));
    }
    prepare_all(files, &pending, |index, prepared| {
        outcomes[index] = Some(store(tx, &files[index], prepared, full, report, before)?);
        Ok(())
    })?;
    Ok(outcomes.into_iter().flatten().collect())
}

/// Ignores the index but keeps the shareable project config committable.
pub const INDEX_DIR_GITIGNORE: &str =
    "*\n!.gitignore\n!config.toml\n!baseline.json\n!accepted.toml\n";
/// What earlier versions wrote, replaced when found.
const OLD_INDEX_DIR_GITIGNORES: &[&str] = &["*\n", "*\n!.gitignore\n!config.toml\n"];

pub fn ignore_index_dir(db_path: &Path) -> Result<()> {
    let Some(dir) = db_path.parent() else {
        return Ok(());
    };
    let ignore_file = dir.join(".gitignore");
    let outdated = fs::read_to_string(&ignore_file).map_or(true, |content| {
        OLD_INDEX_DIR_GITIGNORES.contains(&content.as_str())
    });
    if dir.file_name().is_some_and(|name| name == ".tessera") && outdated {
        fs::write(ignore_file, INDEX_DIR_GITIGNORE)?;
    }
    Ok(())
}

pub use crate::extract::{parse_file, parse_file_at, ParsedFile};
