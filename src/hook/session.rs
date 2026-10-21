//! Per-session hook state, kept beside the index (never in the repository's
//! tracked files) and locked so concurrent hooks of one session take turns.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Session files untouched for this long are removed at the next session start.
const EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// `HEAD` when the session was first seen; `None` outside git or before
    /// the first commit.
    pub baseline: Option<String>,
    /// Files already changed when the session was first seen, by content
    /// hash. Stop leaves them out unless they change again.
    pub dirty: BTreeMap<String, String>,
    /// Files the agent edited through its tools.
    pub edited: BTreeSet<String>,
    /// Symbols the pre-edit note already announced, as `path#qualified name`.
    pub announced: BTreeSet<String>,
    /// Blocking findings after the last edit, as `id path message`.
    pub open: BTreeSet<String>,
    /// Stop blocks since the turn began.
    pub stop_blocks: u32,
    /// Hash of the last edit event, to answer a duplicate only once.
    pub last_edit: Option<String>,
}

/// The locked session file; the lock is released when this is dropped.
pub struct Store {
    path: PathBuf,
    _lock: File,
}

impl Store {
    /// Opens (or starts) the session `id` stored next to `db_path`. The bool
    /// is true when the session is new.
    pub fn open(db_path: &Path, id: &str) -> Result<(Self, Session, bool)> {
        let dir = sessions_dir(db_path);
        fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
        let name = file_name(id);
        let lock_path = dir.join(format!("{name}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open {}", lock_path.display()))?;
        lock.lock()
            .with_context(|| format!("failed to lock {}", lock_path.display()))?;
        let path = dir.join(format!("{name}.json"));
        let (session, new) = match fs::read(&path) {
            Ok(bytes) => (serde_json::from_slice(&bytes).unwrap_or_default(), false),
            Err(_) => (Session::default(), true),
        };
        Ok((Self { path, _lock: lock }, session, new))
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(session)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

pub fn sessions_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("sessions")
}

/// Removes session files nobody has touched for a week.
pub fn expire(db_path: &Path) {
    let Ok(entries) = fs::read_dir(sessions_dir(db_path)) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() > EXPIRY);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Session ids become file names: kept as they are when safe, hashed otherwise.
fn file_name(id: &str) -> String {
    let safe = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if safe {
        id.to_string()
    } else {
        hash(id.as_bytes())[..32].to_string()
    }
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The content hash Stop compares against `Session::dirty`.
pub fn content_hash(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| hash(&bytes))
        .unwrap_or_else(|_| "deleted".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_ids_are_hashed() {
        assert_eq!(file_name("abc-123_x"), "abc-123_x");
        assert_eq!(file_name("../etc/passwd").len(), 32);
        assert_eq!(file_name("").len(), 32);
    }

    #[test]
    fn state_survives_a_round_trip() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = temp.path().join("tessera.db");
        {
            let (store, mut session, new) = Store::open(&db, "s").unwrap();
            assert!(new);
            session.edited.insert("a.ts".into());
            store.save(&session).unwrap();
        }
        let (_, session, new) = Store::open(&db, "s").unwrap();
        assert!(!new);
        assert!(session.edited.contains("a.ts"));
    }
}
