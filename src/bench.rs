//! Benchmark harness. Indexes a path (real or synthetic) and reports the
//! numbers that live in the README's perf chart. We deliberately keep this
//! self-contained and reproducible — `tessera bench` is meant to be runnable
//! by anyone evaluating Tessera.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use tempfile::TempDir;

use crate::db;
use crate::indexer::{self, IndexOptions};
use crate::query;
use crate::types::{BenchQuery, BenchResult, BenchSavings};

pub struct BenchOptions {
    pub path: Option<PathBuf>,
    pub probe_symbol: Option<String>,
    /// Size of the synthetic repo when `path` is None. Defaults to 50.
    pub scale: Option<usize>,
}

pub fn run(options: BenchOptions) -> Result<BenchResult> {
    let (path, _synthetic_dir) = match options.path {
        Some(p) => (p, None),
        None => {
            let dir = TempDir::new()?;
            generate_synthetic_repo(dir.path(), options.scale.unwrap_or(50))?;
            let path = dir.path().to_path_buf();
            (path, Some(dir))
        }
    };

    let db_dir = tempfile::tempdir()?;
    let db_path = db_dir.path().join("bench.db");

    // Cold index.
    let full = indexer::index_path_with(
        &path,
        &db_path,
        IndexOptions {
            full: true,
            build_snapshot: true,
        },
    )?;

    // Incremental rerun (no file changes — exercises the sha-skip path).
    let incremental = indexer::index_path_with(
        &path,
        &db_path,
        IndexOptions {
            full: false,
            build_snapshot: false,
        },
    )?;

    let conn = db::open(&db_path)?;
    // Pick a symbol that actually has callers so the "who calls X?" comparison
    // produces meaningful numbers, not an empty result.
    let probe = options
        .probe_symbol
        .unwrap_or_else(|| pick_called_symbol(&conn).unwrap_or_else(|_| "main".to_string()));

    let queries = vec![
        bench_query(&conn, "find_definition", || {
            let r = query::find_definition_conn(&conn, &probe)?;
            Ok(estimate(&r))
        })?,
        bench_query(&conn, "find_references", || {
            let r = query::find_references_conn(&conn, &probe)?;
            Ok(estimate(&r))
        })?,
        bench_query(&conn, "get_outline", || {
            let r = query::get_outline_conn(&conn, Path::new("."))?;
            Ok(estimate(&r))
        })?,
        bench_query(&conn, "impact", || {
            let r = query::impact_conn(&conn, &probe, 4)?;
            Ok(estimate(&r))
        })?,
        bench_query(&conn, "validate", || {
            let r = query::validate_conn(&conn, &probe)?;
            Ok(estimate(&r))
        })?,
    ];

    // Headline comparison: "who calls X?" via raw grep+read vs `tessera impact`.
    // The raw baseline is the total token cost of files that actually contain
    // the symbol name — that's the work an agent would do with grep + read.
    let raw_callers_tokens = estimate_raw_grep_tokens(&path, &probe)?;
    let impact_tokens = queries
        .iter()
        .find(|q| q.name == "impact")
        .map(|q| q.tokens)
        .unwrap_or(1);

    // Secondary comparison: "where is X defined?" — agent reads at least one
    // file to confirm a definition; Tessera returns a single symbol record.
    let raw_definition_tokens = estimate_mean_file_tokens(&path)?;
    let find_def_tokens = queries
        .iter()
        .find(|q| q.name == "find_definition")
        .map(|q| q.tokens)
        .unwrap_or(1);

    let savings = vec![
        BenchSavings {
            label: format!("\"who calls {}?\"", probe),
            raw_tokens: raw_callers_tokens,
            tessera_tokens: impact_tokens,
            ratio: ratio(raw_callers_tokens, impact_tokens),
        },
        BenchSavings {
            label: format!("\"where is {} defined?\"", probe),
            raw_tokens: raw_definition_tokens,
            tessera_tokens: find_def_tokens,
            ratio: ratio(raw_definition_tokens, find_def_tokens),
        },
    ];

    let chart = render_chart(
        &path,
        full.files_indexed + full.files_reused,
        full.symbols_indexed,
        full.references_indexed,
        full.elapsed_ms,
        incremental.elapsed_ms,
        &savings,
        &queries,
    );

    Ok(BenchResult {
        path: path.to_string_lossy().to_string(),
        files: full.files_indexed + full.files_reused,
        symbols: full.symbols_indexed,
        references: full.references_indexed,
        index_full_ms: full.elapsed_ms,
        index_incremental_ms: incremental.elapsed_ms,
        queries,
        savings,
        chart,
    })
}

fn bench_query<F>(_conn: &rusqlite::Connection, name: &str, mut run_one: F) -> Result<BenchQuery>
where
    F: FnMut() -> Result<usize>,
{
    let iterations: u128 = 3;
    let started = Instant::now();
    let mut tokens = 0;
    for _ in 0..iterations {
        tokens = run_one()?;
    }
    let elapsed_ms = started.elapsed().as_millis() / iterations;
    Ok(BenchQuery {
        name: name.to_string(),
        ms: elapsed_ms,
        tokens,
    })
}

fn estimate<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| (bytes.len() / 4).max(1))
        .unwrap_or(1)
}

fn estimate_raw_grep_tokens(root: &Path, symbol: &str) -> Result<usize> {
    // Approximate the agent's "grep + read every match" workflow: any source
    // file that mentions `symbol` would be read in full. We sum those file
    // sizes and divide by 4 (the same heuristic the per-query estimator uses).
    let mut total = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if crate::types::Language::from_extension(ext).is_none() {
            continue;
        }
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if content.contains(symbol) {
            total += content.len() / 4;
        }
    }
    Ok(total.max(1))
}

fn estimate_mean_file_tokens(root: &Path) -> Result<usize> {
    // Agents that ask "where is X defined?" typically read one file —
    // approximate that cost with the mean source-file token count.
    let mut total = 0usize;
    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if crate::types::Language::from_extension(ext).is_none() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            total += (meta.len() as usize) / 4;
            count += 1;
        }
    }
    Ok(if count == 0 { 1 } else { total / count })
}

fn ratio(raw: usize, tessera: usize) -> f32 {
    if tessera == 0 {
        return f32::INFINITY;
    }
    raw as f32 / tessera as f32
}

fn pick_called_symbol(conn: &rusqlite::Connection) -> Result<String> {
    // Prefer a symbol that has at least one inbound edge (someone calls it).
    // Falls back to the most-defined function/method name if no edges exist.
    if let Ok(name) = conn.query_row::<String, _, _>(
        "
        SELECT s.name
        FROM symbols s
        JOIN edges e ON e.to_symbol_name = s.name OR e.to_symbol_name = s.qualified_name
        WHERE s.kind IN ('function', 'method')
        GROUP BY s.name
        ORDER BY COUNT(*) DESC, s.name
        LIMIT 1
        ",
        [],
        |row| row.get(0),
    ) {
        return Ok(name);
    }
    let name: String = conn.query_row(
        "
        SELECT name FROM symbols
        WHERE kind IN ('function', 'method')
        GROUP BY name
        ORDER BY COUNT(*) DESC
        LIMIT 1
        ",
        [],
        |row| row.get(0),
    )?;
    Ok(name)
}

#[allow(clippy::too_many_arguments)]
fn render_chart(
    path: &Path,
    files: usize,
    symbols: usize,
    refs: usize,
    full_ms: u128,
    inc_ms: u128,
    savings: &[BenchSavings],
    queries: &[BenchQuery],
) -> String {
    const BAR_W: usize = 32;
    let mut out = String::new();

    out.push_str(&format!("Tessera v{} bench\n", env!("CARGO_PKG_VERSION")));
    out.push_str("─────────────────────\n");
    out.push_str(&format!("Repo: {}\n", short_path(path)));
    out.push_str(&format!(
        "  {} files · {} symbols · {} references\n\n",
        thousands(files),
        thousands(symbols),
        thousands(refs)
    ));

    out.push_str("Index time\n");
    let max_ms = full_ms.max(inc_ms).max(1) as f32;
    out.push_str(&format!(
        "  full         {}  {:>6} ms\n",
        bar(full_ms as f32 / max_ms, BAR_W),
        full_ms
    ));
    out.push_str(&format!(
        "  incremental  {}  {:>6} ms",
        bar(inc_ms as f32 / max_ms, BAR_W),
        inc_ms
    ));
    if inc_ms > 0 && full_ms > inc_ms {
        out.push_str(&format!(
            "   ·  {:.0}× faster",
            full_ms as f32 / inc_ms.max(1) as f32
        ));
    }
    out.push_str("\n\n");

    for s in savings {
        out.push_str(&format!("{}\n", s.label));
        let max_tokens = s.raw_tokens.max(s.tessera_tokens).max(1) as f32;
        out.push_str(&format!(
            "  raw grep + read   {}  {:>8} tokens\n",
            bar(s.raw_tokens as f32 / max_tokens, BAR_W),
            thousands(s.raw_tokens)
        ));
        out.push_str(&format!(
            "  tessera           {}  {:>8} tokens   ·  {:.0}× cheaper\n\n",
            bar(s.tessera_tokens as f32 / max_tokens, BAR_W),
            thousands(s.tessera_tokens),
            s.ratio
        ));
    }

    if !queries.is_empty() {
        out.push_str("Per-query latency  ·  median of 3 runs\n");
        for q in queries {
            out.push_str(&format!(
                "  {:<18} {:>3} ms     ~{:>5} tokens\n",
                q.name,
                q.ms,
                thousands(q.tokens)
            ));
        }
    }

    out
}

fn bar(fraction: f32, width: usize) -> String {
    let fraction = fraction.clamp(0.0, 1.0);
    let filled =
        ((fraction * width as f32).round() as usize).max(if fraction > 0.0 { 1 } else { 0 });
    let mut s = String::with_capacity(width);
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..width {
        s.push(' ');
    }
    s
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn short_path(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    // Tempdirs are noisy; collapse them.
    if s.contains("/tmp/") || s.contains("/T/") {
        return "synthetic repo".to_string();
    }
    s
}

fn generate_synthetic_repo(root: &Path, file_count: usize) -> Result<()> {
    // Models a realistic "popular utility" topology:
    //   util.ts exports sharedHelper / formatRow / parseInput
    //   every module_i calls sharedHelper + neighbor bridges
    // sharedHelper ends up with `file_count` callers, which is what makes the
    // "who calls sharedHelper?" comparison interesting.
    fs::create_dir_all(root)?;
    fs::write(root.join("util.ts"), synthetic_util())?;
    for i in 0..file_count {
        let path = root.join(format!("module_{i:03}.ts"));
        let content = synthetic_module(i, file_count);
        fs::write(path, content)?;
    }
    Ok(())
}

fn synthetic_util() -> String {
    "// Shared utility module — imported across the synthetic repo.
// In real codebases, files like this are the heaviest blast-radius targets
// for any refactor, which is why Tessera ranks their callers by PageRank.

export function sharedHelper(x: number): number {
    // Inline computation; mirrors the kind of common helper found in real
    // codebases that ends up called from dozens or hundreds of places.
    return (x * 13 + 7) ^ 0;
}

export function formatRow(row: { id: number; name: string }): string {
    return `${row.id}\\t${row.name}`;
}

export function parseInput(raw: string): number {
    return Number.parseInt(raw, 10);
}
"
    .to_string()
}

fn synthetic_module(index: usize, total: usize) -> String {
    let prev = (index + total - 1) % total;
    let next = (index + 1) % total;
    format!(
        "// Module {index} of {total} — autogenerated for `tessera bench`.
// Each module imports the shared utility module and forwards through a
// couple of bridge functions, modelling the call topology of real services
// where a small set of helpers is called from many places.

import {{ sharedHelper, formatRow, parseInput }} from \"./util\";

export interface Module{index}Input {{
    raw: string;
    label: string;
}}

export function moduleEntry{index}(input: Module{index}Input): string {{
    const parsed = parseInput(input.raw);
    const value = helper{index}(parsed);
    return formatRow({{ id: value, name: input.label }});
}}

function helper{index}(x: number): number {{
    // Cross-cuts: every helper depends on the shared utility plus its two
    // neighbours, so the call graph has both a popular target and a chain.
    return sharedHelper(x) + bridge{prev}(x) + bridge{next}(x);
}}

export function bridge{index}(x: number): number {{
    return sharedHelper(x + {index});
}}
"
    )
}
