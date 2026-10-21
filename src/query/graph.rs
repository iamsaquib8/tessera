//! Queries over resolved edges: who references a symbol, what it calls, and
//! everything a change to it can reach.

use std::collections::{HashMap, VecDeque};

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::db;
use crate::output::Confidence;
use crate::types::{ReferenceRecord, SymbolRecord};

/// Stops a blast walk on hub symbols; the result says it was cut.
pub const MAX_FRONTIER: usize = 800;

const SYMBOL_SQL: &str = "SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path,
    f.language, s.start_line, s.end_line, s.signature, s.exported
    FROM symbols s JOIN files f ON f.id = s.file_id";

pub fn confidence(value: &str) -> Confidence {
    match value {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        "low" => Confidence::Low,
        _ => Confidence::Name,
    }
}

/// Symbols `target` names: exact qualified-name matches, else every symbol
/// with that simple name.
pub fn definitions(conn: &Connection, target: &str) -> Result<Vec<SymbolRecord>> {
    let exact = symbols(
        conn,
        &format!("{SYMBOL_SQL} WHERE s.qualified_name = ?1"),
        target,
    )?;
    if !exact.is_empty() {
        return Ok(exact);
    }
    let name = super::common::bare_name(target);
    symbols(conn, &format!("{SYMBOL_SQL} WHERE s.name = ?1"), name)
}

fn symbols(conn: &Connection, sql: &str, arg: &str) -> Result<Vec<SymbolRecord>> {
    let mut stmt = conn.prepare_cached(&format!("{sql} ORDER BY f.path, s.start_line, s.id"))?;
    let rows = stmt.query_map(params![arg], db::map_symbol)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn symbol(conn: &Connection, id: i64) -> Result<Option<SymbolRecord>> {
    Ok(symbols(
        conn,
        &format!("{SYMBOL_SQL} WHERE s.id = ?1"),
        &id.to_string(),
    )?
    .into_iter()
    .next())
}

/// References that resolve to one of `ids`, plus references to `name` that
/// nothing resolved (they may mean it). External references are left out.
pub fn incoming_refs(
    conn: &Connection,
    ids: &[i64],
    name: &str,
) -> Result<Vec<(ReferenceRecord, Confidence)>> {
    let mut best: HashMap<i64, (ReferenceRecord, Confidence)> = HashMap::new();
    let mut keep = |reference: ReferenceRecord, confidence: Confidence| {
        let entry = best
            .entry(reference.id)
            .or_insert_with(|| (reference, confidence));
        if confidence < entry.1 {
            entry.1 = confidence;
        }
    };
    let columns = "r.id, r.symbol_name, r.from_symbol_id, s.qualified_name, f.path,
        r.line, r.column, r.context, r.kind";
    let mut resolved = conn.prepare_cached(&format!(
        "SELECT {columns}, e.confidence FROM edges e
         JOIN refs r ON r.id = e.ref_id
         JOIN files f ON f.id = r.file_id
         LEFT JOIN symbols s ON s.id = r.from_symbol_id
         WHERE e.to_symbol_id = ?1"
    ))?;
    for id in ids {
        let rows = resolved.query_map(params![id], |row| {
            Ok((db::map_reference(row)?, row.get::<_, String>(9)?))
        })?;
        for row in rows {
            let (reference, value) = row?;
            keep(reference, confidence(&value));
        }
    }
    let mut unresolved = conn.prepare_cached(&format!(
        "SELECT {columns} FROM refs r
         JOIN files f ON f.id = r.file_id
         LEFT JOIN symbols s ON s.id = r.from_symbol_id
         WHERE r.symbol_name = ?1 AND (
             r.from_symbol_id IS NULL
             OR EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id
                        AND e.to_symbol_id IS NULL AND e.resolver IS NULL))"
    ))?;
    for row in unresolved.query_map(params![name], db::map_reference)? {
        keep(row?, Confidence::Name);
    }
    let mut refs: Vec<(ReferenceRecord, Confidence)> = best.into_values().collect();
    refs.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.0.path.cmp(&b.0.path))
            .then(a.0.line.cmp(&b.0.line))
            .then(a.0.column.cmp(&b.0.column))
    });
    Ok(refs)
}

/// Symbols whose resolved edges point at `id`, with the best confidence.
pub fn callers(conn: &Connection, id: i64) -> Result<Vec<(SymbolRecord, Confidence)>> {
    neighbours(
        conn,
        "SELECT from_symbol_id, confidence FROM edges WHERE to_symbol_id = ?1",
        id,
    )
}

/// Symbols `id` resolves to, with the best confidence.
pub fn callees(conn: &Connection, id: i64) -> Result<Vec<(SymbolRecord, Confidence)>> {
    neighbours(
        conn,
        "SELECT to_symbol_id, confidence FROM edges
         WHERE from_symbol_id = ?1 AND to_symbol_id IS NOT NULL",
        id,
    )
}

fn neighbours(conn: &Connection, sql: &str, id: i64) -> Result<Vec<(SymbolRecord, Confidence)>> {
    let mut best: HashMap<i64, Confidence> = HashMap::new();
    let rows: Vec<(i64, String)> = conn
        .prepare_cached(sql)?
        .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (other, value) in rows {
        let value = confidence(&value);
        let entry = best.entry(other).or_insert(value);
        if value < *entry {
            *entry = value;
        }
    }
    let mut out = Vec::new();
    for (other, value) in best {
        if other == id {
            continue;
        }
        if let Some(record) = symbol(conn, other)? {
            out.push((record, value));
        }
    }
    out.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.0.path.cmp(&b.0.path))
            .then(a.0.start_line.cmp(&b.0.start_line))
    });
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Impacted {
    pub symbol: SymbolRecord,
    pub depth: usize,
    /// The weakest edge on the shortest path from a seed.
    pub confidence: Confidence,
    /// Personalised PageRank, scaled so the top caller is 100.
    pub criticality: f32,
    /// The seed this caller was first reached from.
    pub via: i64,
}

/// Everything that reaches `seeds` through resolved edges, up to `depth`
/// hops, ranked by personalised PageRank. The flag is true when the walk
/// stopped at [`MAX_FRONTIER`].
pub fn blast(conn: &Connection, seeds: &[i64], depth: usize) -> Result<(Vec<Impacted>, bool)> {
    let mut reached: HashMap<i64, (usize, Confidence)> = HashMap::new();
    let mut via: HashMap<i64, i64> = HashMap::new();
    let mut queue: VecDeque<(i64, usize, Confidence, i64)> = seeds
        .iter()
        .map(|id| (*id, 0, Confidence::High, *id))
        .collect();
    let mut walk: Vec<(i64, i64, f64)> = Vec::new();
    let mut capped = false;
    let mut incoming = conn.prepare_cached(
        "SELECT from_symbol_id, confidence, weight FROM edges WHERE to_symbol_id = ?1",
    )?;
    'walk: while let Some((node, node_depth, node_confidence, seed)) = queue.pop_front() {
        if node_depth >= depth {
            continue;
        }
        let rows: Vec<(i64, String, f64)> = incoming
            .query_map(params![node], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (caller, value, weight) in rows {
            if caller == node {
                continue;
            }
            walk.push((node, caller, weight));
            if seeds.contains(&caller) {
                continue;
            }
            let path_confidence = node_confidence.max(confidence(&value));
            let caller_depth = node_depth + 1;
            match reached.get_mut(&caller) {
                Some(entry) => {
                    if caller_depth == entry.0 && path_confidence < entry.1 {
                        entry.1 = path_confidence;
                    }
                }
                None => {
                    if reached.len() >= MAX_FRONTIER {
                        capped = true;
                        break 'walk;
                    }
                    reached.insert(caller, (caller_depth, path_confidence));
                    via.insert(caller, seed);
                    queue.push_back((caller, caller_depth, path_confidence, seed));
                }
            }
        }
    }
    let rank = pagerank(seeds, &reached, &walk);
    let top = rank.values().copied().fold(0.0f64, f64::max);
    let top = if top > 0.0 { top } else { 1.0 };
    let mut impacted = Vec::new();
    for (id, (caller_depth, caller_confidence)) in reached {
        let Some(record) = symbol(conn, id)? else {
            continue;
        };
        let score = rank.get(&id).copied().unwrap_or(0.0);
        impacted.push(Impacted {
            symbol: record,
            depth: caller_depth,
            confidence: caller_confidence,
            criticality: ((score / top) * 100.0).clamp(0.0, 100.0) as f32,
            via: via.get(&id).copied().unwrap_or_default(),
        });
    }
    impacted.sort_by(|a, b| {
        b.criticality
            .partial_cmp(&a.criticality)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.depth.cmp(&b.depth))
            .then(a.confidence.cmp(&b.confidence))
            .then_with(|| a.symbol.path.cmp(&b.symbol.path))
            .then(a.symbol.start_line.cmp(&b.symbol.start_line))
            .then(a.symbol.id.cmp(&b.symbol.id))
    });
    Ok((impacted, capped))
}

/// Personalised PageRank over the reversed call graph (callee → caller),
/// teleporting to the seeds. Iteration order is fixed so scores are
/// identical run to run.
fn pagerank(
    seeds: &[i64],
    reached: &HashMap<i64, (usize, Confidence)>,
    walk: &[(i64, i64, f64)],
) -> HashMap<i64, f64> {
    let mut nodes: Vec<i64> = seeds.iter().chain(reached.keys()).copied().collect();
    nodes.sort_unstable();
    nodes.dedup();
    let index: HashMap<i64, usize> = nodes.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let n = nodes.len();
    let mut out: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for (callee, caller, weight) in walk {
        if let (Some(&from), Some(&to)) = (index.get(callee), index.get(caller)) {
            out[from].push((to, *weight));
        }
    }
    for edges in &mut out {
        edges.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let mut teleport = vec![0.0f64; n];
    for seed in seeds {
        if let Some(&i) = index.get(seed) {
            teleport[i] = 1.0 / seeds.len() as f64;
        }
    }
    let damping = 0.85;
    let mut rank = teleport.clone();
    for _ in 0..25 {
        let mut next: Vec<f64> = teleport.iter().map(|t| (1.0 - damping) * t).collect();
        let mut dangling = 0.0;
        for (from, edges) in out.iter().enumerate() {
            let total: f64 = edges.iter().map(|(_, w)| w).sum();
            if total <= 0.0 {
                dangling += rank[from];
                continue;
            }
            for (to, weight) in edges {
                next[*to] += damping * rank[from] * weight / total;
            }
        }
        for (i, value) in next.iter_mut().enumerate() {
            *value += damping * dangling * teleport[i];
        }
        rank = next;
    }
    nodes
        .into_iter()
        .zip(rank)
        .filter(|(id, _)| !seeds.contains(id))
        .collect()
}

/// Whether anything might use `symbol`: a resolved edge to it, or an
/// unresolved reference by its name.
pub fn is_referenced(conn: &Connection, symbol: &SymbolRecord) -> Result<bool> {
    let resolved: bool = conn
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM edges WHERE to_symbol_id = ?1)")?
        .query_row(params![symbol.id], |row| row.get(0))?;
    if resolved {
        return Ok(true);
    }
    let unresolved: bool = conn
        .prepare_cached(
            "SELECT EXISTS(SELECT 1 FROM refs r WHERE r.symbol_name = ?1 AND (
                 r.from_symbol_id IS NULL
                 OR EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id
                            AND e.to_symbol_id IS NULL AND e.resolver IS NULL)))",
        )?
        .query_row(params![symbol.name], |row| row.get(0))?;
    Ok(unresolved)
}
