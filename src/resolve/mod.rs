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

#[derive(Debug, Clone)]
pub enum ResolveScope {
    All,
    /// Files whose content changed; their dependents are added.
    Changed(Vec<i64>),
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
        ResolveScope::Changed(changed) => {
            let expanded = dependents(conn, &changed)?;
            if expanded.len() > MAX_INCREMENTAL_FILES {
                report.fallback = true;
                (all_ids()?, true)
            } else {
                (expanded, false)
            }
        }
    };
    let modules = ModuleResolver::build(conn, root)?;
    let mut resolver = scope::Resolver::new(conn, modules, full)?;
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
            // A reference known to point outside the repository gets no edge:
            // a reference with an enclosing symbol and no edge is external.
            if resolution.resolver == Some("external") {
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

/// `changed` plus every file whose edges may now resolve differently: files
/// importing a changed file (through re-export chains), files whose resolved
/// edges lost their target, files with name-matched edges to a name the
/// changed files define, same-package Java files, and files with unresolved
/// relative imports that a changed file may now satisfy.
fn dependents(conn: &Connection, changed: &[i64]) -> Result<Vec<i64>> {
    let mut files: HashSet<i64> = changed.iter().copied().collect();
    let ids = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<Vec<i64>> {
        Ok(conn
            .prepare_cached(sql)?
            .query_map(args, |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };
    let strings = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<Vec<String>> {
        Ok(conn
            .prepare_cached(sql)?
            .query_map(args, |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };

    let mut paths: Vec<String> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut packages: HashSet<String> = HashSet::new();
    for id in changed {
        paths.extend(strings("SELECT path FROM files WHERE id = ?1", &[id])?);
        names.extend(strings(
            "SELECT name FROM symbols WHERE file_id = ?1",
            &[id],
        )?);
        packages.extend(strings(
            "SELECT module FROM files WHERE id = ?1 AND language = 'java' AND module IS NOT NULL",
            &[id],
        )?);
    }

    let mut seen_paths: HashSet<String> = HashSet::new();
    while let Some(path) = paths.pop() {
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
                paths.push(importer_path);
            }
        }
    }

    files.extend(ids(
        "SELECT DISTINCT r.file_id FROM edges e JOIN refs r ON r.id = e.ref_id
         WHERE e.to_symbol_id IS NULL AND e.confidence != 'name'",
        &[],
    )?);
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

    let mut stems: HashSet<String> = HashSet::new();
    for path in &seen_paths {
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
    let by_stem: HashMap<String, Vec<i64>> =
        unresolved
            .into_iter()
            .fold(HashMap::new(), |mut acc, (file_id, source)| {
                let stem = source.rsplit('/').next().unwrap_or(&source);
                let stem = stem.split('.').next().unwrap_or(stem).to_string();
                acc.entry(stem).or_default().push(file_id);
                acc
            });
    for stem in &stems {
        if let Some(found) = by_stem.get(stem) {
            files.extend(found);
        }
    }

    let mut files: Vec<i64> = files.into_iter().collect();
    files.sort_unstable();
    Ok(files)
}
