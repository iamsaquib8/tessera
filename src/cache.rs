//! Index cache files: one compressed snapshot of the index that CI moves
//! between machines. Tessera itself makes no network call; the file is
//! stored and restored by whatever the CI uses (the Action uses
//! `actions/cache`).
//!
//! A file is the line `tessera-cache`, one line of JSON (the [`Header`]),
//! then the index database compressed with zstd. Import refuses a file
//! from another Tessera version, schema, configuration or repository before
//! it touches the index, installs the rest in one SQLite transaction, then
//! re-indexes the files whose content differs from the working tree.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{self, APPLICATION_ID, SCHEMA_VERSION};
use crate::fresh;
use crate::indexer::{self, IndexLock, IndexOptions};
use crate::workspace::Workspace;

/// The first line of every cache file.
pub const MAGIC: &[u8; 14] = b"tessera-cache\n";
/// The layout this build writes and reads.
pub const FORMAT: u64 = 1;
const MAX_HEADER_BYTES: u64 = 64 * 1024;
/// zstd's default level: a few times smaller than the database, and fast
/// enough that compressing costs less than the upload it saves.
const ZSTD_LEVEL: i32 = 3;
const INSTALL_ATTEMPTS: usize = 50;

/// What a cache file says about the index it holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub format: u64,
    /// The Tessera version that wrote it.
    pub version: String,
    /// The index schema (`PRAGMA user_version`).
    pub schema: i64,
    /// The fingerprint freshness rebuilds on: version, `.tessera/config.toml`
    /// and excludes.
    pub fingerprint: String,
    /// The `--exclude` globs the index was built with.
    pub excludes: Vec<String>,
    /// `git rev-parse HEAD^{tree}` where it was written; `None` outside git.
    pub tree: Option<String>,
    /// The indexed root relative to the top of its git repository: `.` for
    /// the top, `packages/api` for a package of a monorepo.
    pub root: String,
    /// Indexed files.
    pub files: usize,
    /// Bytes of the uncompressed database, and their SHA-256.
    pub size: u64,
    pub sha256: String,
}

/// An import refused before the index was touched.
#[derive(Debug)]
pub struct Refused(pub String);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

fn refused(message: String) -> anyhow::Error {
    Refused(message).into()
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    /// The file written.
    pub exported: String,
    pub tree: Option<String>,
    pub version: String,
    pub schema: i64,
    pub root: String,
    pub files: usize,
    /// Size of the cache file.
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub imported: bool,
    /// The file read.
    pub file: String,
    pub tree: Option<String>,
    pub version: String,
    pub schema: i64,
    /// Files the cache held.
    pub files: usize,
    /// Files re-indexed because their content differs from the cache's, or
    /// the cache lacks them.
    pub reindexed: usize,
    /// Files whose content matched the cache's.
    pub unchanged: usize,
    /// Files the cache held that the working tree does not.
    pub removed: usize,
}

/// Writes the index of `ws`, brought up to date first, to `out`. The file
/// appears whole or not at all.
pub fn export(ws: &Workspace, out: &Path) -> Result<ExportReport> {
    fresh::ensure(ws)?;
    let conn = db::open_ro(&ws.db_path)?;
    let fingerprint = indexer::stored_fingerprint(&conn)?
        .context("the index has no fingerprint; run `tessera index` first")?;
    let excludes = indexer::persisted_excludes(&conn)?;
    let files = db::count_files(&conn)?;
    let dir = parent_dir(out);
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let scratch = tempfile::Builder::new()
        .prefix(".tessera-cache-")
        .tempdir_in(&dir)
        .with_context(|| format!("cannot write in {}", dir.display()))?;
    let snapshot = scratch.path().join("index.db");
    // A consistent, compacted copy, even while another process writes.
    conn.execute(
        "VACUUM INTO ?1",
        [snapshot
            .to_str()
            .context("the cache path is not valid UTF-8")?],
    )?;
    drop(conn);
    let (size, sha256) = digest(&snapshot)?;
    let header = Header {
        format: FORMAT,
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: SCHEMA_VERSION,
        fingerprint,
        excludes,
        tree: git(
            &ws.root,
            &["rev-parse", "--verify", "--quiet", "HEAD^{tree}"],
        ),
        root: root_marker(&ws.root),
        files,
        size,
        sha256,
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(".tessera-cache-");
    // Readable like any file the user writes, not private like a temp file.
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o644));
    let mut file = builder.tempfile_in(&dir)?;
    {
        let mut writer = BufWriter::new(file.as_file_mut());
        writer.write_all(MAGIC)?;
        serde_json::to_writer(&mut writer, &header)?;
        writer.write_all(b"\n")?;
        let mut encoder = zstd::Encoder::new(writer, ZSTD_LEVEL)?;
        encoder.include_checksum(true)?;
        io::copy(&mut File::open(&snapshot)?, &mut encoder)?;
        encoder.finish()?.flush()?;
    }
    file.as_file().sync_all()?;
    let bytes = file.as_file().metadata()?.len();
    file.persist(out)
        .map_err(|error| error.error)
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(ExportReport {
        exported: out.display().to_string(),
        tree: header.tree,
        version: header.version,
        schema: header.schema,
        root: header.root,
        files,
        bytes,
    })
}

/// Installs the index in `file` as the index of `ws`, then re-indexes what
/// differs from the working tree. A [`Refused`] error leaves the index as
/// it was.
pub fn import(ws: &Workspace, file: &Path) -> Result<ImportReport> {
    let display = file.display().to_string();
    let source = match File::open(file) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(refused(format!("no cache file at {display}")))
        }
        Err(error) => return Err(refused(format!("cannot read {display}: {error}"))),
    };
    let mut reader = BufReader::new(source);
    let header = read_header(&mut reader, &display)?;
    check(ws, &header, &display)?;

    let dir = parent_dir(&ws.db_path);
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let scratch = tempfile::Builder::new()
        .prefix(".tessera-import-")
        .tempdir_in(&dir)?;
    let snapshot = scratch.path().join("index.db");
    unpack(reader, &snapshot, &header)
        .map_err(|error| refused(format!("{display} is damaged: {error}")))?;
    let source = open_snapshot(ws, &snapshot, &header, &display)?;

    let lock = IndexLock::acquire(&ws.db_path)?;
    install(&source, &ws.db_path)?;
    drop(source);
    let report = indexer::index_path_locked(&lock, &ws.root, &ws.db_path, IndexOptions::default())?;
    Ok(ImportReport {
        imported: true,
        file: display,
        tree: header.tree,
        version: header.version,
        schema: header.schema,
        files: header.files,
        reindexed: report.files_indexed,
        unchanged: report.files_reused,
        removed: report.files_removed,
    })
}

/// The magic line and the header, checked for a layout and a version this
/// build reads before anything else is trusted.
fn read_header(reader: &mut impl BufRead, display: &str) -> Result<Header> {
    let foreign = || refused(format!("{display} is not a Tessera index cache"));
    let mut magic = [0u8; MAGIC.len()];
    if reader.read_exact(&mut magic).is_err() || &magic != MAGIC {
        return Err(foreign());
    }
    let mut line = Vec::new();
    reader.take(MAX_HEADER_BYTES).read_until(b'\n', &mut line)?;
    let json = line.strip_suffix(b"\n").ok_or_else(foreign)?;
    let value: serde_json::Value = serde_json::from_slice(json).map_err(|_| foreign())?;
    let format = value["format"].as_u64().unwrap_or_default();
    if format != FORMAT {
        return Err(refused(format!(
            "{display} is in cache format {format}; this Tessera reads format {FORMAT}"
        )));
    }
    let version = value["version"].as_str().unwrap_or("?");
    let current = env!("CARGO_PKG_VERSION");
    if version != current {
        return Err(refused(format!(
            "{display} was written by Tessera {version}, and this is Tessera {current}: \
             an index from another version is never reused. Index from scratch, or \
             import a cache this version wrote."
        )));
    }
    serde_json::from_value(value)
        .map_err(|error| refused(format!("{display} is damaged: its header {error}")))
}

/// Refuses a cache of another schema, directory or configuration.
fn check(ws: &Workspace, header: &Header, display: &str) -> Result<()> {
    if header.schema != SCHEMA_VERSION {
        return Err(refused(format!(
            "{display} holds an index of schema v{}; this Tessera uses schema v{SCHEMA_VERSION}",
            header.schema
        )));
    }
    let root = root_marker(&ws.root);
    if header.root != root {
        return Err(refused(format!(
            "{display} is the index of {} in its repository, and this index is of {root}",
            header.root
        )));
    }
    if indexer::fingerprint(&ws.root, &header.excludes) != header.fingerprint {
        return Err(refused(format!(
            "{display} was built under another .tessera/config.toml; \
             index from scratch, or export a cache with this configuration"
        )));
    }
    // An index already here keeps the excludes it was built with.
    let local = db::open_ro(&ws.db_path)
        .ok()
        .and_then(|conn| indexer::persisted_excludes(&conn).ok());
    if let Some(local) = local.filter(|local| *local != header.excludes) {
        let list = |globs: &[String]| {
            if globs.is_empty() {
                "none".to_string()
            } else {
                globs.join(", ")
            }
        };
        return Err(refused(format!(
            "{display} was built with --exclude {}, and this index with --exclude {}; \
             run `tessera index --exclude …` to match, or `--reset-excludes`",
            list(&header.excludes),
            list(&local)
        )));
    }
    Ok(())
}

/// Decompresses the database into `path`, checking it against the header.
fn unpack(reader: impl BufRead, path: &Path, header: &Header) -> Result<()> {
    let mut decoder = zstd::Decoder::with_buffer(reader)?;
    let mut out = BufWriter::new(File::create(path)?);
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = decoder.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > header.size {
            anyhow::bail!("it is longer than its header says");
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])?;
    }
    out.flush()?;
    if size != header.size || format!("{:x}", hasher.finalize()) != header.sha256 {
        anyhow::bail!("its checksum does not match");
    }
    Ok(())
}

/// Opens the unpacked database, refuses it if it is not the index the
/// header describes or not of this repository, and makes it this
/// checkout's: its root, and every file's content checked again.
fn open_snapshot(
    ws: &Workspace,
    snapshot: &Path,
    header: &Header,
    display: &str,
) -> Result<Connection> {
    let conn = Connection::open(snapshot)?;
    let id: i64 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let schema = db::schema_version(&conn)?;
    let fingerprint = indexer::stored_fingerprint(&conn).ok().flatten();
    if id != APPLICATION_ID
        || schema != header.schema
        || fingerprint.as_deref() != Some(&header.fingerprint)
    {
        return Err(refused(format!(
            "{display} is damaged: it does not hold the index its header describes"
        )));
    }
    let paths: Vec<String> = conn
        .prepare("SELECT path FROM files")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let present = paths
        .iter()
        .filter(|path| ws.root.join(path).exists())
        .count();
    // A branch moves or deletes some files, never most of them.
    if present * 2 < paths.len() {
        return Err(refused(format!(
            "{display} is the index of another repository: {present} of its {} files exist under {}",
            paths.len(),
            ws.root.display()
        )));
    }
    // No stored stat matches: freshness compares every file's content.
    conn.execute_batch("UPDATE files SET mtime_ns = -1; UPDATE skipped_files SET mtime_ns = -1;")?;
    let root = ws.root.canonicalize().unwrap_or_else(|_| ws.root.clone());
    db::set_meta(&conn, "root", &root.to_string_lossy())?;
    Ok(conn)
}

/// Copies `source` over the index at `db_path` in one SQLite transaction:
/// a reader sees the old index or the new one, and a failure keeps the old.
fn install(source: &Connection, db_path: &Path) -> Result<()> {
    let mut target = db::open_rw(db_path)?;
    let backup = Backup::new(source, &mut target)?;
    for _ in 0..INSTALL_ATTEMPTS {
        match backup.step(-1)? {
            StepResult::Done => return Ok(()),
            StepResult::More => {}
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    anyhow::bail!(
        "{} stayed busy; the cache was not installed",
        db_path.display()
    )
}

fn digest(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let size = io::copy(&mut file, &mut hasher)?;
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_string())
}

/// Where `root` sits in its git repository: `.` at the top or outside git.
fn root_marker(root: &Path) -> String {
    git(root, &["rev-parse", "--show-prefix"])
        .map(|prefix| prefix.trim_end_matches('/').to_string())
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

impl fmt::Display for ExportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Wrote {}: {} files, {} bytes (Tessera {}, schema v{}, {}).",
            self.exported,
            self.files,
            self.bytes,
            self.version,
            self.schema,
            tree_text(self.tree.as_deref())
        )
    }
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Imported {} ({} files, Tessera {}, {}): re-indexed {} that differ from the working tree, removed {}.",
            self.file,
            self.files,
            self.version,
            tree_text(self.tree.as_deref()),
            self.reindexed,
            self.removed
        )
    }
}

fn tree_text(tree: Option<&str>) -> String {
    tree.map_or("not in git".to_string(), |tree| {
        format!("tree {}", &tree[..tree.len().min(12)])
    })
}
