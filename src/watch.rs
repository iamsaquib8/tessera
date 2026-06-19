use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::indexer::{self, IndexOptions};
use crate::types::Language;

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub poll_interval: Duration,
    pub debounce: Duration,
    pub index_options: IndexOptions,
    pub once: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            debounce: Duration::from_millis(250),
            index_options: IndexOptions::default(),
            once: false,
        }
    }
}

pub fn watch_path(root: &Path, db_path: &Path, options: WatchOptions) -> Result<()> {
    let root = root.canonicalize().unwrap_or_else(|_| PathBuf::from(root));
    let mut last_fingerprint = fingerprint_sources(&root)?;
    index_once(&root, db_path, options.index_options)?;

    if options.once {
        return Ok(());
    }

    println!(
        "[watch] watching {} every {}ms",
        root.display(),
        options.poll_interval.as_millis()
    );

    loop {
        thread::sleep(options.poll_interval);
        let current = fingerprint_sources(&root)?;
        if current == last_fingerprint {
            continue;
        }

        thread::sleep(options.debounce);
        let settled = fingerprint_sources(&root)?;
        if settled == last_fingerprint {
            continue;
        }

        last_fingerprint = settled;
        index_once(&root, db_path, options.index_options)?;
    }
}

fn index_once(root: &Path, db_path: &Path, options: IndexOptions) -> Result<()> {
    let report = indexer::index_path_with(root, db_path, options)?;
    let mode = match report.mode {
        indexer::IndexMode::Full => "full",
        indexer::IndexMode::Incremental => "incremental",
    };
    println!(
        "[watch:{mode}] indexed {} files (+{} reused, -{} removed), {} symbols, {} references into {} in {}ms",
        report.files_indexed,
        report.files_reused,
        report.files_removed,
        report.symbols_indexed,
        report.references_indexed,
        db_path.display(),
        report.elapsed_ms
    );
    Ok(())
}

fn fingerprint_sources(root: &Path) -> Result<String> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(should_enter)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if language_for_path(path).is_none() {
            continue;
        }
        let Ok(content) = fs::read(path) else {
            continue;
        };
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push((rel_path, content));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (path, content) in entries {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(content);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn should_enter(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git"
            | ".hg"
            | ".svn"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".tessera"
    )
}

fn language_for_path(path: &Path) -> Option<Language> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(Language::from_extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_changes_when_source_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("app.ts");
        fs::write(&path, "export function a() { return 1; }\n").unwrap();

        let first = fingerprint_sources(temp.path()).unwrap();
        fs::write(&path, "export function a() { return 2; }\n").unwrap();
        let second = fingerprint_sources(temp.path()).unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn fingerprint_ignores_unindexed_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "one\n").unwrap();

        let first = fingerprint_sources(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "two\n").unwrap();
        let second = fingerprint_sources(temp.path()).unwrap();

        assert_eq!(first, second);
    }
}
