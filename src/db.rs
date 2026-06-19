use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::types::{
    IndexedImport, IndexedReference, IndexedSymbol, Language, ReferenceRecord, SymbolRecord,
};

pub const SCHEMA_VERSION: i64 = 3;

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_existing(path: &Path) -> Result<Connection> {
    if !path.exists() {
        bail!(
            "Tessera database not found at {}. Run `tessera index . --db {}` first.",
            path.display(),
            path.display()
        );
    }
    open(path)
}

pub fn reset(conn: &Connection) -> Result<()> {
    // The FTS5 virtual table is configured with external content (content='symbols')
    // and is kept in sync via triggers. Deleting `symbols` here causes the
    // AFTER DELETE trigger to fire for every row, which is correct but slow on
    // large tables; for v0.2's repo sizes the cost is irrelevant.
    conn.execute_batch(
        "
        DELETE FROM imports;
        DELETE FROM edges;
        DELETE FROM refs;
        DELETE FROM symbols;
        DELETE FROM files;
        ",
    )?;
    // Ask FTS5 to drop any orphaned tombstones.
    let _ = conn.execute_batch("INSERT INTO symbols_fts(symbols_fts) VALUES('delete-all');");
    Ok(())
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            language TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            loc INTEGER NOT NULL,
            indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS meta_blob (
            key TEXT PRIMARY KEY,
            value BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS symbols (
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

        CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_symbols_qualified ON symbols(qualified_name);
        CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);

        CREATE TABLE IF NOT EXISTS refs (
            id INTEGER PRIMARY KEY,
            symbol_name TEXT NOT NULL,
            from_symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            line INTEGER NOT NULL,
            column INTEGER NOT NULL,
            context TEXT NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_refs_symbol ON refs(symbol_name);
        CREATE INDEX IF NOT EXISTS idx_refs_from ON refs(from_symbol_id);
        CREATE INDEX IF NOT EXISTS idx_refs_file ON refs(file_id);

        CREATE TABLE IF NOT EXISTS edges (
            id INTEGER PRIMARY KEY,
            from_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
            to_symbol_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            weight REAL NOT NULL DEFAULT 1.0
        );

        CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_symbol_name);
        CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_symbol_id);

        CREATE TABLE IF NOT EXISTS imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            source TEXT NOT NULL,
            line INTEGER NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file_id);
        CREATE INDEX IF NOT EXISTS idx_imports_source ON imports(source);
        ",
    )?;

    // FTS5 trigram virtual table. rusqlite's bundled SQLite ships FTS5 and the
    // trigram tokenizer. The triggers below keep it in sync with `symbols`.
    let fts_setup = "
        CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
            name, qualified_name,
            content='symbols', content_rowid='id',
            tokenize='trigram'
        );

        CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
            INSERT INTO symbols_fts(rowid, name, qualified_name)
            VALUES (new.id, new.name, new.qualified_name);
        END;

        CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
            INSERT INTO symbols_fts(symbols_fts, rowid, name, qualified_name)
            VALUES('delete', old.id, old.name, old.qualified_name);
        END;
        ";
    // FTS5 may not be available in unusual builds; if it fails, fall back silently.
    let _ = conn.execute_batch(fts_setup);

    set_meta(conn, "schema_version", &SCHEMA_VERSION.to_string())?;
    Ok(())
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "
        INSERT INTO meta(key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn set_meta_blob(conn: &Connection, key: &str, value: &[u8]) -> Result<()> {
    conn.execute(
        "
        INSERT INTO meta_blob(key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_meta_blob(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    conn.query_row(
        "SELECT value FROM meta_blob WHERE key = ?1",
        params![key],
        |row| row.get::<_, Vec<u8>>(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn file_sha(conn: &Connection, path: &str) -> Result<Option<(i64, String)>> {
    conn.query_row(
        "SELECT id, sha256 FROM files WHERE path = ?1",
        params![path],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

pub fn delete_file_cascade(conn: &Connection, file_id: i64) -> Result<()> {
    conn.execute("DELETE FROM files WHERE id = ?1", params![file_id])?;
    Ok(())
}

pub fn delete_files_not_in(conn: &Connection, retained_ids: &[i64]) -> Result<usize> {
    if retained_ids.is_empty() {
        let removed = conn.execute("DELETE FROM files", [])?;
        return Ok(removed);
    }
    let placeholders = retained_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM files WHERE id NOT IN ({placeholders})");
    let params_iter: Vec<&dyn rusqlite::ToSql> = retained_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    let removed = conn.execute(&sql, params_iter.as_slice())?;
    Ok(removed)
}

pub fn insert_file(
    conn: &Connection,
    path: &str,
    language: Language,
    sha256: &str,
    loc: usize,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO files(path, language, sha256, loc) VALUES (?1, ?2, ?3, ?4)",
        params![path, language.to_string(), sha256, loc as i64],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_symbols(
    conn: &Connection,
    file_id: i64,
    symbols: &[IndexedSymbol],
) -> Result<Vec<i64>> {
    let mut ids = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        conn.execute(
            "
            INSERT INTO symbols(
                name, qualified_name, kind, file_id, start_line, end_line, signature, exported
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                symbol.name,
                symbol.qualified_name,
                symbol.kind,
                file_id,
                symbol.start_line as i64,
                symbol.end_line as i64,
                symbol.signature,
                i64::from(symbol.exported)
            ],
        )?;
        ids.push(conn.last_insert_rowid());
    }
    Ok(ids)
}

pub fn find_symbol_id(
    conn: &Connection,
    file_id: i64,
    qualified_name: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM symbols WHERE file_id = ?1 AND qualified_name = ?2",
        params![file_id, qualified_name],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_imports(conn: &Connection, file_id: i64, imports: &[IndexedImport]) -> Result<usize> {
    for imp in imports {
        conn.execute(
            "INSERT INTO imports(file_id, source, line, kind) VALUES (?1, ?2, ?3, ?4)",
            params![file_id, imp.source, imp.line as i64, imp.kind],
        )?;
    }
    Ok(imports.len())
}

pub fn insert_references(
    conn: &Connection,
    file_id: i64,
    refs: &[IndexedReference],
) -> Result<usize> {
    let mut count = 0;
    for reference in refs {
        let from_symbol_id = reference
            .from_qualified_name
            .as_deref()
            .map(|name| find_symbol_id(conn, file_id, name))
            .transpose()?
            .flatten();
        conn.execute(
            "
            INSERT INTO refs(
                symbol_name, from_symbol_id, file_id, line, column, context, kind
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                reference.symbol_name,
                from_symbol_id,
                file_id,
                reference.line as i64,
                reference.column as i64,
                reference.context,
                reference.kind
            ],
        )?;
        if let Some(from_symbol_id) = from_symbol_id {
            conn.execute(
                "
                INSERT INTO edges(from_symbol_id, to_symbol_name, kind, weight)
                VALUES (?1, ?2, ?3, 1.0)
                ",
                params![from_symbol_id, reference.symbol_name, reference.kind],
            )?;
        }
        count += 1;
    }
    Ok(count)
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
