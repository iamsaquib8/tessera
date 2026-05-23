//! Graph-engine abstraction. The default backend is SQLite (the one used
//! everywhere in the codebase). A feature-gated `cozo` backend mirrors the
//! call edges into an embedded Datalog database so impact queries can be
//! expressed as a recursive Datalog rule.
//!
//! The trait is intentionally small: it only covers the call-graph traversal
//! that benefits from a different algebra. Symbol lookup, references, and
//! outlines all stay on SQLite — they're already fast there.

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::types::GraphEngineKind;

pub trait GraphEngine {
    /// Return caller symbol IDs that transitively reach `target` within
    /// `depth` hops (depth 1 = direct callers).
    fn transitive_callers(&self, target: &str, depth: usize) -> Result<Vec<(i64, usize)>>;

    fn kind(&self) -> GraphEngineKind;
}

pub struct SqliteEngine<'a> {
    pub conn: &'a Connection,
}

impl<'a> SqliteEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

impl<'a> GraphEngine for SqliteEngine<'a> {
    fn transitive_callers(&self, target: &str, depth: usize) -> Result<Vec<(i64, usize)>> {
        use std::collections::{HashSet, VecDeque};
        let mut out: Vec<(i64, usize)> = Vec::new();
        let mut seen: HashSet<i64> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        queue.push_back((target.to_string(), 1));

        let mut stmt = self.conn.prepare(
            "
            SELECT DISTINCT s.id, s.name, s.qualified_name
            FROM edges e
            JOIN symbols s ON s.id = e.from_symbol_id
            WHERE e.to_symbol_name = ?1 OR e.to_symbol_name LIKE ?2
            LIMIT 500
            ",
        )?;

        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth > depth {
                continue;
            }
            let like = format!("%.{}", current);
            let rows = stmt
                .query_map(rusqlite::params![current, like], |row| {
                    let id: i64 = row.get(0)?;
                    let name: String = row.get(1)?;
                    let qname: String = row.get(2)?;
                    Ok((id, name, qname))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (id, name, qname) in rows {
                if !seen.insert(id) {
                    continue;
                }
                out.push((id, current_depth));
                if current_depth < depth {
                    queue.push_back((name, current_depth + 1));
                    if qname != current {
                        queue.push_back((qname, current_depth + 1));
                    }
                }
            }
        }
        Ok(out)
    }

    fn kind(&self) -> GraphEngineKind {
        GraphEngineKind::Sqlite
    }
}

/// Open the requested engine. The `cozo` variant returns an error unless the
/// `cozo` Cargo feature is enabled. We deliberately do **not** silently fall
/// back to SQLite — if the user asked for Cozo, they should see the gap.
pub fn select_engine<'a>(
    kind: GraphEngineKind,
    conn: &'a Connection,
    _db_path: &Path,
) -> Result<Box<dyn GraphEngine + 'a>> {
    match kind {
        GraphEngineKind::Sqlite => Ok(Box::new(SqliteEngine::new(conn))),
        GraphEngineKind::Cozo => cozo_engine(conn, _db_path),
    }
}

fn cozo_engine<'a>(_conn: &'a Connection, _db_path: &Path) -> Result<Box<dyn GraphEngine + 'a>> {
    Err(anyhow::anyhow!(
        "Cozo backend is a v0.3 deliverable — the GraphEngine trait is in place \
         and the call-edge relation is documented, but the embedded Cozo dependency \
         is deliberately not wired up in v0.2. Use the default SQLite backend."
    ))
}
