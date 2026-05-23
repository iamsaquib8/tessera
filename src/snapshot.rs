use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use memmap2::Mmap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::types::SymbolRecord;

/// Memory-mapped, derived view of the graph. Snapshot is a derived artifact —
/// SQLite is always the source of truth. Building the snapshot lets the MCP
/// server skip SQLite round-trips on the hot path: every `find_definition`,
/// `find_references`, and `impact` query becomes pure-memory work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub version: u32,
    pub symbols: Vec<SymbolEntry>,
    pub edges: Vec<(u32, u32)>,
    pub name_index: HashMap<String, Vec<u32>>,
    pub qualified_index: HashMap<String, Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub id: i64,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file_id: i64,
    pub path: String,
    pub language: String,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: String,
    pub exported: bool,
}

impl SymbolEntry {
    pub fn to_record(&self) -> SymbolRecord {
        SymbolRecord {
            id: self.id,
            name: self.name.clone(),
            qualified_name: self.qualified_name.clone(),
            kind: self.kind.clone(),
            file_id: self.file_id,
            path: self.path.clone(),
            language: self.language.clone(),
            start_line: self.start_line as usize,
            end_line: self.end_line as usize,
            signature: self.signature.clone(),
            exported: self.exported,
        }
    }
}

pub const SNAPSHOT_VERSION: u32 = 1;

pub fn build(conn: &Connection, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut id_to_index: HashMap<i64, u32> = HashMap::new();
    let mut symbols: Vec<SymbolEntry> = Vec::new();
    let mut name_index: HashMap<String, Vec<u32>> = HashMap::new();
    let mut qualified_index: HashMap<String, Vec<u32>> = HashMap::new();

    {
        let mut stmt = conn.prepare(
            "
            SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                   s.start_line, s.end_line, s.signature, s.exported
            FROM symbols s
            JOIN files f ON f.id = s.file_id
            ORDER BY s.id
            ",
        )?;
        let rows = stmt.query_map([], db::map_symbol)?;
        for row in rows {
            let record = row?;
            let idx = symbols.len() as u32;
            id_to_index.insert(record.id, idx);
            name_index.entry(record.name.clone()).or_default().push(idx);
            qualified_index
                .entry(record.qualified_name.clone())
                .or_default()
                .push(idx);
            symbols.push(SymbolEntry {
                id: record.id,
                name: record.name,
                qualified_name: record.qualified_name,
                kind: record.kind,
                file_id: record.file_id,
                path: record.path,
                language: record.language,
                start_line: record.start_line as u32,
                end_line: record.end_line as u32,
                signature: record.signature,
                exported: record.exported,
            });
        }
    }

    let mut edges: Vec<(u32, u32)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "
            SELECT e.from_symbol_id, e.to_symbol_name
            FROM edges e
            ",
        )?;
        let rows = stmt.query_map([], |row| {
            let from: i64 = row.get(0)?;
            let to_name: String = row.get(1)?;
            Ok((from, to_name))
        })?;
        for row in rows {
            let (from, to_name) = row?;
            let Some(&from_idx) = id_to_index.get(&from) else {
                continue;
            };
            // Resolve to-name to one or more target indices.
            let candidates = qualified_index
                .get(&to_name)
                .or_else(|| name_index.get(&to_name));
            if let Some(targets) = candidates {
                for &to_idx in targets {
                    edges.push((from_idx, to_idx));
                }
            }
        }
    }

    let snapshot = GraphSnapshot {
        version: SNAPSHOT_VERSION,
        symbols,
        edges,
        name_index,
        qualified_index,
    };
    let encoded = bincode::serialize(&snapshot)?;
    let tmp_path = path.with_extension("bin.tmp");
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub struct MappedSnapshot {
    _mmap: Mmap,
    snapshot: GraphSnapshot,
}

impl MappedSnapshot {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let snapshot: GraphSnapshot = bincode::deserialize(&mmap[..])?;
        Ok(Self {
            _mmap: mmap,
            snapshot,
        })
    }

    pub fn graph(&self) -> &GraphSnapshot {
        &self.snapshot
    }
}

pub fn try_open(path: &Path) -> Option<MappedSnapshot> {
    MappedSnapshot::open(path).ok()
}
