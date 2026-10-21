//! Turns name-level references into resolved edges: module specifiers to
//! files, then names to symbols through scope, imports, `this` and type hints.

pub mod modules;
mod scope;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db;
pub use modules::{ModuleResolver, ModuleTarget};

/// Re-resolving more dependents than this falls back to one full pass.
pub const MAX_INCREMENTAL_FILES: usize = 2_000;
const META_KEY: &str = "resolve";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveReport {
    pub files: usize,
    pub edges: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub unresolved: usize,
    /// References to a repository module or class that lacks the name.
    #[serde(default)]
    pub missing: usize,
    pub elapsed_ms: u128,
    /// An incremental pass had too many dependents and resolved everything.
    pub fallback: bool,
}

#[derive(Debug)]
pub enum ResolveScope {
    All,
    /// Files whose content changed, and what the files they replaced (or
    /// that were removed) offered; their dependents are added.
    Changed(Vec<i64>, Before),
}

/// What re-indexed or removed files offered the rest of the repository,
/// recorded just before an incremental index deletes their rows.
#[derive(Debug, Default)]
pub struct Before {
    files: Vec<Snapshot>,
}

#[derive(Debug)]
struct Snapshot {
    shape: Shape,
    /// Edges from other files into the file's symbols: the edge, the file
    /// holding its reference, and the target's key.
    incoming: Vec<(i64, i64, String)>,
}

impl Before {
    /// Records `file_id`; call it before the file's rows are deleted.
    pub fn record(&mut self, conn: &Connection, file_id: i64) -> Result<()> {
        let shape = Shape::read(conn, file_id)?;
        let mut incoming = Vec::new();
        let mut stmt = conn.prepare_cached(
            "SELECT e.id, r.file_id, e.to_symbol_id FROM symbols s
             JOIN edges e ON e.to_symbol_id = s.id
             JOIN refs r ON r.id = e.ref_id
             WHERE s.file_id = ?1 AND r.file_id != ?1",
        )?;
        let rows = stmt.query_map(params![file_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        for row in rows {
            let (edge, file, symbol): (i64, i64, i64) = row?;
            if let Some(key) = shape.keys.get(&symbol) {
                incoming.push((edge, file, key.clone()));
            }
        }
        self.files.push(Snapshot { shape, incoming });
        Ok(())
    }
}

/// A file as the resolution of other files sees it.
#[derive(Debug)]
struct Shape {
    id: i64,
    path: String,
    module: Option<String>,
    /// Everything another file's references can resolve through: top-level
    /// symbols and class members with their kinds, export flags and arity,
    /// import and re-export bindings, module-level, field and return type
    /// hints, and heritage clauses. Function bodies are not in it.
    surface: Vec<String>,
    /// How many symbols carry each name, for repo-wide name matches.
    names: HashMap<String, usize>,
    /// Symbol id -> a key that survives re-indexing the file: kind,
    /// qualified name, and position among symbols sharing both.
    keys: HashMap<i64, String>,
}

type SymbolRow = (
    i64,
    String,
    String,
    String,
    Option<String>,
    bool,
    bool,
    Option<i64>,
    Option<i64>,
);

impl Shape {
    fn read(conn: &Connection, file_id: i64) -> Result<Self> {
        const SEP: char = '\u{1f}';
        let (path, language, module): (String, String, Option<String>) = conn.query_row(
            "SELECT path, language, module FROM files WHERE id = ?1",
            params![file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let symbols: Vec<SymbolRow> = conn
            .prepare_cached(
                "SELECT id, name, qualified_name, kind, container, exported, default_export,
                        arity_min, arity_max
                 FROM symbols WHERE file_id = ?1 ORDER BY id",
            )?
            .query_map(params![file_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let classes: HashSet<&str> = symbols
            .iter()
            .filter(|s| scope::CLASS_KINDS.contains(&s.3.as_str()))
            .map(|s| s.2.as_str())
            .collect();
        let mut surface = vec![format!("l{SEP}{language}{SEP}{module:?}")];
        let mut names: HashMap<String, usize> = HashMap::new();
        let mut keys = HashMap::new();
        let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
        for (id, name, qname, kind, container, exported, default, min, max) in &symbols {
            let n = seen.entry((kind.as_str(), qname.as_str())).or_default();
            keys.insert(*id, format!("{kind}{SEP}{qname}{SEP}{n}"));
            *n += 1;
            if kind == "callback" {
                continue;
            }
            *names.entry(name.clone()).or_default() += 1;
            if container
                .as_deref()
                .is_none_or(|container| classes.contains(container))
            {
                surface.push(format!(
                    "s{SEP}{qname}{SEP}{kind}{SEP}{exported}{SEP}{default}{SEP}{min:?}{SEP}{max:?}"
                ));
            }
        }
        let tables = [
            (
                'b',
                "SELECT kind, local, imported, source FROM import_bindings WHERE file_id = ?1",
            ),
            (
                'h',
                "SELECT scope, name, type_name, origin FROM type_hints
                 WHERE file_id = ?1 AND (scope IS NULL OR name LIKE 'this.%' OR origin = 'return')",
            ),
            (
                'r',
                "SELECT s.qualified_name, r.kind, r.symbol_name, r.receiver_name
                 FROM refs r JOIN symbols s ON s.id = r.from_symbol_id
                 WHERE r.file_id = ?1 AND r.kind IN ('extends', 'implements')",
            ),
        ];
        for (tag, sql) in tables {
            let mut stmt = conn.prepare_cached(sql)?;
            let lines = stmt.query_map(params![file_id], |row| {
                let mut line = tag.to_string();
                for i in 0..4 {
                    line.push(SEP);
                    line.push_str(&row.get::<_, Option<String>>(i)?.unwrap_or_default());
                }
                Ok(line)
            })?;
            for line in lines {
                surface.push(line?);
            }
        }
        surface.sort_unstable();
        Ok(Self {
            id: file_id,
            path,
            module,
            surface,
            names,
            keys,
        })
    }
}

/// Rewrites the edges of every file in `scope` (plus dependents for
/// `Changed`) with resolved targets, confidence and resolver.
pub fn resolve(conn: &Connection, root: &Path, scope: ResolveScope) -> Result<ResolveReport> {
    let started = Instant::now();
    let mut report = ResolveReport::default();
    let all_ids = || -> Result<Vec<i64>> {
        Ok(conn
            .prepare("SELECT id FROM files ORDER BY id")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };
    let (files, full) = match scope {
        ResolveScope::All => (all_ids()?, true),
        ResolveScope::Changed(changed, before) => {
            let now: Vec<Shape> = changed
                .iter()
                .map(|id| Shape::read(conn, *id))
                .collect::<Result<_>>()?;
            let lost = repoint(conn, &now, &before)?;
            let expanded = dependents(conn, &now, &before, lost)?;
            if expanded.len() > MAX_INCREMENTAL_FILES {
                report.fallback = true;
                (all_ids()?, true)
            } else {
                (expanded, false)
            }
        }
    };
    let modules = ModuleResolver::build(conn, root)?;
    let mut resolver = scope::Resolver::new(conn, root, modules, full)?;
    let mut set_target = conn
        .prepare_cached("UPDATE import_bindings SET target_path = ?2, status = ?3 WHERE id = ?1")?;
    let mut delete_edges = conn.prepare_cached(
        "DELETE FROM edges WHERE ref_id IN (SELECT id FROM refs WHERE file_id = ?1)",
    )?;
    let mut insert_edge = conn.prepare_cached(
        "INSERT INTO edges(from_symbol_id, to_symbol_name, to_symbol_id, kind, confidence,
                           resolver, ambiguous_group, ref_id, weight, missing_in,
                           arity_mismatch)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for file_id in &files {
        let file = resolver.file(*file_id)?;
        for binding in &file.bindings {
            let target = resolver.binding_target_path(&file, binding);
            let status = resolver.binding_status(&file, binding)?;
            set_target.execute(params![binding.id, target, status])?;
        }
        delete_edges.execute(params![file_id])?;
        for reference in resolver.refs(*file_id)? {
            let resolution = resolver.resolve_ref(&file, &reference)?;
            match resolution.confidence {
                _ if resolution.resolver == Some("missing") => report.missing += 1,
                "high" => report.high += 1,
                "medium" => report.medium += 1,
                "low" => report.low += 1,
                _ => report.unresolved += 1,
            }
            // A reference known to point outside the repository, or at a
            // parameter or local variable, gets no edge: a reference with an
            // enclosing symbol and no edge is external.
            if matches!(resolution.resolver, Some("external" | "value")) {
                continue;
            }
            let targets: Vec<Option<i64>> = if resolution.targets.is_empty() {
                vec![None]
            } else {
                resolution.targets.iter().copied().map(Some).collect()
            };
            let group = (targets.len() > 1).then_some(reference.id);
            let weight = 1.0 / targets.len() as f64;
            for target in targets {
                insert_edge.execute(params![
                    reference.from,
                    reference.name,
                    target,
                    reference.kind,
                    resolution.confidence,
                    resolution.resolver,
                    group,
                    reference.id,
                    weight,
                    resolution.missing_in,
                    resolution.arity_mismatch
                ])?;
                report.edges += 1;
            }
        }
        drop(file);
    }
    report.files = files.len();
    report.elapsed_ms = started.elapsed().as_millis();
    db::set_meta(conn, META_KEY, &serde_json::to_string(&report)?)?;
    Ok(report)
}

/// The report of the last resolve pass, if any.
pub fn last_report(conn: &Connection) -> Result<Option<ResolveReport>> {
    Ok(db::get_meta(conn, META_KEY)?.and_then(|json| serde_json::from_str(&json).ok()))
}

/// Points edges from other files at the re-indexed copies of their targets
/// (same key); returns the files whose edges lost a target that is gone.
fn repoint(conn: &Connection, now: &[Shape], before: &Before) -> Result<HashSet<i64>> {
    let by_path: HashMap<&str, &Shape> = now.iter().map(|s| (s.path.as_str(), s)).collect();
    let mut update = conn.prepare_cached("UPDATE edges SET to_symbol_id = ?2 WHERE id = ?1")?;
    let mut lost = HashSet::new();
    for snapshot in &before.files {
        let keys: HashMap<&str, i64> = by_path
            .get(snapshot.shape.path.as_str())
            .map(|shape| {
                shape
                    .keys
                    .iter()
                    .map(|(id, key)| (key.as_str(), *id))
                    .collect()
            })
            .unwrap_or_default();
        for (edge, file, key) in &snapshot.incoming {
            match keys.get(key.as_str()) {
                Some(symbol) => {
                    update.execute(params![edge, symbol])?;
                }
                None => {
                    lost.insert(*file);
                }
            }
        }
    }
    // A referencing file re-indexed in the same pass has a new id.
    let mut exists = conn.prepare_cached("SELECT 1 FROM files WHERE id = ?1")?;
    let mut kept = HashSet::new();
    for file in lost {
        if exists.exists(params![file])? {
            kept.insert(file);
        }
    }
    Ok(kept)
}

/// The changed files plus every file whose edges may now resolve
/// differently: files whose edges lost their target (`lost`), files with
/// names reported missing from a changed or removed file, files with
/// name-matched edges to a name the changed files define, and, for a file
/// whose surface changed (or that is new or removed), its importers through
/// re-export chains, its Java package, and files with unresolved relative
/// imports it may satisfy.
fn dependents(
    conn: &Connection,
    now: &[Shape],
    before: &Before,
    lost: HashSet<i64>,
) -> Result<Vec<i64>> {
    let mut files: HashSet<i64> = now.iter().map(|shape| shape.id).collect();
    files.extend(lost);
    let ids = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<Vec<i64>> {
        Ok(conn
            .prepare_cached(sql)?
            .query_map(args, |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };

    let mut previous: HashMap<&str, &Shape> = HashMap::new();
    for snapshot in &before.files {
        previous
            .entry(snapshot.shape.path.as_str())
            .or_insert(&snapshot.shape);
    }
    let current: HashSet<&str> = now.iter().map(|shape| shape.path.as_str()).collect();
    // Paths whose importers must be re-resolved, every changed or removed
    // path, and the paths that appeared or disappeared.
    let mut follow: Vec<String> = Vec::new();
    let mut touched: Vec<&str> = Vec::new();
    let mut appeared: Vec<&str> = Vec::new();
    let mut packages: HashSet<&str> = HashSet::new();
    let mut names: HashSet<&str> = HashSet::new();
    for shape in now {
        touched.push(&shape.path);
        names.extend(shape.names.keys().map(String::as_str));
        let old = previous.get(shape.path.as_str());
        if old.is_none_or(|old| old.surface != shape.surface) {
            follow.push(shape.path.clone());
            packages.extend(shape.module.as_deref());
            packages.extend(old.and_then(|old| old.module.as_deref()));
        }
        if old.is_none() {
            appeared.push(&shape.path);
        }
    }
    for (path, old) in &previous {
        if !current.contains(path) {
            touched.push(path);
            follow.push(path.to_string());
            appeared.push(path);
            packages.extend(old.module.as_deref());
        }
    }

    let mut seen_paths: HashSet<String> = HashSet::new();
    while let Some(path) = follow.pop() {
        if !seen_paths.insert(path.clone()) || files.len() > MAX_INCREMENTAL_FILES {
            continue;
        }
        let importers: Vec<(i64, bool, String)> = conn
            .prepare_cached(
                "SELECT b.file_id, MAX(b.kind IN ('re_export', 're_export_all')), f.path
                 FROM import_bindings b JOIN files f ON f.id = b.file_id
                 WHERE b.target_path = ?1 GROUP BY b.file_id",
            )?
            .query_map(params![path], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (file_id, re_exports, importer_path) in importers {
            files.insert(file_id);
            if re_exports {
                follow.push(importer_path);
            }
        }
    }

    // A name reported missing from a changed file may be there now, or a
    // property it gained may now declare it.
    for path in &touched {
        files.extend(ids(
            "SELECT DISTINCT r.file_id FROM edges e JOIN refs r ON r.id = e.ref_id
             WHERE e.missing_in = ?1 AND e.resolver = 'missing'",
            &[path],
        )?);
    }
    for name in &names {
        if files.len() > MAX_INCREMENTAL_FILES {
            break;
        }
        files.extend(ids(
            "SELECT DISTINCT r.file_id FROM edges e JOIN refs r ON r.id = e.ref_id
             WHERE e.to_symbol_name = ?1 AND e.confidence IN ('medium', 'low', 'name')
               AND (e.resolver IS NULL OR e.resolver = 'name')",
            &[name],
        )?);
    }
    for package in &packages {
        files.extend(ids("SELECT id FROM files WHERE module = ?1", &[package])?);
    }

    if !appeared.is_empty() {
        let mut stems: HashSet<String> = HashSet::new();
        for path in &appeared {
            let path = Path::new(path);
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let stem = stem.unwrap_or_default();
            let stem = stem.trim_end_matches(".d").to_string();
            if stem == "index" {
                if let Some(dir) = path.parent().and_then(Path::file_name) {
                    stems.insert(dir.to_string_lossy().into_owned());
                }
            }
            stems.insert(stem);
        }
        let unresolved: Vec<(i64, String)> = conn
            .prepare_cached(
                "SELECT file_id, source FROM import_bindings
                 WHERE target_path IS NULL AND (source LIKE '.%' OR source LIKE '@%/%')",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (file_id, source) in unresolved {
            let stem = source.rsplit('/').next().unwrap_or(&source);
            let stem = stem.split('.').next().unwrap_or(stem);
            if stems.contains(stem) {
                files.insert(file_id);
            }
        }
    }

    let mut files: Vec<i64> = files.into_iter().collect();
    files.sort_unstable();
    Ok(files)
}
