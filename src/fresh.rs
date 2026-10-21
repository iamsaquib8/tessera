use std::collections::HashSet;
use std::fs::File;

use anyhow::Result;

use crate::config::TesseraConfig;
use crate::db::{self, DbError};
use crate::indexer::{self, IndexLock, IndexOptions, IndexReport};
use crate::walk::{self, SkipReason};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Freshness {
    Missing,
    Stale {
        changed: usize,
        added: usize,
        removed: usize,
    },
    /// Built by another tessera version or under a different config.
    Outdated,
    Fresh,
}

#[derive(Debug)]
pub enum Refresh {
    Indexed(IndexReport),
    /// Another process is writing the index; the existing one is still readable.
    Busy,
}

pub fn check(ws: &Workspace) -> Result<Freshness> {
    let conn = match db::open_ro(&ws.db_path) {
        Ok(conn) => conn,
        Err(DbError::Missing(_) | DbError::SchemaMismatch { .. }) => return Ok(Freshness::Missing),
        Err(error) => return Err(error.into()),
    };
    let config = TesseraConfig::load(&ws.root);
    let excludes = indexer::persisted_excludes(&conn)?;
    if indexer::stored_fingerprint(&conn)? != Some(indexer::fingerprint(&ws.root, &excludes)) {
        return Ok(Freshness::Outdated);
    }
    let options = indexer::walk_options(&config, &excludes);
    let discovery = walk::discover(&ws.root, &config, &options)?;
    let indexed = db::indexed_stats(&conn)?;

    let (mut changed, mut added) = (0, 0);
    let mut seen: HashSet<&str> = HashSet::new();
    let current = discovery
        .files
        .iter()
        .map(|f| (f.rel_path.as_str(), f.size, f.mtime_ns))
        .chain(
            discovery
                .skipped
                .iter()
                .filter(|s| s.reason != SkipReason::Excluded)
                .map(|s| (s.rel_path.as_str(), s.size, s.mtime_ns)),
        );
    for (path, size, mtime_ns) in current {
        seen.insert(path);
        match indexed.get(path) {
            None => added += 1,
            Some(&stat) if stat != (size, mtime_ns) => changed += 1,
            Some(_) => {}
        }
    }
    let removed = indexed
        .keys()
        .filter(|path| !seen.contains(path.as_str()))
        .count();
    // Fixing permissions changes no size or mtime, so retry unreadable files directly.
    changed += db::unreadable_paths(&conn)?
        .iter()
        .filter(|path| seen.contains(path.as_str()) && File::open(ws.root.join(path)).is_ok())
        .count();
    Ok(if changed + added + removed == 0 {
        Freshness::Fresh
    } else {
        Freshness::Stale {
            changed,
            added,
            removed,
        }
    })
}

/// Brings the index up to date, waiting for any other writer to finish first.
pub fn ensure(ws: &Workspace) -> Result<Option<IndexReport>> {
    if check(ws)? == Freshness::Fresh {
        return Ok(None);
    }
    let report = indexer::index_path_with(&ws.root, &ws.db_path, IndexOptions::default())?;
    Ok(Some(report))
}

/// Re-indexes a stale index unless another process is already doing so and the
/// existing index is readable; waits only when there is nothing to serve yet.
pub fn refresh(ws: &Workspace) -> Result<Refresh> {
    let lock = match IndexLock::try_acquire(&ws.db_path)? {
        Some(lock) => lock,
        None if db::open_ro(&ws.db_path).is_ok() => return Ok(Refresh::Busy),
        None => IndexLock::acquire(&ws.db_path)?,
    };
    let report = indexer::index_path_locked(&lock, &ws.root, &ws.db_path, IndexOptions::default())?;
    Ok(Refresh::Indexed(report))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;
    use crate::db;
    use crate::indexer::{self, IndexOptions};

    fn repo() -> (TempDir, Workspace) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/a.ts"),
            "export function a() { return b(); }\n",
        )
        .unwrap();
        fs::write(root.join("src/b.ts"), "export function b() { return 1; }\n").unwrap();
        let ws = Workspace::resolve(None, None, &root);
        (temp, ws)
    }

    fn file_id(ws: &Workspace, path: &str) -> Option<i64> {
        db::file_sha(&db::open_ro(&ws.db_path).unwrap(), path)
            .unwrap()
            .map(|(id, _)| id)
    }

    fn edit(ws: &Workspace, path: &str, content: &str) {
        std::thread::sleep(Duration::from_millis(20));
        fs::write(ws.root.join(path), content).unwrap();
    }

    #[test]
    fn missing_db_is_missing() {
        let (_temp, ws) = repo();
        assert_eq!(check(&ws).unwrap(), Freshness::Missing);
    }

    #[test]
    fn ensure_indexes_once_then_reports_fresh() {
        let (_temp, ws) = repo();
        assert!(ensure(&ws).unwrap().is_some());
        assert!(ws.root.join(".tessera/.gitignore").exists());
        assert_eq!(check(&ws).unwrap(), Freshness::Fresh);
        assert!(ensure(&ws).unwrap().is_none());
    }

    #[test]
    fn edited_added_and_removed_files_are_stale() {
        let (_temp, ws) = repo();
        ensure(&ws).unwrap();
        edit(&ws, "src/a.ts", "export function a() { return 2; }\n");
        fs::write(ws.root.join("src/c.ts"), "export function c() {}\n").unwrap();
        fs::remove_file(ws.root.join("src/b.ts")).unwrap();
        assert_eq!(
            check(&ws).unwrap(),
            Freshness::Stale {
                changed: 1,
                added: 1,
                removed: 1
            }
        );
    }

    #[test]
    fn skipped_and_excluded_files_do_not_keep_index_stale() {
        let (_temp, ws) = repo();
        fs::write(ws.root.join("src/vendor.js"), "a();".repeat(5_000)).unwrap();
        fs::create_dir_all(ws.root.join("legacy")).unwrap();
        fs::write(ws.root.join("legacy/old.ts"), "export function old() {}\n").unwrap();
        indexer::index_path_with(
            &ws.root,
            &ws.db_path,
            IndexOptions {
                excludes: Some(vec!["legacy/**".to_string()]),
                ..IndexOptions::default()
            },
        )
        .unwrap();
        assert_eq!(check(&ws).unwrap(), Freshness::Fresh);
        assert!(ensure(&ws).unwrap().is_none());
        assert!(file_id(&ws, "legacy/old.ts").is_none());
    }

    #[test]
    fn index_files_touches_only_named_files() {
        let (_temp, ws) = repo();
        ensure(&ws).unwrap();
        let a_before = file_id(&ws, "src/a.ts").unwrap();
        let b_before = file_id(&ws, "src/b.ts").unwrap();
        edit(&ws, "src/a.ts", "export function a2() {}\n");
        indexer::index_files(&ws, &["src/a.ts".to_string()]).unwrap();
        assert_ne!(file_id(&ws, "src/a.ts").unwrap(), a_before);
        assert_eq!(file_id(&ws, "src/b.ts").unwrap(), b_before);
        let conn = db::open_ro(&ws.db_path).unwrap();
        assert!(db::resolve_symbol(&conn, "a2").unwrap().is_some());
        assert_eq!(check(&ws).unwrap(), Freshness::Fresh);
    }

    #[test]
    fn index_files_removes_deleted_files() {
        let (_temp, ws) = repo();
        ensure(&ws).unwrap();
        fs::remove_file(ws.root.join("src/b.ts")).unwrap();
        indexer::index_files(&ws, &["src/b.ts".to_string()]).unwrap();
        assert!(file_id(&ws, "src/b.ts").is_none());
    }
}
