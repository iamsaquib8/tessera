use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::params;

use crate::db;
use crate::types::{
    AlternativeQuery, DefinitionResult, ExpandResult, ImpactCaller, ImpactResult, OutlineResult,
    QueryMeta, ReferenceRecord, ReferencesResult, SymbolRecord,
};

pub fn find_definition(db_path: &Path, symbol: &str) -> Result<DefinitionResult> {
    let conn = db::open(db_path)?;
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
    let matches = stmt
        .query_map(params![symbol, format!("%.{}", symbol)], db::map_symbol)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let tokens = estimate_tokens(&matches);
    Ok(DefinitionResult {
        matches,
        meta: meta(tokens, "get_outline", 320, 0.72),
    })
}

pub fn find_references(db_path: &Path, symbol: &str) -> Result<ReferencesResult> {
    let conn = db::open(db_path)?;
    let refs = references_for_symbol(&conn, symbol, 250)?;
    let tokens = estimate_tokens(&refs);
    Ok(ReferencesResult {
        references: refs,
        meta: meta(tokens, "impact", 900, 0.84),
    })
}

pub fn get_outline(db_path: &Path, path: &Path) -> Result<OutlineResult> {
    let conn = db::open(db_path)?;
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

pub fn expand_symbol(db_path: &Path, symbol: &str) -> Result<ExpandResult> {
    let conn = db::open(db_path)?;
    let Some(symbol_record) = db::resolve_symbol(&conn, symbol)? else {
        return Ok(ExpandResult {
            symbol: None,
            body: None,
            dependencies: Vec::new(),
            meta: meta(20, "find_definition", 120, 0.65),
        });
    };
    let body = read_symbol_body(&conn, &symbol_record).ok();
    let dependencies = references_from_symbol(&conn, symbol_record.id, 100)?;
    let tokens = estimate_tokens(&body) + estimate_tokens(&dependencies);
    Ok(ExpandResult {
        symbol: Some(symbol_record),
        body,
        dependencies,
        meta: meta(tokens, "get_outline", 320, 0.7),
    })
}

pub fn impact(db_path: &Path, symbol: &str, depth: usize) -> Result<ImpactResult> {
    let conn = db::open(db_path)?;
    let mut callers = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(symbol.to_string(), 1usize)]);

    while let Some((target, current_depth)) = queue.pop_front() {
        if current_depth > depth {
            continue;
        }
        for caller in callers_for_symbol(&conn, &target, 500)? {
            if !seen.insert(caller.id) {
                continue;
            }
            let fanout = db::symbol_fanout(&conn, caller.id)?;
            let public_api = if caller.exported { 5 } else { 0 };
            let test_bonus = if caller.path.contains("test") || caller.path.contains("spec") {
                2
            } else {
                0
            };
            let criticality = fanout * 3 + public_api + test_bonus + (depth + 1 - current_depth);
            queue.push_back((caller.name.clone(), current_depth + 1));
            queue.push_back((caller.qualified_name.clone(), current_depth + 1));
            callers.push(ImpactCaller {
                symbol: caller,
                depth: current_depth,
                fanout,
                criticality,
            });
        }
    }

    callers.sort_by(|a, b| {
        b.criticality
            .cmp(&a.criticality)
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

pub fn shell(db_path: &Path) -> Result<()> {
    println!("Tessera shell. Commands: def <symbol>, refs <symbol>, outline <path>, expand <symbol>, impact <symbol>, quit");
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
            _ => println!("Unknown command: {name}"),
        }
    }
    Ok(())
}

fn references_for_symbol(
    conn: &rusqlite::Connection,
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
    conn: &rusqlite::Connection,
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

fn callers_for_symbol(
    conn: &rusqlite::Connection,
    symbol: &str,
    limit: usize,
) -> Result<Vec<SymbolRecord>> {
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

fn read_symbol_body(conn: &rusqlite::Connection, symbol: &SymbolRecord) -> Result<String> {
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

#[allow(dead_code)]
fn _group_by_file(symbols: &[SymbolRecord]) -> HashMap<&str, Vec<&SymbolRecord>> {
    let mut grouped = HashMap::new();
    for symbol in symbols {
        grouped
            .entry(symbol.path.as_str())
            .or_insert_with(Vec::new)
            .push(symbol);
    }
    grouped
}
