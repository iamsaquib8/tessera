use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::bloom::BloomFilter;
use crate::config::TesseraConfig;
use crate::db;
use crate::snapshot;
use crate::walk::{self, WalkOptions};

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

#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub full: bool,
    pub build_snapshot: bool,
    pub excludes: Vec<String>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            full: false,
            build_snapshot: true,
            excludes: Vec::new(),
        }
    }
}

pub fn index_path(root: &Path, db_path: &Path) -> Result<IndexReport> {
    index_path_with(root, db_path, IndexOptions::default())
}

pub fn index_path_with(root: &Path, db_path: &Path, options: IndexOptions) -> Result<IndexReport> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let conn = db::open(db_path)?;
    let config = TesseraConfig::load(&root);
    if options.full {
        db::reset(&conn)?;
    }
    db::set_meta(&conn, "root", &root.to_string_lossy())?;

    let mode = if options.full {
        IndexMode::Full
    } else {
        IndexMode::Incremental
    };

    let mut report = IndexReport {
        files_indexed: 0,
        files_reused: 0,
        files_removed: 0,
        symbols_indexed: 0,
        references_indexed: 0,
        files_skipped: 0,
        elapsed_ms: 0,
        mode,
        warnings: Vec::new(),
    };

    let mut visited_ids: Vec<i64> = Vec::new();

    let tx = conn.unchecked_transaction()?;

    let walk_options = WalkOptions {
        max_file_bytes: config
            .max_file_bytes
            .unwrap_or(WalkOptions::default().max_file_bytes),
        excludes: options.excludes.clone(),
        sniff_content: false,
    };
    let discovery = walk::discover(&root, &config, &walk_options)?;
    report.warnings.extend(discovery.warnings);
    report.files_skipped = discovery.skipped.len();

    for file in discovery.files {
        let rel_path = file.rel_path;
        let language = file.language;
        let content = match fs::read_to_string(&file.abs_path) {
            Ok(c) => c,
            Err(error) => {
                report.warnings.push(IndexWarning {
                    path: rel_path,
                    message: format!("failed to read file: {error}"),
                });
                continue;
            }
        };
        let name = file
            .abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if walk::is_minified(&name, content.as_bytes()) {
            report.files_skipped += 1;
            continue;
        }
        let sha = format!("{:x}", Sha256::digest(content.as_bytes()));

        // Incremental: short-circuit when sha is unchanged.
        if !options.full {
            if let Some((existing_id, existing_sha)) = db::file_sha(&tx, &rel_path)? {
                if existing_sha == sha {
                    visited_ids.push(existing_id);
                    report.files_reused += 1;
                    continue;
                }
                db::delete_file_cascade(&tx, existing_id)?;
            }
        }

        let parsed = match parse_file(language, &content) {
            Ok(parsed) => parsed,
            Err(error) => {
                report.warnings.push(IndexWarning {
                    path: rel_path,
                    message: format!("failed to parse {language}: {error}"),
                });
                continue;
            }
        };

        let file_id = db::insert_file(&tx, &rel_path, language, &sha, content.lines().count())?;
        let symbol_ids = db::insert_symbols(&tx, file_id, &parsed.symbols)?;
        let ref_count = db::insert_references(&tx, file_id, &parsed.references)?;
        let _ = db::insert_imports(&tx, file_id, &parsed.imports)?;

        visited_ids.push(file_id);
        report.files_indexed += 1;
        report.symbols_indexed += symbol_ids.len();
        report.references_indexed += ref_count;
    }

    // Reconcile deletes only in incremental mode (full mode already reset).
    if !options.full {
        report.files_removed = db::delete_files_not_in(&tx, &visited_ids)?;
    }

    tx.commit()?;

    rebuild_bloom(&conn)?;

    if options.build_snapshot {
        let snapshot_path = snapshot_path(db_path);
        let _ = snapshot::build(&conn, &snapshot_path);
    }

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn snapshot_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("snapshot.bin")
}

fn rebuild_bloom(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT name, qualified_name FROM symbols")?;
    let mut names: Vec<String> = Vec::new();
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let qualified: String = row.get(1)?;
        Ok((name, qualified))
    })?;
    for row in rows {
        let (name, qualified) = row?;
        names.push(name);
        names.push(qualified);
    }
    let expected = names.len().max(64);
    let mut bloom = BloomFilter::for_expected(expected, 0.01);
    for name in &names {
        bloom.insert(name);
    }
    let bytes = bloom.to_bytes();
    db::set_meta_blob(conn, "bloom_symbols", &bytes)?;
    Ok(())
}

pub use crate::extract::{parse_file, ParsedFile};
