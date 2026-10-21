use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use strsim::jaro_winkler;

use crate::db;
use crate::output::MatchKind;
use crate::types::{
    ConnectResult, DefinitionResult, DiffChangedSymbol, DiffImpactResult, DiffImpactedSymbol,
    ExportResult, ImportRecord, ImportsResult, KindCount, LanguageCount, OutlineResult, PathNode,
    QueryMeta, SearchHit, SearchOptions, SearchResult, Sibling, SiblingsResult, SignatureLine,
    SignatureResult, StatsResult, SymbolRecord, SymbolSuggestion, TopFanout, ValidateResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportGroupBy {
    None,
    File,
    Directory,
    Language,
}

impl ExportGroupBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::File => "file",
            Self::Directory => "directory",
            Self::Language => "language",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: String,
    pub from: Option<String>,
    pub depth: usize,
    pub limit: usize,
    pub group_by: ExportGroupBy,
    pub collapse_tests: bool,
    pub exported_only: bool,
    pub html_out: Option<PathBuf>,
}

impl ExportOptions {
    pub fn new(format: &str, from: Option<&str>, depth: usize, limit: usize) -> Self {
        Self {
            format: format.to_string(),
            from: from.map(ToOwned::to_owned),
            depth,
            limit,
            group_by: ExportGroupBy::None,
            collapse_tests: false,
            exported_only: false,
            html_out: None,
        }
    }
}

// ─── Connection-based public API ─────────────────────────────────────────────
// Every query takes a `&Connection`. Convenience wrappers that open the DB
// from a path live at the bottom for the CLI.

pub fn find_definition_conn(conn: &Connection, symbol: &str) -> Result<DefinitionResult> {
    let mut stmt = conn.prepare(
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
            length(s.qualified_name),
            f.path
        LIMIT 1000
        ",
    )?;
    let mut matches = stmt
        .query_map(params![symbol, format!("%.{}", symbol)], db::map_symbol)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut match_kind = MatchKind::Exact;
    if matches.is_empty() {
        let ranked = ranked_fuzzy_matches(conn, symbol, 5)?;
        if let Some((score, _)) = ranked.first() {
            match_kind = MatchKind::Fuzzy { score: *score };
        }
        matches = ranked.into_iter().map(|(_, s)| s).collect();
    }
    let tokens = estimate_tokens(&matches);
    Ok(DefinitionResult {
        match_kind,
        matches,
        meta: meta(tokens),
    })
}

pub fn get_outline_conn(conn: &Connection, path: &Path) -> Result<OutlineResult> {
    let prefix = path.to_string_lossy().replace('\\', "/");
    let like = if prefix == "." || prefix.is_empty() {
        "%".to_string()
    } else {
        format!("{prefix}%")
    };
    let mut stmt = conn.prepare(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE f.path = ?1 OR f.path LIKE ?2
        ORDER BY f.path, s.start_line
        LIMIT 100000
        ",
    )?;
    let symbols = stmt
        .query_map(params![prefix, like], db::map_symbol)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let tokens = estimate_tokens(&symbols);
    Ok(OutlineResult {
        path: prefix,
        symbols,
        meta: meta(tokens),
    })
}

pub fn validate_conn(conn: &Connection, symbol: &str) -> Result<ValidateResult> {
    let exists = symbol_exists(conn, symbol)?;

    let candidates = if exists {
        Vec::new()
    } else {
        fuzzy_candidates(conn, symbol, 5)?
    };

    let tokens = estimate_tokens(&candidates).max(40);
    Ok(ValidateResult {
        query: symbol.to_string(),
        exists,
        candidates,
        meta: meta(tokens),
    })
}

pub fn stats_conn(conn: &Connection, db_path: &Path) -> Result<StatsResult> {
    let files = db::count_files(conn)?;
    let symbols = db::count_symbols(conn)?;
    let references = db::count_refs(conn)?;
    let edges = db::count_edges(conn)?;

    let mut languages = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY 2 DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(LanguageCount {
                language: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)? as usize,
            })
        })?;
        for row in rows {
            languages.push(row?);
        }
    }

    let mut kinds = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT kind, COUNT(*) FROM symbols GROUP BY kind ORDER BY 2 DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(KindCount {
                kind: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)? as usize,
            })
        })?;
        for row in rows {
            kinds.push(row?);
        }
    }

    let mut top_fanout = Vec::new();
    {
        let mut stmt = conn.prepare(
            "
            SELECT s.qualified_name, COUNT(DISTINCT e.from_symbol_id) AS callers
            FROM edges e
            JOIN symbols s ON s.name = e.to_symbol_name OR s.qualified_name = e.to_symbol_name
            GROUP BY s.qualified_name
            ORDER BY callers DESC
            LIMIT 10
            ",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TopFanout {
                qualified_name: row.get::<_, String>(0)?,
                callers: row.get::<_, i64>(1)? as usize,
            })
        })?;
        for row in rows {
            top_fanout.push(row?);
        }
    }

    let tokens = symbols / 8 + 60;
    Ok(StatsResult {
        files,
        symbols,
        references,
        edges,
        languages,
        kinds,
        top_fanout,
        db_path: db_path.to_string_lossy().to_string(),
        meta: meta(tokens),
    })
}

/// Resolves `rev` to a commit id. Anything git could read as an option is
/// rejected, so a revision can never make `git diff` write files.
fn resolve_commit(root: &Path, rev: &str) -> Result<String> {
    if rev.is_empty() || rev.starts_with('-') {
        anyhow::bail!("invalid git revision {rev:?}");
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{rev}^{{commit}}"))
        .output()
        .map_err(|e| anyhow::anyhow!("git rev-parse failed: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("{rev:?} is not a commit in {}", root.display());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Without `to_ref` the diff runs against the working tree, which is what the
/// index reflects. Paths are relative to `root`, which may sit below the git
/// top level.
pub fn diff_impact_conn(
    conn: &Connection,
    root: &Path,
    from_ref: &str,
    to_ref: Option<&str>,
    depth: usize,
) -> Result<DiffImpactResult> {
    let from = resolve_commit(root, from_ref)?;
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["-c", "core.quotePath=false", "diff", "-U0", "--relative"])
        .args(["--no-color", "--no-ext-diff"])
        .arg(&from);
    if let Some(to) = to_ref {
        command.arg(resolve_commit(root, to)?);
    }
    let to = to_ref.unwrap_or("working tree");
    let out = command
        .output()
        .map_err(|e| anyhow::anyhow!("git diff failed: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git diff exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let diff = String::from_utf8_lossy(&out.stdout);

    // Parse the unified diff: track current file + line ranges of additions
    // and removals. We treat any hunk that touches a symbol's [start, end]
    // range as "this symbol changed".
    #[derive(Default)]
    struct FileHunks {
        path: String,
        ranges: Vec<(usize, usize, usize, usize)>, // (old_start, old_count, new_start, new_count)
    }
    let mut current: Option<FileHunks> = None;
    let mut hunks: Vec<FileHunks> = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            if let Some(cur) = current.take() {
                hunks.push(cur);
            }
            let path = rest
                .trim_start_matches("b/")
                .trim_start_matches("a/")
                .trim()
                .to_string();
            if path != "/dev/null" {
                current = Some(FileHunks {
                    path,
                    ranges: Vec::new(),
                });
            }
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            // @@ -old_start,old_count +new_start,new_count @@
            if let Some(rest) = hunk.split(" @@").next() {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    let old = parse_range(parts[0].trim_start_matches('-'));
                    let new = parse_range(parts[1].trim_start_matches('+'));
                    if let (Some(o), Some(n)) = (old, new) {
                        if let Some(cur) = current.as_mut() {
                            cur.ranges.push((o.0, o.1, n.0, n.1));
                        }
                    }
                }
            }
        }
    }
    if let Some(cur) = current.take() {
        hunks.push(cur);
    }

    let changed_files = hunks.len();
    let mut changed_symbols: Vec<DiffChangedSymbol> = Vec::new();

    for fh in &hunks {
        // Resolve file_id by path (path is repo-relative).
        let Some((file_id, _)) = db::file_sha(conn, &fh.path)? else {
            continue;
        };
        for (_, _, new_start, new_count) in &fh.ranges {
            let hunk_start = *new_start;
            let hunk_end = new_start + new_count.saturating_sub(1);
            let mut stmt = conn.prepare(
                "
                SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                       s.start_line, s.end_line, s.signature, s.exported
                FROM symbols s
                JOIN files f ON f.id = s.file_id
                WHERE s.file_id = ?1
                  AND s.start_line <= ?2
                  AND s.end_line >= ?3
                ",
            )?;
            let rows = stmt.query_map(
                params![file_id, hunk_end as i64, hunk_start as i64],
                db::map_symbol,
            )?;
            for row in rows {
                let sym = row?;
                if let Some(existing) = changed_symbols.iter_mut().find(|c| c.symbol.id == sym.id) {
                    existing.added_lines += *new_count;
                } else {
                    changed_symbols.push(DiffChangedSymbol {
                        symbol: sym,
                        added_lines: *new_count,
                        removed_lines: 0,
                    });
                }
            }
        }
    }

    // Everything that reaches a changed symbol through resolved edges.
    let seeds: Vec<i64> = changed_symbols.iter().map(|c| c.symbol.id).collect();
    let (reached, _) = if depth == 0 {
        (Vec::new(), false)
    } else {
        super::graph::blast(conn, &seeds, depth)?
    };
    let mut impacted: Vec<DiffImpactedSymbol> = reached
        .into_iter()
        .map(|r| DiffImpactedSymbol {
            via: changed_symbols
                .iter()
                .find(|c| c.symbol.id == r.via)
                .map(|c| c.symbol.qualified_name.clone())
                .unwrap_or_default(),
            symbol: r.symbol,
            criticality: r.criticality,
        })
        .collect();
    impacted.sort_by(|a, b| {
        b.criticality
            .partial_cmp(&a.criticality)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.path.cmp(&b.symbol.path))
            .then_with(|| a.symbol.start_line.cmp(&b.symbol.start_line))
            .then_with(|| a.symbol.qualified_name.cmp(&b.symbol.qualified_name))
    });

    let tokens = estimate_tokens(&changed_symbols) + estimate_tokens(&impacted);
    Ok(DiffImpactResult {
        from_ref: from_ref.to_string(),
        to_ref: to.to_string(),
        changed_files,
        changed_symbols,
        impacted,
        meta: meta(tokens),
    })
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let mut parts = s.split(',');
    let start: usize = parts.next()?.parse().ok()?;
    let count: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    Some((start, count))
}

pub fn imports_conn(conn: &Connection, path: &str) -> Result<ImportsResult> {
    let mut stmt = conn.prepare(
        "
        SELECT i.source, f.path, i.line, i.kind
        FROM imports i
        JOIN files f ON f.id = i.file_id
        WHERE f.path = ?1 OR f.path LIKE ?2
        ORDER BY f.path, i.line
        LIMIT 100000
        ",
    )?;
    let like = if path.ends_with('/') || path.ends_with("**") {
        format!("{}%", path.trim_end_matches("**"))
    } else {
        format!("{}%", path)
    };
    let imports: Vec<ImportRecord> = stmt
        .query_map(params![path, like], |row| {
            Ok(ImportRecord {
                source: row.get(0)?,
                from_path: row.get(1)?,
                line: row.get::<_, i64>(2)? as usize,
                kind: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let tokens = estimate_tokens(&imports).max(40);
    Ok(ImportsResult {
        path: path.to_string(),
        imports,
        meta: meta(tokens),
    })
}

pub fn signature_conn(conn: &Connection, symbol: &str) -> Result<SignatureResult> {
    let Some(target) = db::resolve_symbol(conn, symbol)? else {
        return Ok(SignatureResult {
            symbol: None,
            members: Vec::new(),
            meta: meta(30),
        });
    };
    // For container kinds, list child symbols defined inside this symbol's
    // line range — same file, nested. Cheap; no body included.
    let container = matches!(
        target.kind.as_str(),
        "class" | "struct" | "interface" | "trait" | "enum" | "record" | "impl" | "module"
    );
    let mut members: Vec<SignatureLine> = Vec::new();
    if container {
        let mut stmt = conn.prepare(
            "
            SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                   s.start_line, s.end_line, s.signature, s.exported
            FROM symbols s
            JOIN files f ON f.id = s.file_id
            WHERE s.file_id = ?1
              AND s.id != ?2
              AND s.start_line > ?3
              AND s.end_line <= ?4
            ORDER BY s.start_line
            LIMIT 200
            ",
        )?;
        let rows = stmt.query_map(
            params![
                target.file_id,
                target.id,
                target.start_line as i64,
                target.end_line as i64
            ],
            db::map_symbol,
        )?;
        for row in rows {
            let sym = row?;
            members.push(SignatureLine {
                qualified_name: sym.qualified_name,
                kind: sym.kind,
                path: sym.path,
                line: sym.start_line,
                signature: sym.signature,
            });
        }
    }
    let tokens = estimate_tokens(&members).max(50);
    Ok(SignatureResult {
        symbol: Some(target),
        members,
        meta: meta(tokens),
    })
}

pub fn siblings_conn(conn: &Connection, symbol: &str) -> Result<SiblingsResult> {
    // Symbols that share callers with `symbol`. The SQL self-join on `edges`
    // finds, for each from_symbol that calls our target, the OTHER things it
    // also calls — then ranks them by how many distinct callers they share.
    let mut stmt = conn.prepare(
        "
        SELECT e2.to_symbol_name, COUNT(DISTINCT e1.from_symbol_id) AS shared
        FROM edges e1
        JOIN edges e2 ON e2.from_symbol_id = e1.from_symbol_id
        WHERE (e1.to_symbol_name = ?1 OR e1.to_symbol_name = ?2)
          AND e2.to_symbol_name != ?1
          AND e2.to_symbol_name != ?2
        GROUP BY e2.to_symbol_name
        HAVING shared > 0
        ORDER BY shared DESC, e2.to_symbol_name
        LIMIT 50
        ",
    )?;
    let target = db::resolve_symbol(conn, symbol)?;
    let qualified = target
        .as_ref()
        .map(|t| t.qualified_name.clone())
        .unwrap_or_default();
    let name = target
        .as_ref()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| symbol.to_string());

    let rows = stmt.query_map(params![name, qualified], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
    })?;

    let mut siblings: Vec<Sibling> = Vec::new();
    for row in rows {
        let (sibling_name, shared) = row?;
        let resolved = db::resolve_symbol(conn, &sibling_name).ok().flatten();
        siblings.push(Sibling {
            qualified_name: resolved
                .as_ref()
                .map(|s| s.qualified_name.clone())
                .unwrap_or(sibling_name),
            shared_callers: shared,
            path: resolved.as_ref().map(|s| s.path.clone()),
            line: resolved.as_ref().map(|s| s.start_line),
        });
    }

    let tokens = estimate_tokens(&siblings).max(60);
    Ok(SiblingsResult {
        symbol: symbol.to_string(),
        siblings,
        meta: meta(tokens),
    })
}

pub const SEARCH_SCAN_LIMIT: usize = 500;

pub fn search_conn(
    conn: &Connection,
    pattern: &str,
    options: SearchOptions,
) -> Result<SearchResult> {
    let limit = options.limit.clamp(1, 500);

    // Two query modes:
    //   - if the pattern has `*`, treat it as a glob → SQL LIKE
    //   - otherwise fuzzy via FTS5 trigram + Jaro-Winkler
    let candidates: Vec<SymbolRecord> = if pattern.contains('*') {
        glob_symbol_matches(conn, pattern, limit.saturating_mul(4).max(50))?
    } else if pattern.is_empty() {
        list_symbols(conn, limit.saturating_mul(4).max(50))?
    } else {
        fuzzy_symbol_matches(conn, pattern, limit.saturating_mul(4).max(50))?
    };

    let mut hits: Vec<SearchHit> = candidates
        .into_iter()
        .filter(|s| options.kinds.is_empty() || options.kinds.contains(&s.kind))
        .filter(|s| options.languages.is_empty() || options.languages.contains(&s.language))
        .filter(|s| options.exported.is_none_or(|e| s.exported == e))
        .filter(|s| {
            options
                .path_prefix
                .as_deref()
                .is_none_or(|prefix| s.path.starts_with(prefix))
        })
        .map(|s| {
            let score = if pattern.is_empty() {
                0.0
            } else if pattern.contains('*') {
                glob_score(pattern, &s.name).max(glob_score(pattern, &s.qualified_name))
            } else {
                jaro_winkler(pattern, &s.qualified_name).max(jaro_winkler(pattern, &s.name)) as f32
            };
            SearchHit { symbol: s, score }
        })
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.qualified_name.cmp(&b.symbol.qualified_name))
            .then_with(|| a.symbol.path.cmp(&b.symbol.path))
            .then_with(|| a.symbol.start_line.cmp(&b.symbol.start_line))
    });
    hits.truncate(limit);

    let tokens = estimate_tokens(&hits).max(40);
    Ok(SearchResult {
        query: pattern.to_string(),
        hits,
        meta: meta(tokens),
    })
}

pub fn connect_conn(
    conn: &Connection,
    from: &str,
    to: &str,
    max_depth: usize,
) -> Result<ConnectResult> {
    let max_depth = max_depth.clamp(1, 12);

    let to_node = |s: &SymbolRecord| PathNode {
        qualified_name: s.qualified_name.clone(),
        kind: s.kind.clone(),
        path: s.path.clone(),
        line: s.start_line,
    };

    let starts = exact_symbols(conn, from, 25)?;
    let targets = exact_symbols(conn, to, 50)?;
    if starts.is_empty() || targets.is_empty() {
        return Ok(ConnectResult {
            from: from.to_string(),
            to: to.to_string(),
            found: false,
            path: Vec::new(),
            meta: meta(40),
        });
    }
    let target_ids: HashSet<i64> = targets.iter().map(|s| s.id).collect();

    // Trivial: `from` already resolves to a target symbol.
    if let Some(s) = starts.iter().find(|s| target_ids.contains(&s.id)) {
        return Ok(ConnectResult {
            from: from.to_string(),
            to: to.to_string(),
            found: true,
            path: vec![to_node(s)],
            meta: meta(40),
        });
    }

    // BFS forward over the call graph (caller -> callee), tracking a predecessor
    // by symbol id so the path can be reconstructed. `prev` absent ⇒ a start node.
    const MAX_VISITED: usize = 5000;
    let mut visited: HashSet<i64> = HashSet::new();
    let mut prev: HashMap<i64, i64> = HashMap::new();
    let mut record: HashMap<i64, SymbolRecord> = HashMap::new();
    let mut queue: VecDeque<(SymbolRecord, usize)> = VecDeque::new();

    for s in &starts {
        if visited.insert(s.id) {
            record.insert(s.id, s.clone());
            queue.push_back((s.clone(), 0));
        }
    }

    // Contracts add hops across services: a client to the server's handler.
    let links = super::map::links(conn)?;
    let mut hit: Option<i64> = None;
    'bfs: while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let mut callees = callees_for_symbol(conn, &node, 200)?;
        for id in links.get(&node.id).into_iter().flatten() {
            if let Some(target) = super::graph::symbol_by_id(conn, *id)? {
                callees.push(target);
            }
        }
        for callee in callees {
            if !visited.insert(callee.id) {
                continue;
            }
            prev.insert(callee.id, node.id);
            record.insert(callee.id, callee.clone());
            if target_ids.contains(&callee.id) {
                hit = Some(callee.id);
                break 'bfs;
            }
            queue.push_back((callee.clone(), depth + 1));
            if visited.len() >= MAX_VISITED {
                break 'bfs;
            }
        }
    }

    let mut path_nodes = Vec::new();
    if let Some(target_id) = hit {
        let mut chain = vec![target_id];
        let mut cur = target_id;
        while let Some(&p) = prev.get(&cur) {
            chain.push(p);
            cur = p;
        }
        chain.reverse();
        for id in chain {
            if let Some(rec) = record.get(&id) {
                path_nodes.push(to_node(rec));
            }
        }
    }

    let tokens = estimate_tokens(&path_nodes).max(40);
    Ok(ConnectResult {
        from: from.to_string(),
        to: to.to_string(),
        found: hit.is_some(),
        path: path_nodes,
        meta: meta(tokens),
    })
}

pub fn export_conn_with_options(conn: &Connection, options: ExportOptions) -> Result<ExportResult> {
    let fmt = if options.format.eq_ignore_ascii_case("dot") {
        "dot"
    } else {
        "mermaid"
    };
    let limit = options.limit.clamp(1, 5000);
    let depth = options.depth.clamp(1, 12);
    let mut edges: Vec<(String, String)> = Vec::new();
    let mut node_meta: HashMap<String, GraphNode> = HashMap::new();
    let mut truncated = false;
    let scope;

    if let Some(sym) = options.from.as_deref() {
        scope = format!("from:{sym}");
        let starts = exact_symbols(conn, sym, 25)?;
        let mut visited: HashSet<i64> = HashSet::new();
        let mut queue: VecDeque<(SymbolRecord, usize)> = VecDeque::new();
        for s in &starts {
            insert_graph_node(&mut node_meta, s);
            if visited.insert(s.id) {
                queue.push_back((s.clone(), 0));
            }
        }
        'bfs: while let Some((node, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            for callee in callees_for_symbol(conn, &node, 200)? {
                insert_graph_node(&mut node_meta, &node);
                insert_graph_node(&mut node_meta, &callee);
                if !include_graph_node(&node, &options) || !include_graph_node(&callee, &options) {
                    continue;
                }
                edges.push((node.qualified_name.clone(), callee.qualified_name.clone()));
                if edges.len() >= limit {
                    truncated = true;
                    break 'bfs;
                }
                if visited.insert(callee.id) {
                    queue.push_back((callee, d + 1));
                }
            }
        }
    } else {
        scope = "all".to_string();
        let mut stmt = conn.prepare(
            "
            SELECT DISTINCT
                s1.qualified_name, f1.path, f1.language, s1.exported,
                s2.qualified_name, f2.path, f2.language, s2.exported
            FROM edges e
            JOIN symbols s1 ON s1.id = e.from_symbol_id
            JOIN files f1 ON f1.id = s1.file_id
            JOIN symbols s2 ON (s2.name = e.to_symbol_name OR s2.qualified_name = e.to_symbol_name)
            JOIN files f2 ON f2.id = s2.file_id
            WHERE s1.id != s2.id
            ORDER BY s1.qualified_name, s2.qualified_name
            LIMIT ?1
            ",
        )?;
        let rows = stmt.query_map(params![(limit + 1) as i64], |row| {
            Ok((
                GraphNode {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    language: row.get(2)?,
                    exported: row.get::<_, i64>(3)? != 0,
                },
                GraphNode {
                    name: row.get(4)?,
                    path: row.get(5)?,
                    language: row.get(6)?,
                    exported: row.get::<_, i64>(7)? != 0,
                },
            ))
        })?;
        for row in rows {
            let (from_node, to_node) = row?;
            node_meta.insert(from_node.name.clone(), from_node.clone());
            node_meta.insert(to_node.name.clone(), to_node.clone());
            if !include_graph_node_raw(&from_node, &options)
                || !include_graph_node_raw(&to_node, &options)
            {
                continue;
            }
            edges.push((from_node.name, to_node.name));
        }
        if edges.len() > limit {
            truncated = true;
            edges.truncate(limit);
        }
    }

    edges.sort();
    edges.dedup();

    // Node set in deterministic order.
    let mut nodes: Vec<String> = Vec::new();
    {
        let mut seen: HashSet<String> = HashSet::new();
        for (a, b) in &edges {
            for n in [a, b] {
                if seen.insert(n.clone()) {
                    nodes.push(n.clone());
                }
            }
        }
        nodes.sort();
    }

    let diagram = if fmt == "dot" {
        render_dot(&edges, &node_meta, options.group_by)
    } else {
        render_mermaid(&nodes, &edges, &node_meta, options.group_by)
    };
    let html_path = if let Some(path) = options.html_out.as_ref() {
        let preview_diagram = if fmt == "mermaid" {
            diagram.clone()
        } else {
            render_mermaid(&nodes, &edges, &node_meta, options.group_by)
        };
        fs::write(
            path,
            render_html_preview(&preview_diagram, &scope, options.group_by),
        )?;
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    let tokens = estimate_tokens(&diagram).max(40);
    Ok(ExportResult {
        format: fmt.to_string(),
        scope,
        group_by: options.group_by.as_str().to_string(),
        nodes: nodes.len(),
        edges: edges.len(),
        truncated,
        diagram,
        html_path,
        meta: meta(tokens),
    })
}

// ─── Path-based wrappers used by the CLI ─────────────────────────────────────

/// Symbols that `caller` calls — the forward direction of the call graph (the
/// inverse of `callers_for_symbol`). Used by `connect` / `export`.
///
/// Each distinct callee *name* is resolved to a single best symbol, preferring
/// a definition in the caller's own file. Without this, a call to a common name
/// like `loadUser` would fan out to every `loadUser` across the index and the
/// forward subgraph would explode across unrelated files — defeating the point
/// of a precise graph.
fn callees_for_symbol(
    conn: &Connection,
    caller: &SymbolRecord,
    limit: usize,
) -> Result<Vec<SymbolRecord>> {
    let mut name_stmt = conn
        .prepare("SELECT DISTINCT to_symbol_name FROM edges WHERE from_symbol_id = ?1 LIMIT ?2")?;
    let names = name_stmt
        .query_map(params![caller.id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    for name in names {
        if let Some(sym) = resolve_callee(conn, &name, caller.file_id)? {
            if sym.id != caller.id && seen.insert(sym.id) {
                out.push(sym);
            }
        }
    }
    Ok(out)
}

/// Resolve a referenced name to a single best-matching symbol, preferring a
/// definition in `prefer_file_id`, then exact-qualified, then exact-name.
fn resolve_callee(
    conn: &Connection,
    name: &str,
    prefer_file_id: i64,
) -> Result<Option<SymbolRecord>> {
    conn.query_row(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE s.qualified_name = ?1 OR s.name = ?1
        ORDER BY
            CASE WHEN s.file_id = ?2 THEN 0 ELSE 1 END,
            CASE WHEN s.qualified_name = ?1 THEN 0 ELSE 1 END,
            length(s.qualified_name),
            f.path
        LIMIT 1
        ",
        params![name, prefer_file_id],
        db::map_symbol,
    )
    .optional()
    .map_err(Into::into)
}

/// Exact, case-sensitive symbol matches by name or qualified name. Unlike
/// `find_definition_conn` this never falls back to fuzzy matching, and unlike a
/// `LIKE` clause it is case-sensitive — `connect` and `export` need precise
/// endpoints. (SQLite `LIKE` is case-insensitive by default, which would cross
/// `FindByID` with `findById`; `=` uses the BINARY collation, which does not.)
fn exact_symbols(conn: &Connection, symbol: &str, limit: usize) -> Result<Vec<SymbolRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE s.qualified_name = ?1 OR s.name = ?1
        ORDER BY
            CASE WHEN s.qualified_name = ?1 THEN 0 ELSE 1 END,
            length(s.qualified_name),
            f.path
        LIMIT ?2
        ",
    )?;
    let rows = stmt.query_map(params![symbol, limit as i64], db::map_symbol)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug, Clone)]
struct GraphNode {
    name: String,
    path: String,
    language: String,
    exported: bool,
}

fn insert_graph_node(nodes: &mut HashMap<String, GraphNode>, symbol: &SymbolRecord) {
    nodes.insert(
        symbol.qualified_name.clone(),
        GraphNode {
            name: symbol.qualified_name.clone(),
            path: symbol.path.clone(),
            language: symbol.language.clone(),
            exported: symbol.exported,
        },
    );
}

fn include_graph_node(symbol: &SymbolRecord, options: &ExportOptions) -> bool {
    if options.exported_only && !symbol.exported {
        return false;
    }
    if options.collapse_tests && is_test_path(&symbol.path) {
        return false;
    }
    true
}

fn include_graph_node_raw(node: &GraphNode, options: &ExportOptions) -> bool {
    if options.exported_only && !node.exported {
        return false;
    }
    if options.collapse_tests && is_test_path(&node.path) {
        return false;
    }
    true
}

fn graph_group(node: Option<&GraphNode>, group_by: ExportGroupBy) -> String {
    let Some(node) = node else {
        return "unknown".to_string();
    };
    match group_by {
        ExportGroupBy::None => "graph".to_string(),
        ExportGroupBy::File => node.path.clone(),
        ExportGroupBy::Directory => node
            .path
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_else(|| ".".to_string()),
        ExportGroupBy::Language => node.language.clone(),
    }
}

fn render_dot(
    edges: &[(String, String)],
    node_meta: &HashMap<String, GraphNode>,
    group_by: ExportGroupBy,
) -> String {
    let mut out = String::from(
        "digraph tessera {\n  rankdir=LR;\n  node [shape=box, fontname=\"monospace\"];\n",
    );
    if group_by != ExportGroupBy::None {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for name in dot_nodes(edges) {
            groups
                .entry(graph_group(node_meta.get(&name), group_by))
                .or_default()
                .push(name);
        }
        for (idx, (group, mut nodes)) in groups.into_iter().enumerate() {
            nodes.sort();
            nodes.dedup();
            out.push_str(&format!(
                "  subgraph cluster_{idx} {{\n    label={};\n",
                dot_quote(&group)
            ));
            for node in nodes {
                out.push_str(&format!("    {};\n", dot_quote(&node)));
            }
            out.push_str("  }\n");
        }
    }
    for (a, b) in edges {
        out.push_str(&format!("  {} -> {};\n", dot_quote(a), dot_quote(b)));
    }
    out.push_str("}\n");
    out
}

fn dot_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn dot_nodes(edges: &[(String, String)]) -> Vec<String> {
    let mut nodes = Vec::new();
    for (a, b) in edges {
        nodes.push(a.clone());
        nodes.push(b.clone());
    }
    nodes.sort();
    nodes.dedup();
    nodes
}

fn render_mermaid(
    nodes: &[String],
    edges: &[(String, String)],
    node_meta: &HashMap<String, GraphNode>,
    group_by: ExportGroupBy,
) -> String {
    let id_of: HashMap<&String, usize> = nodes.iter().enumerate().map(|(i, n)| (n, i)).collect();
    let mut out = String::from("graph TD\n");
    if group_by == ExportGroupBy::None {
        for (i, n) in nodes.iter().enumerate() {
            out.push_str(&format!("  n{}[\"{}\"]\n", i, mermaid_label(n)));
        }
    } else {
        let mut groups: BTreeMap<String, Vec<&String>> = BTreeMap::new();
        for n in nodes {
            groups
                .entry(graph_group(node_meta.get(n), group_by))
                .or_default()
                .push(n);
        }
        for (group, members) in groups {
            out.push_str(&format!("  subgraph \"{}\"\n", mermaid_label(&group)));
            for n in members {
                if let Some(&i) = id_of.get(n) {
                    out.push_str(&format!("    n{}[\"{}\"]\n", i, mermaid_label(n)));
                }
            }
            out.push_str("  end\n");
        }
    }
    for (a, b) in edges {
        if let (Some(&ai), Some(&bi)) = (id_of.get(a), id_of.get(b)) {
            out.push_str(&format!("  n{ai} --> n{bi}\n"));
        }
    }
    out
}

fn mermaid_label(s: &str) -> String {
    // Mermaid node labels are quoted; quotes and brackets break the parser.
    s.replace('"', "'").replace('[', "(").replace(']', ")")
}

fn render_html_preview(diagram: &str, scope: &str, group_by: ExportGroupBy) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Tessera Graph Preview</title>
  <script type="module">
    import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs";
    mermaid.initialize({{ startOnLoad: true, theme: "default" }});
  </script>
  <style>
    body {{ margin: 0; font: 14px system-ui, sans-serif; color: #17202a; background: #f7f8fa; }}
    header {{ display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 18px; background: white; border-bottom: 1px solid #d9dee7; }}
    main {{ padding: 18px; }}
    button {{ border: 1px solid #9aa7b7; background: #fff; border-radius: 6px; padding: 8px 12px; cursor: pointer; }}
    .meta {{ color: #5e6b7a; }}
    .mermaid {{ background: white; border: 1px solid #d9dee7; border-radius: 8px; padding: 18px; overflow: auto; }}
    pre {{ position: absolute; left: -9999px; }}
  </style>
</head>
<body>
  <header>
    <div>
      <strong>Tessera Graph Preview</strong>
      <div class="meta">scope: {scope} · grouped by: {group_by}</div>
    </div>
    <button type="button" onclick="navigator.clipboard.writeText(document.getElementById('diagram').textContent)">Copy Mermaid</button>
  </header>
  <main>
    <pre id="diagram">{diagram}</pre>
    <div class="mermaid">{diagram}</div>
  </main>
</body>
</html>
"#,
        scope = html_escape(scope),
        group_by = group_by.as_str(),
        diagram = html_escape(diagram)
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn symbol_exists(conn: &Connection, symbol: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "
        SELECT COUNT(*) FROM symbols
        WHERE qualified_name = ?1 OR name = ?1 OR qualified_name LIKE ?2
        ",
        params![symbol, format!("%.{}", symbol)],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn fuzzy_candidates(
    conn: &Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<SymbolSuggestion>> {
    let matches = fuzzy_symbol_matches(conn, symbol, limit)?;
    let mut suggestions: Vec<SymbolSuggestion> = matches
        .into_iter()
        .map(|s| {
            let by_qualified = jaro_winkler(symbol, &s.qualified_name) as f32;
            let by_name = jaro_winkler(symbol, &s.name) as f32;
            let confidence = by_qualified.max(by_name);
            SymbolSuggestion {
                qualified_name: s.qualified_name,
                name: s.name,
                path: s.path,
                line: s.start_line,
                confidence,
            }
        })
        .collect();
    suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

fn fuzzy_symbol_matches(
    conn: &Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<SymbolRecord>> {
    Ok(ranked_fuzzy_matches(conn, symbol, limit)?
        .into_iter()
        .map(|(_, s)| s)
        .collect())
}

fn ranked_fuzzy_matches(
    conn: &Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<(f32, SymbolRecord)>> {
    // First try FTS5 (cheap, indexed). If unavailable or no hits, fall back to
    // scanning all symbol names and ranking by Jaro-Winkler.
    let escaped = escape_fts_term(symbol);
    let fts_query = format!("{}*", escaped);
    let fts_attempt: rusqlite::Result<Vec<SymbolRecord>> = (|| {
        let mut stmt = conn.prepare(
            "
            SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                   s.start_line, s.end_line, s.signature, s.exported
            FROM symbols_fts fts
            JOIN symbols s ON s.id = fts.rowid
            JOIN files f ON f.id = s.file_id
            WHERE symbols_fts MATCH ?1
            LIMIT ?2
            ",
        )?;
        let rows = stmt.query_map(params![fts_query, (limit * 4) as i64], db::map_symbol)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })();

    let mut candidates = fts_attempt.unwrap_or_default();

    if candidates.is_empty() {
        let mut stmt = conn.prepare(
            "
            SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                   s.start_line, s.end_line, s.signature, s.exported
            FROM symbols s
            JOIN files f ON f.id = s.file_id
            LIMIT 5000
            ",
        )?;
        let rows = stmt.query_map([], db::map_symbol)?;
        for row in rows {
            candidates.push(row?);
        }
    }

    let mut ranked: Vec<(f32, SymbolRecord)> = candidates
        .into_iter()
        .map(|s| {
            let score =
                jaro_winkler(symbol, &s.qualified_name).max(jaro_winkler(symbol, &s.name)) as f32;
            (score, s)
        })
        .filter(|(score, _)| *score >= 0.6)
        .collect();
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.path.cmp(&b.1.path))
            .then_with(|| a.1.start_line.cmp(&b.1.start_line))
            .then_with(|| a.1.qualified_name.cmp(&b.1.qualified_name))
    });
    ranked.truncate(limit);
    Ok(ranked)
}

fn escape_fts_term(term: &str) -> String {
    term.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

fn glob_symbol_matches(
    conn: &Connection,
    pattern: &str,
    limit: usize,
) -> Result<Vec<SymbolRecord>> {
    // Translate `*` → SQL `%`; escape SQL wildcards in the literal part.
    let like = pattern
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .replace('*', "%");
    let mut stmt = conn.prepare(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE s.name LIKE ?1 ESCAPE '\\' OR s.qualified_name LIKE ?1 ESCAPE '\\'
        ORDER BY length(s.qualified_name), s.qualified_name
        LIMIT ?2
        ",
    )?;
    let rows = stmt.query_map(params![like, limit as i64], db::map_symbol)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn list_symbols(conn: &Connection, limit: usize) -> Result<Vec<SymbolRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        ORDER BY s.qualified_name
        LIMIT ?1
        ",
    )?;
    let rows = stmt.query_map(params![limit as i64], db::map_symbol)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn glob_score(pattern: &str, name: &str) -> f32 {
    // Convert pattern to a simple SQL LIKE → boolean match; assign a heuristic
    // score so shorter qualified names sort first within a glob hit set.
    let lp = pattern.to_lowercase();
    let ln = name.to_lowercase();
    let head = lp.trim_start_matches('*');
    let tail = head.trim_end_matches('*');
    let stripped = tail.trim_matches('*');
    if stripped.is_empty() {
        return 0.5;
    }
    if ln == stripped {
        return 1.0;
    }
    if ln.contains(stripped) {
        return 0.85;
    }
    0.5
}

const TEST_DIRS: &[&str] = &["test", "tests", "__tests__", "spec", "specs", "e2e"];

pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let mut segments: Vec<&str> = lower.split('/').collect();
    let file = segments.pop().unwrap_or_default();
    if segments.iter().any(|segment| TEST_DIRS.contains(segment)) {
        return true;
    }
    let stem = file.split('.').next().unwrap_or_default();
    let java_stem = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".java"))
        .unwrap_or_default();
    file.contains(".test.")
        || file.contains(".spec.")
        || stem.ends_with("_test")
        || stem.ends_with("_spec")
        || (file.ends_with(".py") && stem.starts_with("test_"))
        || ["Test", "Tests", "IT"]
            .iter()
            .any(|suffix| java_stem.ends_with(suffix))
}

fn estimate_tokens<T: serde::Serialize>(value: &T) -> usize {
    let bytes = serde_json::to_vec(value)
        .map(|json| json.len())
        .unwrap_or(0);
    (bytes / 4).max(1)
}

fn meta(tokens: usize) -> QueryMeta {
    QueryMeta { tokens }
}

/// The diff's label and the indexed symbols whose lines it touches.
pub(crate) fn diff_changed_symbols(
    conn: &Connection,
    root: &Path,
    from_ref: &str,
    to_ref: Option<&str>,
) -> Result<(String, Vec<SymbolRecord>)> {
    let diff = diff_impact_conn(conn, root, from_ref, to_ref, 0)?;
    Ok((
        format!("{}..{}", diff.from_ref, diff.to_ref),
        diff.changed_symbols.into_iter().map(|c| c.symbol).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_rules() {
        for path in [
            "tests/a.rs",
            "src/__tests__/a.ts",
            "a.test.ts",
            "a.spec.js",
            "specs/util/util.spec.js",
            "pkg/a_test.go",
            "test_a.py",
            "spec/models/user_spec.rb",
            "src/test/java/FooTest.java",
            "src/test/java/FooIT.java",
            "src/main/java/FooTests.java",
        ] {
            assert!(is_test_path(path), "{path} should be a test path");
        }
        for path in [
            "app/controllers/api/v1/test_cases_controller.rb",
            "src/latest.ts",
            "contest/a.py",
            "src/main/java/FooUnit.java",
            "src/main/java/Edit.java",
        ] {
            assert!(!is_test_path(path), "{path} should not be a test path");
        }
    }
}
