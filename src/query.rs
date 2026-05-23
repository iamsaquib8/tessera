use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection};
use strsim::jaro_winkler;

use crate::bloom::BloomFilter;
use crate::db;
use crate::indexer;
use crate::types::{
    AlternativeQuery, ContextPack, CriticalityBreakdown, DefinitionResult, DiffChangedSymbol,
    DiffImpactResult, DiffImpactedSymbol, ExpandResult, ImpactCaller, ImpactResult, ImportRecord,
    ImportedByResult, ImportsResult, KindCount, Language, LanguageCount, OutlineResult, QueryMeta,
    ReferenceRecord, ReferencesResult, SearchHit, SearchOptions, SearchResult, Sibling,
    SiblingsResult, SignatureLine, SignatureResult, SnippetReferenceCheck, StatsResult,
    SymbolRecord, SymbolSuggestion, TestsForResult, TopFanout, ValidateResult,
    ValidateSnippetResult,
};

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
        LIMIT 25
        ",
    )?;
    let mut matches = stmt
        .query_map(params![symbol, format!("%.{}", symbol)], db::map_symbol)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if matches.is_empty() {
        matches = fuzzy_symbol_matches(conn, symbol, 5)?;
    }
    let tokens = estimate_tokens(&matches);
    Ok(DefinitionResult {
        matches,
        meta: meta(tokens, "get_outline", 320, 0.72),
    })
}

pub fn find_references_conn(conn: &Connection, symbol: &str) -> Result<ReferencesResult> {
    let refs = references_for_symbol(conn, symbol, 250)?;
    let tokens = estimate_tokens(&refs);
    Ok(ReferencesResult {
        references: refs,
        meta: meta(tokens, "impact", 900, 0.84),
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
        LIMIT 1000
        ",
    )?;
    let symbols = stmt
        .query_map(params![prefix, like], db::map_symbol)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let tokens = estimate_tokens(&symbols);
    Ok(OutlineResult {
        path: prefix,
        symbols,
        meta: meta(tokens, "expand_symbol", 1200, 0.9),
    })
}

pub fn expand_symbol_conn(conn: &Connection, symbol: &str) -> Result<ExpandResult> {
    let Some(symbol_record) = db::resolve_symbol(conn, symbol)? else {
        return Ok(ExpandResult {
            symbol: None,
            body: None,
            dependencies: Vec::new(),
            meta: meta(20, "find_definition", 120, 0.65),
        });
    };
    let body = read_symbol_body(conn, &symbol_record).ok();
    let dependencies = references_from_symbol(conn, symbol_record.id, 100)?;
    let tokens = estimate_tokens(&body) + estimate_tokens(&dependencies);
    Ok(ExpandResult {
        symbol: Some(symbol_record),
        body,
        dependencies,
        meta: meta(tokens, "get_outline", 320, 0.7),
    })
}

pub fn impact_conn(conn: &Connection, symbol: &str, depth: usize) -> Result<ImpactResult> {
    let target = db::resolve_symbol(conn, symbol)?;
    let target_id = target.as_ref().map(|t| t.id);

    // BFS collect: caller_id -> (min_depth, SymbolRecord).
    // We dedupe `(name, min_depth)` queue entries by `name` so that high
    // fan-in symbols (e.g. Java's pervasive `.get()` methods) don't re-query
    // the same callers thousands of times. Also cap the total frontier size
    // so PageRank stays well-defined and fast on hub symbols.
    const MAX_FRONTIER: usize = 800;
    let mut frontier: HashMap<i64, (usize, SymbolRecord)> = HashMap::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut queried_names: HashSet<String> = HashSet::new();

    let seed_name = |name: String, queue: &mut VecDeque<(String, usize)>| {
        if name.is_empty() {
            return;
        }
        queue.push_back((name, 1));
    };
    seed_name(symbol.to_string(), &mut queue);
    if let Some(t) = &target {
        if t.name != symbol {
            seed_name(t.name.clone(), &mut queue);
        }
        if t.qualified_name != symbol && t.qualified_name != t.name {
            seed_name(t.qualified_name.clone(), &mut queue);
        }
    }

    'outer: while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth > depth {
            continue;
        }
        if !queried_names.insert(current.clone()) {
            continue;
        }
        for caller in callers_for_symbol(conn, &current, 500)? {
            let entry = frontier
                .entry(caller.id)
                .or_insert_with(|| (current_depth, caller.clone()));
            if entry.0 > current_depth {
                entry.0 = current_depth;
            }
            if current_depth < depth {
                if !queried_names.contains(&caller.name) {
                    queue.push_back((caller.name.clone(), current_depth + 1));
                }
                if caller.qualified_name != caller.name
                    && !queried_names.contains(&caller.qualified_name)
                {
                    queue.push_back((caller.qualified_name.clone(), current_depth + 1));
                }
            }
            if frontier.len() >= MAX_FRONTIER {
                break 'outer;
            }
        }
    }

    if frontier.is_empty() {
        let tokens = 40;
        return Ok(ImpactResult {
            symbol: symbol.to_string(),
            callers: Vec::new(),
            meta: meta(tokens, "find_references", 700, 0.78),
        });
    }

    // Build node set: target (if known) + all callers in frontier.
    // Then build the walk graph (reverse of call graph): callee -> caller.
    let mut name_to_ids: HashMap<String, Vec<i64>> = HashMap::new();
    if let Some(t) = &target {
        name_to_ids.entry(t.name.clone()).or_default().push(t.id);
        if t.qualified_name != t.name {
            name_to_ids
                .entry(t.qualified_name.clone())
                .or_default()
                .push(t.id);
        }
    }
    for (id, (_, record)) in &frontier {
        name_to_ids
            .entry(record.name.clone())
            .or_default()
            .push(*id);
        if record.qualified_name != record.name {
            name_to_ids
                .entry(record.qualified_name.clone())
                .or_default()
                .push(*id);
        }
    }

    let mut node_ids: Vec<i64> = frontier.keys().copied().collect();
    if let Some(tid) = target_id {
        if !frontier.contains_key(&tid) {
            node_ids.push(tid);
        }
    }
    node_ids.sort_unstable();
    let index_of: HashMap<i64, usize> = node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let n = node_ids.len();
    let mut walk_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    {
        // For each node in our subgraph, find its outgoing call edges, then
        // reverse them into the walk graph.
        let mut stmt = conn.prepare(
            "
            SELECT to_symbol_name FROM edges WHERE from_symbol_id = ?1
            ",
        )?;
        for &caller_id in &node_ids {
            let caller_idx = index_of[&caller_id];
            let rows = stmt.query_map(params![caller_id], |row| row.get::<_, String>(0))?;
            for to_name_res in rows {
                let to_name = to_name_res?;
                let Some(targets) = name_to_ids.get(&to_name) else {
                    continue;
                };
                for &tid in targets {
                    let Some(&t_idx) = index_of.get(&tid) else {
                        continue;
                    };
                    if t_idx == caller_idx {
                        continue;
                    }
                    // Walk graph edge: callee -> caller.
                    walk_adj[t_idx].push(caller_idx);
                }
            }
        }
    }

    // Personalised PageRank: teleport on target if known, else uniform over depth-1.
    let mut teleport = vec![0.0f32; n];
    if let Some(tid) = target_id {
        if let Some(&t_idx) = index_of.get(&tid) {
            teleport[t_idx] = 1.0;
        }
    }
    if teleport.iter().sum::<f32>() == 0.0 {
        // No resolved target — spread teleport over direct callers (depth 1).
        let direct: Vec<usize> = frontier
            .iter()
            .filter_map(|(id, (d, _))| {
                if *d == 1 {
                    index_of.get(id).copied()
                } else {
                    None
                }
            })
            .collect();
        if !direct.is_empty() {
            let share = 1.0 / direct.len() as f32;
            for idx in direct {
                teleport[idx] = share;
            }
        } else if let Some(first) = index_of.values().next() {
            teleport[*first] = 1.0;
        }
    }

    let damping = 0.85f32;
    let iterations = 25;
    let mut rank = teleport.clone();
    let out_degree: Vec<usize> = walk_adj.iter().map(|adj| adj.len()).collect();
    for _ in 0..iterations {
        let mut next = vec![0.0f32; n];
        for v in 0..n {
            next[v] += (1.0 - damping) * teleport[v];
        }
        let mut dangling_mass = 0.0f32;
        for u in 0..n {
            if out_degree[u] == 0 {
                dangling_mass += rank[u];
                continue;
            }
            let share = damping * rank[u] / out_degree[u] as f32;
            for &v in &walk_adj[u] {
                next[v] += share;
            }
        }
        if dangling_mass > 0.0 {
            for v in 0..n {
                next[v] += damping * dangling_mass * teleport[v];
            }
        }
        let total: f32 = next.iter().sum();
        if total > 0.0 {
            for v in &mut next {
                *v /= total;
            }
        }
        rank = next;
    }

    // Materialise callers with normalised criticality + breakdown.
    let max_rank = frontier
        .keys()
        .filter_map(|id| index_of.get(id).map(|&i| rank[i]))
        .fold(0.0f32, f32::max);
    let max_rank = if max_rank > 0.0 { max_rank } else { 1.0 };

    let mut callers: Vec<ImpactCaller> = frontier
        .into_iter()
        .filter_map(|(id, (d, record))| {
            let idx = index_of.get(&id)?;
            let pagerank = rank[*idx];
            let normalised = (pagerank / max_rank * 100.0).clamp(0.0, 100.0);

            let fanout_out = db::symbol_fanout(conn, id).unwrap_or(0);
            let fanout_in = db::symbol_callers_count(conn, &record.name).unwrap_or(0);
            let exported = record.exported;
            let test_coverage = if is_test_path(&record.path) { 1 } else { 0 };
            let depth_decay = 1.0 / d as f32;

            Some(ImpactCaller {
                symbol: record,
                depth: d,
                criticality: normalised,
                breakdown: CriticalityBreakdown {
                    pagerank,
                    fanout_in,
                    fanout_out,
                    exported,
                    test_coverage,
                    depth_decay,
                },
            })
        })
        .collect();

    callers.sort_by(|a, b| {
        b.criticality
            .partial_cmp(&a.criticality)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.depth.cmp(&b.depth))
            .then_with(|| a.symbol.path.cmp(&b.symbol.path))
    });
    callers.truncate(100);

    let tokens = estimate_tokens(&callers);
    Ok(ImpactResult {
        symbol: symbol.to_string(),
        callers,
        meta: meta(tokens, "find_references", 700, 0.78),
    })
}

pub fn validate_conn(conn: &Connection, symbol: &str) -> Result<ValidateResult> {
    let bloom_bytes = db::get_meta_blob(conn, "bloom_symbols")?;
    let bloom_hit = bloom_bytes
        .as_deref()
        .and_then(BloomFilter::from_bytes)
        .map(|b| b.contains(symbol))
        .unwrap_or(true);

    let exists = if bloom_hit {
        symbol_exists(conn, symbol)?
    } else {
        false
    };

    let candidates = if exists {
        Vec::new()
    } else {
        fuzzy_candidates(conn, symbol, 5)?
    };

    let tokens = estimate_tokens(&candidates).max(40);
    Ok(ValidateResult {
        query: symbol.to_string(),
        exists,
        bloom_hit,
        candidates,
        meta: meta(tokens, "find_definition", 320, 0.8),
    })
}

pub fn validate_snippet_conn(
    conn: &Connection,
    code: &str,
    language: Language,
) -> Result<ValidateSnippetResult> {
    let parsed = indexer::parse_file(language, code)?;
    let bloom_bytes = db::get_meta_blob(conn, "bloom_symbols")?;
    let bloom = bloom_bytes.as_deref().and_then(BloomFilter::from_bytes);

    let mut checks = Vec::with_capacity(parsed.references.len());
    let mut unresolved = 0;
    for reference in &parsed.references {
        let bloom_hit = bloom
            .as_ref()
            .map(|b| b.contains(&reference.symbol_name))
            .unwrap_or(true);
        let exists = if bloom_hit {
            symbol_exists(conn, &reference.symbol_name)?
        } else {
            false
        };
        let candidates = if exists {
            Vec::new()
        } else {
            unresolved += 1;
            fuzzy_candidates(conn, &reference.symbol_name, 3)?
        };
        checks.push(SnippetReferenceCheck {
            symbol_name: reference.symbol_name.clone(),
            line: reference.line,
            column: reference.column,
            exists,
            candidates,
        });
    }
    let tokens = estimate_tokens(&checks).max(60);
    Ok(ValidateSnippetResult {
        language: language.to_string(),
        total_calls: checks.len(),
        unresolved_calls: unresolved,
        checks,
        meta: meta(tokens, "validate", 200, 0.9),
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

    let snapshot_present = db_path
        .parent()
        .map(|p| p.join("snapshot.bin").exists())
        .unwrap_or(false);

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
        snapshot_present,
        meta: meta(tokens, "get_outline", 320, 0.6),
    })
}

pub fn context_pack_conn(
    conn: &Connection,
    symbol: &str,
    budget_tokens: usize,
) -> Result<ContextPack> {
    let budget = if budget_tokens == 0 {
        1500
    } else {
        budget_tokens
    };
    // Budget allocation: roughly 40% body, 20% deps, 25% callers, 15% tests.
    let body_budget = budget * 4 / 10;
    let deps_budget = budget * 2 / 10;
    let callers_budget = budget * 25 / 100;
    let tests_budget = budget.saturating_sub(body_budget + deps_budget + callers_budget);

    let Some(target) = db::resolve_symbol(conn, symbol)? else {
        return Ok(ContextPack {
            symbol: None,
            body: None,
            dependency_signatures: Vec::new(),
            caller_signatures: Vec::new(),
            tests: Vec::new(),
            budget_tokens: budget,
            meta: meta(40, "find_definition", 120, 0.6),
        });
    };

    // Body — clipped to body_budget tokens.
    let raw_body = read_symbol_body(conn, &target).ok();
    let body = raw_body.map(|b| clip_to_token_budget(&b, body_budget));

    // Dependency signatures — outgoing refs from this symbol, dedup + resolve to defining symbols.
    let mut dep_lines: Vec<SignatureLine> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "
            SELECT DISTINCT r.symbol_name FROM refs r
            WHERE r.from_symbol_id = ?1
            LIMIT 80
            ",
        )?;
        let names = stmt
            .query_map(params![target.id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for name in names {
            if let Some(sym) = db::resolve_symbol(conn, &name)? {
                dep_lines.push(SignatureLine {
                    qualified_name: sym.qualified_name.clone(),
                    kind: sym.kind.clone(),
                    path: sym.path.clone(),
                    line: sym.start_line,
                    signature: sym.signature.clone(),
                });
            }
        }
    }
    trim_signature_lines(&mut dep_lines, deps_budget);

    // Caller signatures — direct callers (depth-1 of impact).
    let mut caller_lines: Vec<SignatureLine> = Vec::new();
    for caller in callers_for_symbol(conn, &target.qualified_name, 60)? {
        caller_lines.push(SignatureLine {
            qualified_name: caller.qualified_name,
            kind: caller.kind,
            path: caller.path,
            line: caller.start_line,
            signature: caller.signature,
        });
    }
    if caller_lines.is_empty() && target.qualified_name != target.name {
        for caller in callers_for_symbol(conn, &target.name, 60)? {
            caller_lines.push(SignatureLine {
                qualified_name: caller.qualified_name,
                kind: caller.kind,
                path: caller.path,
                line: caller.start_line,
                signature: caller.signature,
            });
        }
    }
    trim_signature_lines(&mut caller_lines, callers_budget);

    // Tests that transitively touch this symbol (capped, budget-aware).
    let mut tests = tests_for_conn(conn, &target.qualified_name)?.tests;
    tests.truncate(8);
    trim_records_to_budget(&mut tests, tests_budget);

    let tokens = estimate_tokens(&body)
        + estimate_tokens(&dep_lines)
        + estimate_tokens(&caller_lines)
        + estimate_tokens(&tests);

    Ok(ContextPack {
        symbol: Some(target),
        body,
        dependency_signatures: dep_lines,
        caller_signatures: caller_lines,
        tests,
        budget_tokens: budget,
        meta: meta(tokens, "expand_symbol", 800, 0.88),
    })
}

pub fn diff_impact_conn(
    conn: &Connection,
    from_ref: &str,
    to_ref: Option<&str>,
    depth: usize,
) -> Result<DiffImpactResult> {
    use std::process::Command;
    let root = db::get_meta(conn, "root")?
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let to = to_ref.unwrap_or("HEAD");
    let range = format!("{from_ref}..{to}");

    let out = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("diff")
        .arg("-U0")
        .arg(&range)
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
            let hunk_end = new_start + new_count.saturating_sub(1).max(0);
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

    // For each changed symbol, run a shallow impact and aggregate.
    let mut impacted: Vec<DiffImpactedSymbol> = Vec::new();
    for changed in &changed_symbols {
        let imp = impact_conn(conn, &changed.symbol.qualified_name, depth.max(1))?;
        for caller in imp.callers.into_iter().take(20) {
            if let Some(existing) = impacted
                .iter_mut()
                .find(|e| e.symbol.id == caller.symbol.id)
            {
                existing.criticality = existing.criticality.max(caller.criticality);
            } else {
                impacted.push(DiffImpactedSymbol {
                    symbol: caller.symbol,
                    via: changed.symbol.qualified_name.clone(),
                    criticality: caller.criticality,
                });
            }
        }
    }
    impacted.sort_by(|a, b| {
        b.criticality
            .partial_cmp(&a.criticality)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    impacted.truncate(50);

    let tokens = estimate_tokens(&changed_symbols) + estimate_tokens(&impacted);
    Ok(DiffImpactResult {
        from_ref: from_ref.to_string(),
        to_ref: to.to_string(),
        changed_files,
        changed_symbols,
        impacted,
        meta: meta(tokens, "impact", 900, 0.85),
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
        LIMIT 500
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
        meta: meta(tokens, "get_outline", 320, 0.7),
    })
}

pub fn imported_by_conn(conn: &Connection, source: &str) -> Result<ImportedByResult> {
    // Match exact + substring (`./users` matches `./users.ts`).
    let mut stmt = conn.prepare(
        "
        SELECT i.source, f.path, i.line, i.kind
        FROM imports i
        JOIN files f ON f.id = i.file_id
        WHERE i.source = ?1 OR i.source LIKE ?2
        ORDER BY f.path, i.line
        LIMIT 500
        ",
    )?;
    let like = format!("%{source}%");
    let importers: Vec<ImportRecord> = stmt
        .query_map(params![source, like], |row| {
            Ok(ImportRecord {
                source: row.get(0)?,
                from_path: row.get(1)?,
                line: row.get::<_, i64>(2)? as usize,
                kind: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let tokens = estimate_tokens(&importers).max(40);
    Ok(ImportedByResult {
        source: source.to_string(),
        importers,
        meta: meta(tokens, "imports", 200, 0.7),
    })
}

pub fn signature_conn(conn: &Connection, symbol: &str) -> Result<SignatureResult> {
    let Some(target) = db::resolve_symbol(conn, symbol)? else {
        return Ok(SignatureResult {
            symbol: None,
            members: Vec::new(),
            meta: meta(30, "find_definition", 120, 0.5),
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
        meta: meta(tokens, "expand_symbol", 600, 0.8),
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
        meta: meta(tokens, "impact", 700, 0.75),
    })
}

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
    });
    hits.truncate(limit);

    let tokens = estimate_tokens(&hits).max(40);
    Ok(SearchResult {
        query: pattern.to_string(),
        hits,
        meta: meta(tokens, "find_definition", 320, 0.7),
    })
}

pub fn tests_for_conn(conn: &Connection, symbol: &str) -> Result<TestsForResult> {
    // Walk callers transitively until we either find a test-path caller or exhaust
    // a small depth budget. We collect any caller whose file path looks like a test.
    let mut seen: HashSet<i64> = HashSet::new();
    let mut tests: Vec<SymbolRecord> = Vec::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((symbol.to_string(), 0));

    let max_depth = 6usize;
    while let Some((name, depth)) = queue.pop_front() {
        if depth > max_depth {
            continue;
        }
        for caller in callers_for_symbol(conn, &name, 500)? {
            if !seen.insert(caller.id) {
                continue;
            }
            if is_test_path(&caller.path) {
                tests.push(caller.clone());
            } else if depth + 1 < max_depth {
                queue.push_back((caller.name.clone(), depth + 1));
                if caller.qualified_name != caller.name {
                    queue.push_back((caller.qualified_name.clone(), depth + 1));
                }
            }
        }
    }

    tests.sort_by(|a, b| a.path.cmp(&b.path).then(a.start_line.cmp(&b.start_line)));
    let tokens = estimate_tokens(&tests).max(60);
    Ok(TestsForResult {
        symbol: symbol.to_string(),
        tests,
        meta: meta(tokens, "impact", 700, 0.78),
    })
}

// ─── Path-based wrappers used by the CLI ─────────────────────────────────────

pub fn find_definition(db_path: &Path, symbol: &str) -> Result<DefinitionResult> {
    let conn = db::open(db_path)?;
    find_definition_conn(&conn, symbol)
}

pub fn find_references(db_path: &Path, symbol: &str) -> Result<ReferencesResult> {
    let conn = db::open(db_path)?;
    find_references_conn(&conn, symbol)
}

pub fn get_outline(db_path: &Path, path: &Path) -> Result<OutlineResult> {
    let conn = db::open(db_path)?;
    get_outline_conn(&conn, path)
}

pub fn expand_symbol(db_path: &Path, symbol: &str) -> Result<ExpandResult> {
    let conn = db::open(db_path)?;
    expand_symbol_conn(&conn, symbol)
}

pub fn impact(db_path: &Path, symbol: &str, depth: usize) -> Result<ImpactResult> {
    let conn = db::open(db_path)?;
    impact_conn(&conn, symbol, depth)
}

pub fn validate(db_path: &Path, symbol: &str) -> Result<ValidateResult> {
    let conn = db::open(db_path)?;
    validate_conn(&conn, symbol)
}

pub fn validate_snippet(
    db_path: &Path,
    code: &str,
    language: Language,
) -> Result<ValidateSnippetResult> {
    let conn = db::open(db_path)?;
    validate_snippet_conn(&conn, code, language)
}

pub fn stats(db_path: &Path) -> Result<StatsResult> {
    let conn = db::open(db_path)?;
    stats_conn(&conn, db_path)
}

pub fn tests_for(db_path: &Path, symbol: &str) -> Result<TestsForResult> {
    let conn = db::open(db_path)?;
    tests_for_conn(&conn, symbol)
}

pub fn search(db_path: &Path, pattern: &str, options: SearchOptions) -> Result<SearchResult> {
    let conn = db::open(db_path)?;
    search_conn(&conn, pattern, options)
}

pub fn context_pack(db_path: &Path, symbol: &str, budget: usize) -> Result<ContextPack> {
    let conn = db::open(db_path)?;
    context_pack_conn(&conn, symbol, budget)
}

pub fn diff_impact(
    db_path: &Path,
    from_ref: &str,
    to_ref: Option<&str>,
    depth: usize,
) -> Result<DiffImpactResult> {
    let conn = db::open(db_path)?;
    diff_impact_conn(&conn, from_ref, to_ref, depth)
}

pub fn imports(db_path: &Path, path: &str) -> Result<ImportsResult> {
    let conn = db::open(db_path)?;
    imports_conn(&conn, path)
}

pub fn imported_by(db_path: &Path, source: &str) -> Result<ImportedByResult> {
    let conn = db::open(db_path)?;
    imported_by_conn(&conn, source)
}

pub fn signature(db_path: &Path, symbol: &str) -> Result<SignatureResult> {
    let conn = db::open(db_path)?;
    signature_conn(&conn, symbol)
}

pub fn siblings(db_path: &Path, symbol: &str) -> Result<SiblingsResult> {
    let conn = db::open(db_path)?;
    siblings_conn(&conn, symbol)
}

pub fn shell(db_path: &Path) -> Result<()> {
    println!("Tessera shell. Commands: def <symbol>, refs <symbol>, outline <path>, expand <symbol>, impact <symbol>, validate <symbol>, stats, tests <symbol>, quit");
    let mut input = String::new();
    loop {
        input.clear();
        print!("tessera> ");
        io::stdout().flush()?;
        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }
        let command = input.trim();
        if command.is_empty() {
            continue;
        }
        if command == "quit" || command == "exit" {
            break;
        }
        let mut parts = command.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or_default().trim();
        match name {
            "def" => println!("{}", find_definition(db_path, arg)?),
            "refs" => println!("{}", find_references(db_path, arg)?),
            "outline" => println!("{}", get_outline(db_path, Path::new(arg))?),
            "expand" => println!("{}", expand_symbol(db_path, arg)?),
            "impact" => println!("{}", impact(db_path, arg, 4)?),
            "validate" => println!("{}", validate(db_path, arg)?),
            "tests" => println!("{}", tests_for(db_path, arg)?),
            "stats" => println!("{}", stats(db_path)?),
            _ => println!("Unknown command: {name}"),
        }
    }
    Ok(())
}

// ─── Internals ───────────────────────────────────────────────────────────────

fn references_for_symbol(
    conn: &Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<ReferenceRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT r.id, r.symbol_name, r.from_symbol_id, s.qualified_name, f.path,
               r.line, r.column, r.context, r.kind
        FROM refs r
        JOIN files f ON f.id = r.file_id
        LEFT JOIN symbols s ON s.id = r.from_symbol_id
        WHERE r.symbol_name = ?1 OR r.symbol_name LIKE ?2
        ORDER BY f.path, r.line
        LIMIT ?3
        ",
    )?;
    let rows = stmt.query_map(
        params![symbol, format!("%.{}", symbol), limit as i64],
        db::map_reference,
    )?;
    let refs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(refs)
}

fn references_from_symbol(
    conn: &Connection,
    symbol_id: i64,
    limit: usize,
) -> Result<Vec<ReferenceRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT r.id, r.symbol_name, r.from_symbol_id, s.qualified_name, f.path,
               r.line, r.column, r.context, r.kind
        FROM refs r
        JOIN files f ON f.id = r.file_id
        LEFT JOIN symbols s ON s.id = r.from_symbol_id
        WHERE r.from_symbol_id = ?1
        ORDER BY r.line
        LIMIT ?2
        ",
    )?;
    let rows = stmt.query_map(params![symbol_id, limit as i64], db::map_reference)?;
    let refs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(refs)
}

fn callers_for_symbol(conn: &Connection, symbol: &str, limit: usize) -> Result<Vec<SymbolRecord>> {
    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
               s.start_line, s.end_line, s.signature, s.exported
        FROM edges e
        JOIN symbols s ON s.id = e.from_symbol_id
        JOIN files f ON f.id = s.file_id
        WHERE e.to_symbol_name = ?1 OR e.to_symbol_name LIKE ?2
        ORDER BY f.path, s.start_line
        LIMIT ?3
        ",
    )?;
    let rows = stmt.query_map(
        params![symbol, format!("%.{}", symbol), limit as i64],
        db::map_symbol,
    )?;
    let symbols = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(symbols)
}

fn read_symbol_body(conn: &Connection, symbol: &SymbolRecord) -> Result<String> {
    let path = db::get_meta(conn, "root")?
        .map(|root| PathBuf::from(root).join(&symbol.path))
        .unwrap_or_else(|| PathBuf::from(&symbol.path));
    let content = fs::read_to_string(path)?;
    let body = content
        .lines()
        .skip(symbol.start_line.saturating_sub(1))
        .take(symbol.end_line.saturating_sub(symbol.start_line) + 1)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(body)
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
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

fn fuzzy_symbol_matches(
    conn: &Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<SymbolRecord>> {
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
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);
    Ok(ranked.into_iter().map(|(_, s)| s).collect())
}

fn clip_to_token_budget(text: &str, budget: usize) -> String {
    let max_chars = budget.saturating_mul(4);
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max_chars).collect();
    clipped.push_str("\n// … truncated by context budget");
    clipped
}

fn trim_signature_lines(lines: &mut Vec<SignatureLine>, budget: usize) {
    while estimate_tokens(lines) > budget && !lines.is_empty() {
        lines.pop();
    }
}

fn trim_records_to_budget(records: &mut Vec<SymbolRecord>, budget: usize) {
    while estimate_tokens(records) > budget && !records.is_empty() {
        records.pop();
    }
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

fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("/__tests__/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.ends_with("_test.go")
        || lower.starts_with("test_")
        || lower.contains("/test_")
}

fn estimate_tokens<T: serde::Serialize>(value: &T) -> usize {
    let bytes = serde_json::to_vec(value)
        .map(|json| json.len())
        .unwrap_or(0);
    (bytes / 4).max(1)
}

fn meta(tokens: usize, alt_tool: &str, alt_tokens: usize, fidelity: f32) -> QueryMeta {
    QueryMeta {
        tokens_returned_estimate: tokens,
        alternative_queries: vec![AlternativeQuery {
            tool: alt_tool.to_string(),
            tokens_estimate: alt_tokens,
            fidelity,
        }],
    }
}
