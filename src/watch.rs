use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::fresh;
use crate::indexer::{self, IndexOptions, IndexReport};
use crate::types::Language;
use crate::workspace::Workspace;

const IGNORED_DIRS: &[&str] = &[".git", ".tessera", "node_modules", "target", "__pycache__"];

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub debounce: Duration,
    pub index_options: IndexOptions,
    pub once: bool,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(200),
            index_options: IndexOptions::default(),
            once: false,
        }
    }
}

/// Keeps the index fresh while the returned guard is alive.
pub struct WatchGuard {
    _debouncer: Debouncer<RecommendedWatcher>,
}

pub fn start(ws: Workspace, debounce: Duration) -> Result<WatchGuard> {
    let root = ws.root.canonicalize().unwrap_or_else(|_| ws.root.clone());
    let watched_root = root.clone();
    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };
        let paths: Vec<PathBuf> = events.into_iter().map(|event| event.path).collect();
        if let Err(error) = apply_changes(&ws, &watched_root, &paths) {
            eprintln!("tessera watch: {error:#}");
        }
    })?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;
    Ok(WatchGuard {
        _debouncer: debouncer,
    })
}

fn apply_changes(ws: &Workspace, root: &Path, paths: &[PathBuf]) -> Result<()> {
    let mut files = Vec::new();
    for path in paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let Ok(rel) = canonical
            .strip_prefix(root)
            .or_else(|_| path.strip_prefix(root))
        else {
            continue;
        };
        if rel.components().any(|c| match c {
            Component::Normal(name) => IGNORED_DIRS.contains(&name.to_string_lossy().as_ref()),
            _ => false,
        }) {
            continue;
        }
        let is_source = rel
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Language::from_extension)
            .is_some();
        if !is_source || path.is_dir() {
            // Directory renames and unknown changes are cheaper to reconcile with one stat walk.
            fresh::ensure(ws)?;
            return Ok(());
        }
        files.push(rel.to_string_lossy().replace('\\', "/"));
    }
    if !files.is_empty() {
        files.sort();
        files.dedup();
        indexer::index_files(ws, &files)?;
    }
    Ok(())
}

pub fn watch_path(root: &Path, db_path: &Path, options: WatchOptions) -> Result<()> {
    let ws = Workspace {
        root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        db_path: db_path.to_path_buf(),
    };
    let report = indexer::index_path_with(&ws.root, &ws.db_path, options.index_options)?;
    print_report(&report);
    if options.once {
        return Ok(());
    }
    let _guard = start(ws.clone(), options.debounce)?;
    eprintln!("[watch] watching {}", ws.root.display());
    let (_keep_alive, wait) = channel::<()>();
    let _ = wait.recv();
    Ok(())
}

fn print_report(report: &IndexReport) {
    println!(
        "[watch] indexed {} files (+{} reused, -{} removed, {} skipped) in {}ms",
        report.files_indexed,
        report.files_reused,
        report.files_removed,
        report.files_skipped,
        report.elapsed_ms
    );
}
