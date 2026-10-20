use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::config::TesseraConfig;
use crate::db;
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
    pub excludes: Vec<String>,
}

pub fn index_path(root: &Path, db_path: &Path) -> Result<IndexReport> {
    index_path_with(root, db_path, IndexOptions::default())
}

const EXCLUDES_META_KEY: &str = "excludes";

pub fn index_path_with(root: &Path, db_path: &Path, options: IndexOptions) -> Result<IndexReport> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let conn = db::open_rw(db_path)?;
    ignore_index_dir(db_path)?;
    let config = TesseraConfig::load(&root);
    if options.full {
        db::reset(&conn)?;
    }
    db::set_meta(&conn, "root", &root.to_string_lossy())?;
    db::set_meta(
        &conn,
        EXCLUDES_META_KEY,
        &serde_json::to_string(&options.excludes)?,
    )?;

    let mut report = IndexReport::new(if options.full {
        IndexMode::Full
    } else {
        IndexMode::Incremental
    });
    let mut visited_ids: Vec<i64> = Vec::new();
    let tx = conn.unchecked_transaction()?;

    let discovery = walk::discover(&root, &config, &walk_options(&config, &options.excludes))?;
    report.warnings.extend(discovery.warnings);
    let mut skipped: Vec<(String, u64, i64, &str)> = discovery
        .skipped
        .iter()
        .map(|s| (s.rel_path.clone(), s.size, s.mtime_ns, s.reason.as_str()))
        .collect();

    for file in &discovery.files {
        match index_one(&tx, file, options.full, &mut report)? {
            FileOutcome::Indexed(id) | FileOutcome::Reused(id) => visited_ids.push(id),
            FileOutcome::Minified => skipped.push((
                file.rel_path.clone(),
                file.size,
                file.mtime_ns,
                SkipReason::Minified.as_str(),
            )),
            FileOutcome::Failed => {}
        }
    }

    if !options.full {
        report.files_removed = db::delete_files_not_in(&tx, &visited_ids)?;
    }
    report.files_skipped = skipped.len();
    db::replace_skipped(&tx, &skipped)?;
    tx.commit()?;

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

pub fn index_files(ws: &Workspace, rel_paths: &[String]) -> Result<IndexReport> {
    let started = Instant::now();
    let root = ws
        .root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", ws.root.display()))?;
    let conn = db::open_rw(&ws.db_path)?;
    let config = TesseraConfig::load(&root);
    let options = walk_options(&config, &persisted_excludes(&conn)?);
    let mut report = IndexReport::new(IndexMode::Incremental);
    let tx = conn.unchecked_transaction()?;
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
        if index_one(&tx, &file, false, &mut report)? == FileOutcome::Minified {
            if let Some((id, _)) = db::file_sha(&tx, &rel_path)? {
                db::delete_file_cascade(&tx, id)?;
            }
            db::upsert_skipped(
                &tx,
                &rel_path,
                file.size,
                file.mtime_ns,
                SkipReason::Minified.as_str(),
            )?;
            report.files_skipped += 1;
        }
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileOutcome {
    Indexed(i64),
    Reused(i64),
    Minified,
    Failed,
}

fn index_one(
    tx: &Connection,
    file: &SourceFile,
    full: bool,
    report: &mut IndexReport,
) -> Result<FileOutcome> {
    let rel_path = &file.rel_path;
    let content = match fs::read_to_string(&file.abs_path) {
        Ok(content) => content,
        Err(error) => {
            report.warnings.push(IndexWarning {
                path: rel_path.clone(),
                message: format!("failed to read file: {error}"),
            });
            return Ok(FileOutcome::Failed);
        }
    };
    let name = file
        .abs_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    if walk::is_minified(&name, content.as_bytes()) {
        return Ok(FileOutcome::Minified);
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
            return Ok(FileOutcome::Failed);
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
    let symbol_ids = db::insert_symbols(tx, file_id, &parsed.symbols)?;
    let mut ids_by_name: HashMap<String, i64> = HashMap::with_capacity(symbol_ids.len());
    for (symbol, id) in parsed.symbols.iter().zip(&symbol_ids) {
        ids_by_name
            .entry(symbol.qualified_name.clone())
            .or_insert(*id);
    }
    report.references_indexed +=
        db::insert_references(tx, file_id, &parsed.references, &ids_by_name)?;
    db::insert_imports(tx, file_id, &parsed.imports)?;
    report.files_indexed += 1;
    report.symbols_indexed += symbol_ids.len();
    Ok(FileOutcome::Indexed(file_id))
}

fn ignore_index_dir(db_path: &Path) -> Result<()> {
    let Some(dir) = db_path.parent() else {
        return Ok(());
    };
    let ignore_file = dir.join(".gitignore");
    if dir.file_name().is_some_and(|name| name == ".tessera") && !ignore_file.exists() {
        fs::write(ignore_file, "*\n")?;
    }
    Ok(())
}

pub use crate::extract::{parse_file, ParsedFile};
