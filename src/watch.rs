use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::config::TesseraConfig;
use crate::indexer::{self, IndexOptions};
use crate::walk::{self, WalkOptions};

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
    index_once(&root, db_path, options.index_options.clone())?;

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
        index_once(&root, db_path, options.index_options.clone())?;
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
    if !report.warnings.is_empty() {
        eprintln!("[watch] indexed with {} warning(s)", report.warnings.len());
        for warning in report.warnings.iter().take(5) {
            eprintln!("  {}: {}", warning.path, warning.message);
        }
    }
    Ok(())
}

fn fingerprint_sources(root: &Path) -> Result<String> {
    let config = TesseraConfig::load(root);
    let options = WalkOptions {
        sniff_content: false,
        ..WalkOptions::default()
    };
    let discovery = walk::discover(root, &config, &options)?;
    let mut hasher = Sha256::new();
    for file in discovery.files {
        hasher.update(file.rel_path.as_bytes());
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.mtime_ns.to_le_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn fingerprint_changes_when_source_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("app.ts");
        fs::write(&path, "export function a() { return 1; }\n").unwrap();

        let first = fingerprint_sources(temp.path()).unwrap();
        // Coarse filesystem clocks can stamp two quick writes with the same mtime.
        std::thread::sleep(Duration::from_millis(20));
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
