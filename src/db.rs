use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::extract::truncate_chars;
use crate::types::{
    IndexedBinding, IndexedImport, IndexedReference, IndexedSymbol, IndexedTypeHint, Language,
    ReferenceRecord, SymbolRecord,
};

pub const SCHEMA_VERSION: i64 = 8;
/// `PRAGMA application_id` of every Tessera index ("TSSR").
pub const APPLICATION_ID: i64 = 0x5453_5352;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);
/// Negative `cache_size` is in KiB: 128 MiB, allocated only as pages are used.
const WRITER_CACHE_KIB: i64 = -131_072;
const CONTEXT_MAX_CHARS: usize = 200;

#[derive(Debug)]
pub enum DbError {
    Missing(PathBuf),
    SchemaMismatch { found: i64 },
    NotTessera(PathBuf),
    Sqlite(rusqlite::Error),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(
                f,
                "Tessera index not found at {path}. Any query builds it, or run `tessera index`.",
                path = path.display()
            ),
            Self::NotTessera(path) => write!(
                f,
                "{} is not a Tessera index; refusing to overwrite it. Pass a different --db path.",
                path.display()
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
    if !is_tessera_or_empty(&conn)? {
        return Err(DbError::NotTessera(path.to_path_buf()).into());
    }
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    // A full index of a large repository keeps its index b-trees hot; with
    // SQLite's 2 MB default, pages spill to the WAL and are read back.
    conn.pragma_update(None, "cache_size", WRITER_CACHE_KIB)?;
    let version = user_version(&conn)?;
    if version != SCHEMA_VERSION {
        drop_all(&conn)?;
        create_schema(&conn)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        conn.pragma_update(None, "application_id", APPLICATION_ID)?;
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

// Indexes written before application_id existed are recognised by their tables.
fn is_tessera_or_empty(conn: &Connection) -> Result<bool> {
    let id: i64 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if id == APPLICATION_ID {
        return Ok(true);
    }
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(id == 0
        && (tables.is_empty()
            || ["meta", "files", "symbols"]
                .iter()
                .all(|t| tables.iter().any(|n| n == t))))
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
            parse_errors INTEGER NOT NULL DEFAULT 0,
            module TEXT,
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
            exported INTEGER NOT NULL DEFAULT 0,
            container TEXT,
            arity_min INTEGER,
            arity_max INTEGER,
            default_export INTEGER NOT NULL DEFAULT 0
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
            kind TEXT NOT NULL,
            receiver TEXT NOT NULL DEFAULT 'none',
            receiver_name TEXT,
            arg_count INTEGER
        );

        CREATE INDEX idx_refs_symbol ON refs(symbol_name);
        CREATE INDEX idx_refs_from ON refs(from_symbol_id);
        CREATE INDEX idx_refs_file ON refs(file_id);

        CREATE TABLE edges (
            id INTEGER PRIMARY KEY,
            from_symbol_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
            to_symbol_name TEXT NOT NULL,
            to_symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
            kind TEXT NOT NULL,
            confidence TEXT NOT NULL DEFAULT 'name',
            resolver TEXT,
            ambiguous_group INTEGER,
            ref_id INTEGER REFERENCES refs(id) ON DELETE CASCADE,
            weight REAL NOT NULL DEFAULT 1.0,
            -- resolver 'missing': the module or class file that lacks the name
            missing_in TEXT,
            -- the call passes an argument count no candidate accepts
            arity_mismatch INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX idx_edges_to ON edges(to_symbol_name);
        CREATE INDEX idx_edges_to_id ON edges(to_symbol_id);
        CREATE INDEX idx_edges_from ON edges(from_symbol_id);
        CREATE INDEX idx_edges_ref ON edges(ref_id);

        CREATE TABLE imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            source TEXT NOT NULL,
            line INTEGER NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE TABLE import_bindings (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            local TEXT NOT NULL,
            imported TEXT,
            source TEXT NOT NULL,
            kind TEXT NOT NULL,
            line INTEGER NOT NULL,
            target_path TEXT,
            -- ok, missing_module (a local specifier resolves to no file),
            -- or missing_export (the module does not export the name)
            status TEXT NOT NULL DEFAULT 'ok'
        );

        CREATE INDEX idx_bindings_file ON import_bindings(file_id);
        CREATE INDEX idx_bindings_source ON import_bindings(source);
        CREATE INDEX idx_bindings_target ON import_bindings(target_path);

        CREATE TABLE type_hints (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            scope TEXT,
            name TEXT NOT NULL,
            type_name TEXT NOT NULL,
            origin TEXT NOT NULL
        );

        CREATE INDEX idx_hints_file ON type_hints(file_id);

        -- Contract facts (routes, gRPC methods, topics, tables). Facts with
        -- the same key link at query time.
        CREATE TABLE facts (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            key TEXT NOT NULL,
            name TEXT NOT NULL,
            line INTEGER NOT NULL,
            symbol_id INTEGER,
            confidence TEXT NOT NULL,
            rule TEXT NOT NULL,
            evidence TEXT NOT NULL
        );

        CREATE INDEX idx_facts_file ON facts(file_id);
        CREATE INDEX idx_facts_key ON facts(key);
        CREATE INDEX idx_facts_kind ON facts(kind);

        CREATE INDEX idx_imports_file ON imports(file_id);
        CREATE INDEX idx_imports_source ON imports(source);

        CREATE VIRTUAL TABLE symbols_fts USING fts5(
            name, qualified_name,
            content='symbols', content_rowid='id',
            tokenize='trigram'
        );
        ",
    )?;
    Ok(())
}

pub fn reset(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DELETE FROM imports;
        DELETE FROM import_bindings;
        DELETE FROM type_hints;
        DELETE FROM facts;
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

pub fn file_stat(conn: &Connection, path: &str) -> Result<Option<(i64, u64, i64)>> {
    conn.prepare_cached("SELECT id, size_bytes, mtime_ns FROM files WHERE path = ?1")?
        .query_row(params![path], |row| {
            Ok((row.get(0)?, row.get::<_, i64>(1)? as u64, row.get(2)?))
        })
        .optional()
        .map_err(Into::into)
}

pub fn touch_file(conn: &Connection, file_id: i64, size_bytes: u64, mtime_ns: i64) -> Result<()> {
    conn.prepare_cached("UPDATE files SET size_bytes = ?2, mtime_ns = ?3 WHERE id = ?1")?
        .execute(params![file_id, size_bytes as i64, mtime_ns])?;
    Ok(())
}

// The symbol search index is kept per file, not per row: a trigger on every
// symbol insert makes FTS5 flush its pending terms on each statement.

/// Adds `file_id`'s symbols to the search index.
pub fn fts_insert_file(conn: &Connection, file_id: i64) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO symbols_fts(rowid, name, qualified_name)
         SELECT id, name, qualified_name FROM symbols WHERE file_id = ?1",
    )?
    .execute(params![file_id])?;
    Ok(())
}

fn fts_delete_file(conn: &Connection, file_id: i64) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO symbols_fts(symbols_fts, rowid, name, qualified_name)
         SELECT 'delete', id, name, qualified_name FROM symbols WHERE file_id = ?1",
    )?
    .execute(params![file_id])?;
    Ok(())
}

/// Rebuilds the search index from the symbols table, after a bulk load.
pub fn fts_rebuild(conn: &Connection) -> Result<()> {
    conn.execute_batch("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild');")?;
    Ok(())
}

pub fn delete_file_cascade(conn: &Connection, file_id: i64) -> Result<()> {
    fts_delete_file(conn, file_id)?;
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
    conn.execute(
        "INSERT INTO symbols_fts(symbols_fts, rowid, name, qualified_name)
         SELECT 'delete', id, name, qualified_name FROM symbols
         WHERE file_id NOT IN (SELECT id FROM retained_files)",
        [],
    )?;
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

pub fn mark_parse_errors(conn: &Connection, file_id: i64) -> Result<()> {
    conn.prepare_cached("UPDATE files SET parse_errors = 1 WHERE id = ?1")?
        .execute(params![file_id])?;
    Ok(())
}

pub fn parse_error_paths(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM files WHERE parse_errors = 1 ORDER BY path")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn unreadable_paths(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM skipped_files WHERE reason = 'unreadable'")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
            name, qualified_name, kind, file_id, start_line, end_line, signature, exported,
            container, arity_min, arity_max, default_export
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
            i64::from(symbol.exported),
            symbol.container,
            symbol.arity.map(|(min, _)| min),
            symbol.arity.and_then(|(_, max)| max),
            i64::from(symbol.default_export)
        ])?;
        ids.push(conn.last_insert_rowid());
    }
    Ok(ids)
}

pub fn insert_bindings(conn: &Connection, file_id: i64, bindings: &[IndexedBinding]) -> Result<()> {
    let mut insert = conn.prepare_cached(
        "INSERT INTO import_bindings(file_id, local, imported, source, kind, line)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for b in bindings {
        insert.execute(params![
            file_id,
            b.local,
            b.imported,
            b.source,
            b.kind.as_str(),
            b.line as i64
        ])?;
    }
    Ok(())
}

/// Facts of one file, each tied to its handler: the symbol named by the
/// fact, or the innermost symbol around its line.
pub fn insert_facts(
    conn: &Connection,
    file_id: i64,
    facts: &[crate::facts::IndexedFact],
    symbols: &[crate::types::IndexedSymbol],
    symbol_ids: &[i64],
) -> Result<()> {
    if facts.is_empty() {
        return Ok(());
    }
    let mut insert = conn.prepare_cached(
        "INSERT INTO facts(file_id, kind, key, name, line, symbol_id, confidence, rule, evidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for fact in facts {
        let named = fact.handler.as_ref().and_then(|handler| {
            symbols
                .iter()
                .position(|s| &s.name == handler && s.container.is_none())
                .or_else(|| symbols.iter().position(|s| &s.name == handler))
        });
        let around = || {
            symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| s.start_line <= fact.line && fact.line <= s.end_line)
                .min_by_key(|(_, s)| s.end_line - s.start_line)
                .map(|(index, _)| index)
        };
        // A decorator above a method (`@Get(":id")`) belongs to the method
        // that starts just below it, not to the class around both.
        let decorated = |outer: usize| {
            symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| s.start_line > fact.line && s.start_line <= fact.line + 4)
                .filter(|(_, s)| {
                    s.container.as_deref() == Some(symbols[outer].qualified_name.as_str())
                })
                .min_by_key(|(_, s)| s.start_line)
                .map(|(index, _)| index)
        };
        let symbol_id = named
            .or_else(|| {
                let outer = around()?;
                let in_class = matches!(
                    symbols[outer].kind.as_str(),
                    "class" | "interface" | "module" | "struct"
                );
                if fact.kind == crate::facts::kind::ROUTE && in_class {
                    decorated(outer).or(Some(outer))
                } else {
                    Some(outer)
                }
            })
            .map(|index| symbol_ids[index]);
        insert.execute(params![
            file_id,
            fact.kind,
            fact.key,
            fact.name,
            fact.line as i64,
            symbol_id,
            fact.confidence,
            fact.rule,
            fact.evidence
        ])?;
    }
    Ok(())
}

pub fn insert_hints(conn: &Connection, file_id: i64, hints: &[IndexedTypeHint]) -> Result<()> {
    let mut insert = conn.prepare_cached(
        "INSERT INTO type_hints(file_id, scope, name, type_name, origin) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for h in hints {
        insert.execute(params![
            file_id,
            h.scope,
            h.name,
            h.type_name,
            h.origin.as_str()
        ])?;
    }
    Ok(())
}

pub fn set_file_module(conn: &Connection, file_id: i64, module: Option<&str>) -> Result<()> {
    conn.prepare_cached("UPDATE files SET module = ?2 WHERE id = ?1")?
        .execute(params![file_id, module])?;
    Ok(())
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
    ordered_ids: &[i64],
    symbol_ids: &HashMap<String, i64>,
) -> Result<usize> {
    let mut insert_ref = conn.prepare_cached(
        "INSERT INTO refs(
            symbol_name, from_symbol_id, file_id, line, column, context, kind,
            receiver, receiver_name, arg_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for reference in refs {
        let from_symbol_id = reference
            .from_symbol
            .and_then(|index| ordered_ids.get(index).copied())
            .or_else(|| {
                reference
                    .from_qualified_name
                    .as_deref()
                    .and_then(|name| symbol_ids.get(name).copied())
            });
        insert_ref.execute(params![
            reference.symbol_name,
            from_symbol_id,
            file_id,
            reference.line as i64,
            reference.column as i64,
            truncate_chars(&reference.context, CONTEXT_MAX_CHARS),
            reference.kind,
            reference.receiver.as_str(),
            reference.receiver_name,
            reference.arg_count
        ])?;
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
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT); \
                 CREATE TABLE symbols (id INTEGER PRIMARY KEY, name TEXT); \
                 CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT); \
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
            from_symbol: None,
            line: 1,
            column: 1,
            context: "x".repeat(10_000),
            kind: "call".to_string(),
            receiver: crate::types::Receiver::None,
            receiver_name: None,
            arg_count: None,
        };
        insert_references(&conn, file_id, &[reference], &[], &HashMap::new()).unwrap();
        let context: String = conn
            .query_row("SELECT context FROM refs", [], |row| row.get(0))
            .unwrap();
        assert!(context.chars().count() <= 201);
    }
}
