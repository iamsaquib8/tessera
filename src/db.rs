use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::extract::truncate_chars;
use crate::types::{
    IndexedImport, IndexedReference, IndexedSymbol, Language, ReferenceRecord, SymbolRecord,
};

pub const SCHEMA_VERSION: i64 = 4;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);
const CONTEXT_MAX_CHARS: usize = 200;

#[derive(Debug)]
pub enum DbError {
    Missing(PathBuf),
    SchemaMismatch { found: i64 },
    Sqlite(rusqlite::Error),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(
                f,
                "Tessera index not found at {path}. Run `tessera index . --db {path}` from the repository root.",
                path = path.display()
            ),
            Self::SchemaMismatch { found } => write!(
                f,
                "Tessera index uses schema v{found}, this binary needs v{SCHEMA_VERSION}. Run `tessera index . --full` to rebuild it."
            ),
            Self::Sqlite(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub fn open_rw(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    let version = user_version(&conn)?;
    if version != SCHEMA_VERSION {
        drop_all(&conn)?;
        create_schema(&conn)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(conn)
}

pub fn open_ro(path: &Path) -> std::result::Result<Connection, DbError> {
    if !path.exists() {
        return Err(DbError::Missing(path.to_path_buf()));
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn)?;
    let found = user_version(&conn)?;
    if found != SCHEMA_VERSION {
        return Err(DbError::SchemaMismatch { found });
    }
    Ok(conn)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")
}

pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(user_version(conn)?)
}

fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn drop_all(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let objects = |sql: &str| -> Result<Vec<(String, String)>> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    };
    for (kind, name) in objects(
        "SELECT type, name FROM sqlite_master WHERE type = 'trigger' \
         OR (type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%')",
    )? {
        let kind = if kind == "trigger" {
            "TRIGGER"
        } else {
            "TABLE"
        };
        conn.execute_batch(&format!("DROP {kind} IF EXISTS \"{name}\";"))?;
    }
    for (_, name) in objects(
        "SELECT type, name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )? {
        conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{name}\";"))?;
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            language TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            loc INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL,
            mtime_ns INTEGER NOT NULL,
            indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE skipped_files (
            path TEXT PRIMARY KEY,
            size_bytes INTEGER NOT NULL,
            mtime_ns INTEGER NOT NULL,
            reason TEXT NOT NULL
        );

        CREATE TABLE meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE symbols (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            qualified_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            signature TEXT NOT NULL,
            exported INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX idx_symbols_name ON symbols(name);
        CREATE INDEX idx_symbols_qualified ON symbols(qualified_name);
        CREATE INDEX idx_symbols_file ON symbols(file_id);

        CREATE TABLE refs (
            id INTEGER PRIMARY KEY,
            symbol_name TEXT NOT NULL,
            from_symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            line INTEGER NOT NULL,
            column INTEGER NOT NULL,
            context TEXT NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE INDEX idx_refs_symbol ON refs(symbol_name);
        CREATE INDEX idx_refs_from ON refs(from_symbol_id);
        CREATE INDEX idx_refs_file ON refs(file_id);

        CREATE TABLE edges (
            id INTEGER PRIMARY KEY,
            from_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
            to_symbol_name TEXT NOT NULL,
            to_symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
            kind TEXT NOT NULL,
            confidence TEXT NOT NULL DEFAULT 'name',
            weight REAL NOT NULL DEFAULT 1.0
        );

        CREATE INDEX idx_edges_to ON edges(to_symbol_name);
        CREATE INDEX idx_edges_to_id ON edges(to_symbol_id);
        CREATE INDEX idx_edges_from ON edges(from_symbol_id);

        CREATE TABLE imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            source TEXT NOT NULL,
            line INTEGER NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE INDEX idx_imports_file ON imports(file_id);
        CREATE INDEX idx_imports_source ON imports(source);

        CREATE VIRTUAL TABLE symbols_fts USING fts5(
            name, qualified_name,
            content='symbols', content_rowid='id',
            tokenize='trigram'
        );

        CREATE TRIGGER symbols_ai AFTER INSERT ON symbols BEGIN
            INSERT INTO symbols_fts(rowid, name, qualified_name)
            VALUES (new.id, new.name, new.qualified_name);
        END;

        CREATE TRIGGER symbols_ad AFTER DELETE ON symbols BEGIN
            INSERT INTO symbols_fts(symbols_fts, rowid, name, qualified_name)
            VALUES('delete', old.id, old.name, old.qualified_name);
        END;
        ",
    )?;
    Ok(())
}

pub fn reset(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DELETE FROM imports;
        DELETE FROM edges;
        DELETE FROM refs;
        DELETE FROM symbols;
        DELETE FROM files;
        DELETE FROM skipped_files;
        INSERT INTO symbols_fts(symbols_fts) VALUES('delete-all');
        ",
    )?;
    Ok(())
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?
    .execute(params![key, value])?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?
        .query_row(params![key], |row| row.get(0))
        .optional()
        .map_err(Into::into)
}

pub fn file_sha(conn: &Connection, path: &str) -> Result<Option<(i64, String)>> {
    conn.prepare_cached("SELECT id, sha256 FROM files WHERE path = ?1")?
        .query_row(params![path], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()
        .map_err(Into::into)
}

pub fn touch_file(conn: &Connection, file_id: i64, size_bytes: u64, mtime_ns: i64) -> Result<()> {
    conn.prepare_cached("UPDATE files SET size_bytes = ?2, mtime_ns = ?3 WHERE id = ?1")?
        .execute(params![file_id, size_bytes as i64, mtime_ns])?;
    Ok(())
}

pub fn delete_file_cascade(conn: &Connection, file_id: i64) -> Result<()> {
    conn.prepare_cached("DELETE FROM files WHERE id = ?1")?
        .execute(params![file_id])?;
    Ok(())
}

pub fn delete_files_not_in(conn: &Connection, retained_ids: &[i64]) -> Result<usize> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS retained_files(id INTEGER PRIMARY KEY);
         DELETE FROM retained_files;",
    )?;
    {
        let mut insert =
            conn.prepare_cached("INSERT OR IGNORE INTO retained_files(id) VALUES (?1)")?;
        for id in retained_ids {
            insert.execute(params![id])?;
        }
    }
    let removed = conn.execute(
        "DELETE FROM files WHERE id NOT IN (SELECT id FROM retained_files)",
        [],
    )?;
    conn.execute_batch("DELETE FROM retained_files;")?;
    Ok(removed)
}

pub fn replace_skipped(conn: &Connection, skipped: &[(String, u64, i64, &str)]) -> Result<()> {
    conn.execute_batch("DELETE FROM skipped_files;")?;
    let mut insert = conn.prepare_cached(
        "INSERT OR REPLACE INTO skipped_files(path, size_bytes, mtime_ns, reason) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (path, size, mtime_ns, reason) in skipped {
        insert.execute(params![path, *size as i64, mtime_ns, reason])?;
    }
    Ok(())
}

pub fn upsert_skipped(
    conn: &Connection,
    path: &str,
    size_bytes: u64,
    mtime_ns: i64,
    reason: &str,
) -> Result<()> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO skipped_files(path, size_bytes, mtime_ns, reason) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![path, size_bytes as i64, mtime_ns, reason])?;
    Ok(())
}

pub fn delete_skipped(conn: &Connection, path: &str) -> Result<()> {
    conn.prepare_cached("DELETE FROM skipped_files WHERE path = ?1")?
        .execute(params![path])?;
    Ok(())
}

pub fn indexed_stats(conn: &Connection) -> Result<HashMap<String, (u64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT path, size_bytes, mtime_ns FROM files
         UNION ALL
         SELECT path, size_bytes, mtime_ns FROM skipped_files WHERE reason != 'excluded'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            (row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)?),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

pub fn insert_file(
    conn: &Connection,
    path: &str,
    language: Language,
    sha256: &str,
    loc: usize,
    size_bytes: u64,
    mtime_ns: i64,
) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO files(path, language, sha256, loc, size_bytes, mtime_ns)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![
        path,
        language.to_string(),
        sha256,
        loc as i64,
        size_bytes as i64,
        mtime_ns
    ])?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_symbols(
    conn: &Connection,
    file_id: i64,
    symbols: &[IndexedSymbol],
) -> Result<Vec<i64>> {
    let mut insert = conn.prepare_cached(
        "INSERT INTO symbols(
            name, qualified_name, kind, file_id, start_line, end_line, signature, exported
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut ids = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        insert.execute(params![
            symbol.name,
            symbol.qualified_name,
            symbol.kind,
            file_id,
            symbol.start_line as i64,
            symbol.end_line as i64,
            symbol.signature,
            i64::from(symbol.exported)
        ])?;
        ids.push(conn.last_insert_rowid());
    }
    Ok(ids)
}

pub fn insert_imports(conn: &Connection, file_id: i64, imports: &[IndexedImport]) -> Result<usize> {
    let mut insert = conn.prepare_cached(
        "INSERT INTO imports(file_id, source, line, kind) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for imp in imports {
        insert.execute(params![file_id, imp.source, imp.line as i64, imp.kind])?;
    }
    Ok(imports.len())
}

pub fn insert_references(
    conn: &Connection,
    file_id: i64,
    refs: &[IndexedReference],
    symbol_ids: &HashMap<String, i64>,
) -> Result<usize> {
    let mut insert_ref = conn.prepare_cached(
        "INSERT INTO refs(symbol_name, from_symbol_id, file_id, line, column, context, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut insert_edge = conn.prepare_cached(
        "INSERT INTO edges(from_symbol_id, to_symbol_name, kind, weight) VALUES (?1, ?2, ?3, 1.0)",
    )?;
    for reference in refs {
        let from_symbol_id = reference
            .from_qualified_name
            .as_deref()
            .and_then(|name| symbol_ids.get(name).copied());
        insert_ref.execute(params![
            reference.symbol_name,
            from_symbol_id,
            file_id,
            reference.line as i64,
            reference.column as i64,
            truncate_chars(&reference.context, CONTEXT_MAX_CHARS),
            reference.kind
        ])?;
        if let Some(from_symbol_id) = from_symbol_id {
            insert_edge.execute(params![
                from_symbol_id,
                reference.symbol_name,
                reference.kind
            ])?;
        }
    }
    Ok(refs.len())
}

pub fn resolve_symbol(conn: &Connection, symbol: &str) -> Result<Option<SymbolRecord>> {
    conn.query_row(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE s.qualified_name = ?1 OR s.name = ?1 OR s.qualified_name LIKE ?2
        ORDER BY
            CASE
                WHEN s.qualified_name = ?1 THEN 0
                WHEN s.name = ?1 THEN 1
                ELSE 2
            END,
            length(s.qualified_name)
        LIMIT 1
        ",
        params![symbol, format!("%.{}", symbol)],
        map_symbol,
    )
    .optional()
    .map_err(Into::into)
}

pub fn symbol_fanout(conn: &Connection, symbol_id: i64) -> Result<usize> {
    conn.query_row(
        "SELECT COUNT(DISTINCT to_symbol_name) FROM edges WHERE from_symbol_id = ?1",
        params![symbol_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(Into::into)
}

pub fn symbol_callers_count(conn: &Connection, symbol_name: &str) -> Result<usize> {
    conn.query_row(
        "SELECT COUNT(DISTINCT from_symbol_id) FROM edges WHERE to_symbol_name = ?1",
        params![symbol_name],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(Into::into)
}

pub fn count_imports(conn: &Connection) -> Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM imports", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|c| c as usize)
    .map_err(Into::into)
}

pub fn count_files(conn: &Connection) -> Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
        .map(|c| c as usize)
        .map_err(Into::into)
}

pub fn count_symbols(conn: &Connection) -> Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|c| c as usize)
    .map_err(Into::into)
}

pub fn count_refs(conn: &Connection) -> Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM refs", [], |row| row.get::<_, i64>(0))
        .map(|c| c as usize)
        .map_err(Into::into)
}

pub fn count_edges(conn: &Connection) -> Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get::<_, i64>(0))
        .map(|c| c as usize)
        .map_err(Into::into)
}

pub fn map_symbol(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolRecord> {
    Ok(SymbolRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        qualified_name: row.get(2)?,
        kind: row.get(3)?,
        file_id: row.get(4)?,
        path: row.get(5)?,
        language: row.get(6)?,
        start_line: row.get::<_, i64>(7)? as usize,
        end_line: row.get::<_, i64>(8)? as usize,
        signature: row.get(9)?,
        exported: row.get::<_, i64>(10)? != 0,
    })
}

pub fn map_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReferenceRecord> {
    Ok(ReferenceRecord {
        id: row.get(0)?,
        symbol_name: row.get(1)?,
        from_symbol_id: row.get(2)?,
        from_symbol: row.get(3)?,
        path: row.get(4)?,
        line: row.get::<_, i64>(5)? as usize,
        column: row.get::<_, i64>(6)? as usize,
        context: row.get(7)?,
        kind: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    fn insert_files(conn: &Connection, count: usize) -> Vec<i64> {
        (0..count)
            .map(|i| {
                insert_file(
                    conn,
                    &format!("f{i}.ts"),
                    Language::TypeScript,
                    "sha",
                    1,
                    10,
                    1,
                )
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn delete_files_not_in_handles_forty_thousand_ids() {
        let temp = TempDir::new().unwrap();
        let conn = open_rw(&temp.path().join("t.db")).unwrap();
        let ids = insert_files(&conn, 40_001);
        let removed = delete_files_not_in(&conn, &ids[..40_000]).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(count_files(&conn).unwrap(), 40_000);
    }

    #[test]
    fn open_rw_rebuilds_an_old_schema() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("t.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT); \
                 INSERT INTO files(path) VALUES ('old.ts'); PRAGMA user_version = 3;",
            )
            .unwrap();
        }
        let conn = open_rw(&path).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(count_files(&conn).unwrap(), 0);
        conn.query_row("SELECT size_bytes, mtime_ns FROM files LIMIT 1", [], |_| {
            Ok(())
        })
        .optional()
        .unwrap();
    }

    #[test]
    fn open_ro_reports_missing_db() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("missing.db");
        assert!(matches!(open_ro(&path), Err(DbError::Missing(_))));
        assert!(!path.exists());
    }

    #[test]
    fn open_ro_reports_schema_mismatch_without_writing() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("t.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE x (id INTEGER); PRAGMA user_version = 3;")
            .unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(matches!(
            open_ro(&path),
            Err(DbError::SchemaMismatch { found: 3 })
        ));
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn stored_reference_context_is_capped() {
        let temp = TempDir::new().unwrap();
        let conn = open_rw(&temp.path().join("t.db")).unwrap();
        let file_id = insert_files(&conn, 1)[0];
        let reference = IndexedReference {
            symbol_name: "f".to_string(),
            from_qualified_name: None,
            line: 1,
            column: 1,
            context: "x".repeat(10_000),
            kind: "call".to_string(),
        };
        insert_references(&conn, file_id, &[reference], &HashMap::new()).unwrap();
        let context: String = conn
            .query_row("SELECT context FROM refs", [], |row| row.get(0))
            .unwrap();
        assert!(context.chars().count() <= 201);
    }
}
