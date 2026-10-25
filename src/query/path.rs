//! `path`: the shortest call chain between two symbols. The search runs over
//! (repository, symbol) nodes: within a repository it follows the call
//! graph, and contracts add hops from a client to the handler that serves
//! it (or a producer to its consumers), in the same repository or in
//! another one of the workspace.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;

use super::{graph, legacy, map};
use crate::output::AgentText;
use crate::types::{ConnectResult, PathNode, QueryMeta, SymbolRecord};

const DEFAULT_DEPTH: usize = 8;
/// Nodes the search visits at most before giving up.
const MAX_VISITED: usize = 5000;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathArgs {
    pub from: String,
    pub to: String,
    pub depth: usize,
}

/// A symbol in one repository: the repository's index in `Sources::repos`
/// and the symbol's id in that repository's index.
type Node = (usize, i64);

pub fn run(conn: &Connection, root: &Path, args: &PathArgs) -> Result<ConnectResult> {
    let depth = if args.depth == 0 {
        DEFAULT_DEPTH
    } else {
        args.depth
    };
    let sources = map::Sources::open(conn, root)?;
    search(
        sources.repos(),
        &map::hops(&sources)?,
        args,
        depth.clamp(1, 12),
    )
}

fn search(
    repos: &[(String, Connection)],
    hops: &[map::Hop],
    args: &PathArgs,
    max_depth: usize,
) -> Result<ConnectResult> {
    let not_found = || ConnectResult {
        from: args.from.clone(),
        to: args.to.clone(),
        found: false,
        path: Vec::new(),
        meta: QueryMeta { tokens: 40 },
    };
    let starts = resolve(repos, &args.from, 25)?;
    let targets = resolve(repos, &args.to, 50)?;
    if starts.is_empty() || targets.is_empty() {
        return Ok(not_found());
    }
    let target_ids: HashSet<Node> = targets.iter().map(|(r, s)| (*r, s.id)).collect();
    let node_of = |(repo, s): &(usize, SymbolRecord)| PathNode {
        qualified_name: s.qualified_name.clone(),
        kind: s.kind.clone(),
        path: format!("{}{}", repos[*repo].0, s.path),
        line: s.start_line,
        via: None,
    };

    // Trivial: `from` already resolves to a target symbol.
    if let Some(start) = starts
        .iter()
        .find(|(r, s)| target_ids.contains(&(*r, s.id)))
    {
        return Ok(found(args, vec![node_of(start)]));
    }

    let links = links(repos, hops);
    // Breadth-first over (repository, symbol), each visited once, so a cycle
    // between repositories ends. `prev` absent means a start node.
    let mut visited: HashSet<Node> = HashSet::new();
    let mut prev: HashMap<Node, (Node, Option<String>)> = HashMap::new();
    let mut record: HashMap<Node, (usize, SymbolRecord)> = HashMap::new();
    let mut queue: VecDeque<(Node, usize)> = VecDeque::new();
    for start in &starts {
        let node = (start.0, start.1.id);
        if visited.insert(node) {
            record.insert(node, start.clone());
            queue.push_back((node, 0));
        }
    }

    let mut hit: Option<Node> = None;
    'bfs: while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let (repo, symbol) = record[&node].clone();
        let conn = &repos[repo].1;
        let mut next: Vec<((usize, SymbolRecord), Option<String>)> =
            legacy::callees_for_symbol(conn, &symbol, 200)?
                .into_iter()
                .map(|callee| ((repo, callee), None))
                .collect();
        for ((to_repo, id), via) in links.get(&node).into_iter().flatten() {
            if let Some(target) = graph::symbol_by_id(&repos[*to_repo].1, *id)? {
                next.push(((*to_repo, target), Some(via.clone())));
            }
        }
        for (callee, via) in next {
            let id = (callee.0, callee.1.id);
            if !visited.insert(id) {
                continue;
            }
            prev.insert(id, (node, via));
            record.insert(id, callee);
            if target_ids.contains(&id) {
                hit = Some(id);
                break 'bfs;
            }
            queue.push_back((id, depth + 1));
            if visited.len() >= MAX_VISITED {
                break 'bfs;
            }
        }
    }

    let Some(hit) = hit else {
        return Ok(not_found());
    };
    let mut chain = vec![(hit, None)];
    let mut cur = hit;
    while let Some((p, via)) = prev.get(&cur) {
        chain.last_mut().expect("chain is never empty").1 = via.clone();
        chain.push((*p, None));
        cur = *p;
    }
    chain.reverse();
    let path = chain
        .into_iter()
        .map(|(id, via)| PathNode {
            via,
            ..node_of(&record[&id])
        })
        .collect();
    Ok(found(args, path))
}

fn found(args: &PathArgs, path: Vec<PathNode>) -> ConnectResult {
    let tokens = serde_json::to_vec(&path).map_or(0, |json| json.len()) / 4;
    ConnectResult {
        from: args.from.clone(),
        to: args.to.clone(),
        found: true,
        path,
        meta: QueryMeta {
            tokens: tokens.max(40),
        },
    }
}

/// The repository a prefixed path belongs to: the longest prefix it starts
/// with.
fn repo_of(repos: &[(String, Connection)], path: &str) -> Option<usize> {
    repos
        .iter()
        .enumerate()
        .filter(|(_, (prefix, _))| path.starts_with(prefix.as_str()))
        .max_by_key(|(_, (prefix, _))| prefix.len())
        .map(|(i, _)| i)
}

/// Contract hops by node, each with the contract it crosses.
fn links(repos: &[(String, Connection)], hops: &[map::Hop]) -> HashMap<Node, Vec<(Node, String)>> {
    let mut out: HashMap<Node, Vec<(Node, String)>> = HashMap::new();
    for hop in hops {
        let (Some(from), Some(to)) = (repo_of(repos, &hop.from.0), repo_of(repos, &hop.to.0))
        else {
            continue;
        };
        let targets = out.entry((from, hop.from.1)).or_default();
        let to = (to, hop.to.1);
        if !targets.iter().any(|(t, _)| *t == to) {
            targets.push((to, hop.via.clone()));
        }
    }
    out
}

/// The symbols `name` names, best first: in the repository its prefix
/// (`web/`) names, or in any repository.
fn resolve(
    repos: &[(String, Connection)],
    name: &str,
    limit: usize,
) -> Result<Vec<(usize, SymbolRecord)>> {
    let scoped = repos.iter().enumerate().find_map(|(i, (prefix, _))| {
        let rest = name
            .strip_prefix(prefix.as_str())
            .filter(|_| !prefix.is_empty())?;
        (!rest.is_empty()).then_some((i, rest))
    });
    let mut out = Vec::new();
    match scoped {
        Some((repo, rest)) => {
            for symbol in legacy::exact_symbols(&repos[repo].1, rest, limit)? {
                out.push((repo, symbol));
            }
        }
        None => {
            for (repo, (_, conn)) in repos.iter().enumerate() {
                for symbol in legacy::exact_symbols(conn, name, limit)? {
                    out.push((repo, symbol));
                }
            }
        }
    }
    let wanted = scoped.map_or(name, |(_, rest)| rest);
    out.sort_by_cached_key(|(repo, s)| {
        (
            s.qualified_name != wanted,
            s.qualified_name.chars().count(),
            format!("{}{}", repos[*repo].0, s.path),
        )
    });
    out.truncate(limit);
    Ok(out)
}

impl AgentText for ConnectResult {
    fn agent_text(&self) -> String {
        self.to_string()
    }
}
