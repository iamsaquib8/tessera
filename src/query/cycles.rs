use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::output::AgentText;
use crate::resolve::modules::{ModuleResolver, ModuleTarget};
use crate::types::Language;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CyclesArgs {
    /// Only cycles involving modules under this path prefix.
    pub path: Option<String>,
    /// Maximum number of cycles to return (default: 100).
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCycleLink {
    pub from: String,
    pub to: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCycle {
    pub modules: usize,
    pub links: Vec<ImportCycleLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CyclesResult {
    pub total_cycles: usize,
    pub cycles: Vec<ImportCycle>,
}

impl fmt::Display for CyclesResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.cycles.is_empty() {
            return writeln!(f, "No circular import cycles found.");
        }
        let word = if self.cycles.len() == 1 {
            "cycle"
        } else {
            "cycles"
        };
        writeln!(f, "Found {} circular import {word}:", self.cycles.len())?;
        for (idx, cycle) in self.cycles.iter().enumerate() {
            let mod_word = if cycle.modules == 1 {
                "module"
            } else {
                "modules"
            };
            writeln!(f, "  {}. ({} {mod_word}):", idx + 1, cycle.modules)?;
            for link in &cycle.links {
                writeln!(f, "     {} -> {}:{}", link.from, link.to, link.line)?;
            }
        }
        Ok(())
    }
}

impl AgentText for CyclesResult {
    fn agent_text(&self) -> String {
        self.to_string()
    }
}

pub fn run(conn: &Connection, root: &Path, args: &CyclesArgs) -> Result<CyclesResult> {
    let limit = if args.limit == 0 {
        100
    } else {
        args.limit.min(1000)
    };
    let (paths, adj, edge_line) = collect_import_edges(conn, root)?;
    let raw_cycles = find_cycles(&adj, limit);

    let prefix = args
        .path
        .as_deref()
        .map(|p| p.trim().trim_start_matches("./").replace('\\', "/"));

    let mut cycles = Vec::new();
    for raw in raw_cycles {
        let mut links = Vec::with_capacity(raw.len());
        for i in 0..raw.len() {
            let u = raw[i];
            let v = raw[(i + 1) % raw.len()];
            let from = paths[u].clone();
            let to = paths[v].clone();
            let line = *edge_line.get(&(u, v)).unwrap_or(&1);
            links.push(ImportCycleLink { from, to, line });
        }

        if let Some(ref p) = prefix {
            if !links
                .iter()
                .any(|l| l.from.starts_with(p) || l.to.starts_with(p))
            {
                continue;
            }
        }

        cycles.push(ImportCycle {
            modules: raw.len(),
            links,
        });
    }

    cycles.sort_by(|a, b| {
        a.modules.cmp(&b.modules).then_with(|| {
            let a_first = a.links.first().map(|l| l.from.as_str()).unwrap_or("");
            let b_first = b.links.first().map(|l| l.from.as_str()).unwrap_or("");
            a_first.cmp(b_first)
        })
    });

    if cycles.len() > limit {
        cycles.truncate(limit);
    }

    Ok(CyclesResult {
        total_cycles: cycles.len(),
        cycles,
    })
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
            _ => {}
        }
    }
    out
}

type ImportGraph = (Vec<String>, Vec<Vec<usize>>, HashMap<(usize, usize), usize>);

fn collect_import_edges(conn: &Connection, root: &Path) -> Result<ImportGraph> {
    let mut stmt = conn.prepare("SELECT id, path, language FROM files ORDER BY path")?;
    let mut paths = Vec::new();
    let mut path_to_idx = HashMap::new();
    let mut id_to_path = HashMap::new();

    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (id, path, _lang) = row?;
        let idx = paths.len();
        path_to_idx.insert(path.clone(), idx);
        id_to_path.insert(id, path.clone());
        paths.push(path);
    }

    let mut edges_map: HashMap<(usize, usize), usize> = HashMap::new();

    // 1. Direct resolved import bindings
    let mut stmt = conn.prepare(
        "SELECT f.path, b.target_path, MIN(b.line)
         FROM import_bindings b
         JOIN files f ON f.id = b.file_id
         WHERE b.target_path IS NOT NULL AND b.target_path != f.path
         GROUP BY f.path, b.target_path",
    )?;
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })? {
        let (from_path, to_path, line) = row?;
        let to_path_norm = to_path.replace('\\', "/");
        if let (Some(&u), Some(&v)) = (path_to_idx.get(&from_path), path_to_idx.get(&to_path_norm))
        {
            if u != v {
                edges_map
                    .entry((u, v))
                    .and_modify(|l| *l = (*l).min(line as usize))
                    .or_insert(line as usize);
            }
        }
    }

    // 2. Unresolved imports via ModuleResolver or relative resolution
    let resolver = ModuleResolver::build(conn, root)?;
    let mut stmt = conn.prepare(
        "SELECT f.path, f.language, i.source, i.line
         FROM imports i
         JOIN files f ON f.id = i.file_id",
    )?;
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })? {
        let (from_path, lang_str, source, line) = row?;
        if let Some(&u) = path_to_idx.get(&from_path) {
            let mut resolved_v = None;
            if let Some(lang) = Language::from_name(&lang_str) {
                if let ModuleTarget::File(target_id) = resolver.resolve(&from_path, &source, lang) {
                    if let Some(target_path) = id_to_path.get(&target_id) {
                        resolved_v = path_to_idx.get(target_path).copied();
                    }
                }
            }

            // Fallback for relative specifiers in other languages
            if resolved_v.is_none() && source.starts_with('.') {
                let base = Path::new(&from_path).parent().unwrap_or(Path::new(""));
                let joined = base.join(&source);
                let normalized = normalize(&joined);
                let norm_str = normalized.to_string_lossy().replace('\\', "/");
                if let Some(&v) = path_to_idx.get(&norm_str) {
                    resolved_v = Some(v);
                } else if let Some(ext) = Path::new(&from_path).extension().and_then(|e| e.to_str())
                {
                    let with_ext = format!("{norm_str}.{ext}");
                    if let Some(&v) = path_to_idx.get(&with_ext) {
                        resolved_v = Some(v);
                    }
                }
            }

            if let Some(v) = resolved_v {
                if u != v {
                    edges_map
                        .entry((u, v))
                        .and_modify(|l| *l = (*l).min(line as usize))
                        .or_insert(line as usize);
                }
            }
        }
    }

    let mut adj = vec![Vec::new(); paths.len()];
    for &(u, v) in edges_map.keys() {
        adj[u].push(v);
    }
    for list in &mut adj {
        list.sort_unstable();
    }

    Ok((paths, adj, edges_map))
}

struct JohnsonSearch<'a> {
    s: usize,
    scc: &'a HashSet<usize>,
    adj: &'a [Vec<usize>],
    blocked: Vec<bool>,
    b_map: Vec<Vec<usize>>,
    stack: Vec<usize>,
    cycles: Vec<Vec<usize>>,
    limit: usize,
}

impl<'a> JohnsonSearch<'a> {
    fn new(s: usize, scc: &'a HashSet<usize>, adj: &'a [Vec<usize>], limit: usize) -> Self {
        let n = adj.len();
        Self {
            s,
            scc,
            adj,
            blocked: vec![false; n],
            b_map: vec![Vec::new(); n],
            stack: Vec::new(),
            cycles: Vec::new(),
            limit,
        }
    }

    fn circuit(&mut self, v: usize) -> bool {
        let mut f = false;
        self.stack.push(v);
        self.blocked[v] = true;

        for &w in &self.adj[v] {
            if !self.scc.contains(&w) {
                continue;
            }
            if w == self.s {
                if self.stack.len() >= 2 {
                    self.cycles.push(self.stack.clone());
                }
                f = true;
                if self.cycles.len() >= self.limit {
                    break;
                }
            } else if !self.blocked[w] {
                if self.circuit(w) {
                    f = true;
                }
                if self.cycles.len() >= self.limit {
                    break;
                }
            }
        }

        if f {
            self.unblock(v);
        } else {
            for &w in &self.adj[v] {
                if self.scc.contains(&w) && !self.b_map[w].contains(&v) {
                    self.b_map[w].push(v);
                }
            }
        }

        self.stack.pop();
        f
    }

    fn unblock(&mut self, v: usize) {
        self.blocked[v] = false;
        let dependents = std::mem::take(&mut self.b_map[v]);
        for w in dependents {
            if self.blocked[w] {
                self.unblock(w);
            }
        }
    }
}

/// Johnson's elementary cycle finding algorithm.
/// Guarantees that every simple directed cycle of length >= 2 is found exactly once,
/// canonicalized to start at its lowest-index node, without infinite loops.
pub fn find_cycles(adj: &[Vec<usize>], limit: usize) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut all_cycles = Vec::new();
    let mut s = 0;

    while s < n && all_cycles.len() < limit {
        let Some((least_scc_node, scc)) = find_least_scc(s, n, adj) else {
            break;
        };
        s = least_scc_node;

        let remaining = limit - all_cycles.len();
        let mut search = JohnsonSearch::new(s, &scc, adj, remaining);
        search.circuit(s);
        all_cycles.append(&mut search.cycles);

        s += 1;
    }

    all_cycles
}

struct TarjanScc<'a> {
    s: usize,
    adj: &'a [Vec<usize>],
    index: usize,
    indices: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    sccs: Vec<HashSet<usize>>,
}

impl<'a> TarjanScc<'a> {
    fn new(s: usize, n: usize, adj: &'a [Vec<usize>]) -> Self {
        Self {
            s,
            adj,
            index: 0,
            indices: vec![None; n],
            lowlink: vec![0; n],
            on_stack: vec![false; n],
            stack: Vec::new(),
            sccs: Vec::new(),
        }
    }

    fn strongconnect(&mut self, v: usize) {
        self.indices[v] = Some(self.index);
        self.lowlink[v] = self.index;
        self.index += 1;
        self.stack.push(v);
        self.on_stack[v] = true;

        for &w in &self.adj[v] {
            if w < self.s {
                continue;
            }
            if self.indices[w].is_none() {
                self.strongconnect(w);
                self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
            } else if self.on_stack[w] {
                self.lowlink[v] = self.lowlink[v].min(self.indices[w].unwrap());
            }
        }

        if self.lowlink[v] == self.indices[v].unwrap() {
            let mut scc = HashSet::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack[w] = false;
                scc.insert(w);
                if w == v {
                    break;
                }
            }
            self.sccs.push(scc);
        }
    }
}

fn find_least_scc(s: usize, n: usize, adj: &[Vec<usize>]) -> Option<(usize, HashSet<usize>)> {
    let mut tarjan = TarjanScc::new(s, n, adj);

    for i in s..n {
        if tarjan.indices[i].is_none() {
            tarjan.strongconnect(i);
        }
    }

    let mut best: Option<(usize, HashSet<usize>)> = None;
    for scc in tarjan.sccs {
        let is_non_trivial = if scc.len() >= 2 {
            true
        } else if scc.len() == 1 {
            let u = *scc.iter().next().unwrap();
            adj[u].contains(&u)
        } else {
            false
        };

        if is_non_trivial {
            let min_node = *scc.iter().min().unwrap();
            if min_node >= s && best.as_ref().is_none_or(|(b_min, _)| min_node < *b_min) {
                best = Some((min_node, scc));
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_johnson_elementary_cycles() {
        // Simple 3-cycle: 0 -> 1 -> 2 -> 0
        let adj = vec![vec![1], vec![2], vec![0]];
        let cycles = find_cycles(&adj, 10);
        assert_eq!(cycles, vec![vec![0, 1, 2]]);

        // Two disjoint cycles: 0 -> 1 -> 0 and 2 -> 3 -> 2
        let adj = vec![vec![1], vec![0], vec![3], vec![2]];
        let cycles = find_cycles(&adj, 10);
        assert_eq!(cycles, vec![vec![0, 1], vec![2, 3]]);

        // Figure-8 graph: 0 -> 1 -> 0 and 0 -> 2 -> 0
        let adj = vec![vec![1, 2], vec![0], vec![0]];
        let cycles = find_cycles(&adj, 10);
        assert_eq!(cycles, vec![vec![0, 1], vec![0, 2]]);

        // DAG with no cycles
        let adj = vec![vec![1, 2], vec![2], vec![]];
        let cycles = find_cycles(&adj, 10);
        assert!(cycles.is_empty());

        // Self-loop: 0 -> 0 (should be skipped because length < 2)
        let adj = vec![vec![0]];
        let cycles = find_cycles(&adj, 10);
        assert!(cycles.is_empty());
    }
}
