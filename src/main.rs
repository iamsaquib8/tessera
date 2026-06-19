use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

use tessera_codegraph::bench::{self, BenchOptions};
use tessera_codegraph::completions::{self, CompletionShell};
use tessera_codegraph::db;
use tessera_codegraph::doctor::{self, DoctorOptions};
use tessera_codegraph::indexer::{self, IndexOptions};
use tessera_codegraph::init::{self, InitOptions};
use tessera_codegraph::mcp;
use tessera_codegraph::query;
use tessera_codegraph::snapshot;
use tessera_codegraph::types::{GraphEngineKind, Language, SearchOptions, UnusedOptions};
use tessera_codegraph::watch::{self, WatchOptions};

#[derive(Debug, Parser)]
#[command(name = "tessera")]
#[command(version)]
#[command(about = "Semantic code graph and MCP server for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, ValueEnum)]
enum LangArg {
    Typescript,
    Tsx,
    Javascript,
    Python,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    Csharp,
    Ruby,
    Php,
}

impl From<LangArg> for Language {
    fn from(value: LangArg) -> Self {
        match value {
            LangArg::Typescript => Language::TypeScript,
            LangArg::Tsx => Language::Tsx,
            LangArg::Javascript => Language::JavaScript,
            LangArg::Python => Language::Python,
            LangArg::Go => Language::Go,
            LangArg::Rust => Language::Rust,
            LangArg::Java => Language::Java,
            LangArg::C => Language::C,
            LangArg::Cpp => Language::Cpp,
            LangArg::Csharp => Language::CSharp,
            LangArg::Ruby => Language::Ruby,
            LangArg::Php => Language::Php,
        }
    }
}

#[derive(Debug, Clone, ValueEnum, Default)]
enum GraphFormat {
    #[default]
    Mermaid,
    Dot,
}

impl GraphFormat {
    fn as_str(&self) -> &'static str {
        match self {
            GraphFormat::Mermaid => "mermaid",
            GraphFormat::Dot => "dot",
        }
    }
}

#[derive(Debug, Clone, ValueEnum, Default)]
enum EngineArg {
    #[default]
    Sqlite,
    Cozo,
}

impl From<EngineArg> for GraphEngineKind {
    fn from(value: EngineArg) -> Self {
        match value {
            EngineArg::Sqlite => GraphEngineKind::Sqlite,
            EngineArg::Cozo => GraphEngineKind::Cozo,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShellArg {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

impl From<CompletionShellArg> for CompletionShell {
    fn from(value: CompletionShellArg) -> Self {
        match value {
            CompletionShellArg::Bash => CompletionShell::Bash,
            CompletionShellArg::Zsh => CompletionShell::Zsh,
            CompletionShellArg::Fish => CompletionShell::Fish,
            CompletionShellArg::Powershell => CompletionShell::Powershell,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Index a repository into a SQLite semantic graph (incremental by default).
    Index {
        /// Repository or directory to index.
        path: PathBuf,
        /// SQLite database path.
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        /// Re-index from scratch instead of using the sha-diff incremental path.
        #[arg(long)]
        full: bool,
        /// Skip writing the memory-mapped snapshot at the end of indexing.
        #[arg(long)]
        no_snapshot: bool,
        /// Graph engine to use for impact queries. Cozo requires `--features cozo`.
        #[arg(long, value_enum, default_value_t = EngineArg::Sqlite)]
        graph_engine: EngineArg,
    },
    /// Watch a repository and incrementally re-index when source files change.
    Watch {
        /// Repository or directory to watch.
        path: PathBuf,
        /// SQLite database path.
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        /// Re-index from scratch instead of using the sha-diff incremental path.
        #[arg(long)]
        full: bool,
        /// Skip writing the memory-mapped snapshot after indexing.
        #[arg(long)]
        no_snapshot: bool,
        /// Poll interval in milliseconds.
        #[arg(long, default_value_t = 500)]
        poll_ms: u64,
        /// Debounce interval in milliseconds after a detected change.
        #[arg(long, default_value_t = 250)]
        debounce_ms: u64,
        /// Run one indexing pass and exit. Useful for smoke tests and CI.
        #[arg(long)]
        once: bool,
    },
    /// Check local Tessera setup and print actionable diagnostics.
    Doctor {
        /// Repository root to inspect.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path.
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Create project-local Tessera defaults and optional integration snippets.
    Init {
        /// Repository root to initialize.
        #[arg(default_value = ".")]
        root: PathBuf,
        /// SQLite database path to write into generated snippets.
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        /// Create local git hooks that run `tessera index .`.
        #[arg(long)]
        git_hooks: bool,
        /// Create MCP config snippets under `.tessera/mcp/`.
        #[arg(long)]
        mcp_configs: bool,
        /// Overwrite existing generated files.
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print shell completion script.
    Completions {
        #[arg(value_enum)]
        shell: CompletionShellArg,
    },
    /// Find symbol definitions by name.
    FindDefinition {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Find call/reference sites for a symbol.
    FindReferences {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return a token-cheap semantic outline for a file or directory.
    GetOutline {
        path: PathBuf,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return a symbol body plus immediate dependencies.
    ExpandSymbol {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return transitive callers ranked by personalised PageRank.
    Impact {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long, default_value_t = 4)]
        depth: usize,
        #[arg(long)]
        json: bool,
    },
    /// Check whether a symbol exists in the graph; suggest near-misses if not.
    Validate {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Parse a code snippet and validate every call against the graph.
    ValidateSnippet {
        #[arg(long, value_enum)]
        language: LangArg,
        /// Read the snippet from this file. If omitted, read from stdin.
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Print index statistics.
    Stats {
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Fuzzy / glob search across indexed symbols, filterable by kind,
    /// language, exported, and path prefix.
    Search {
        /// Substring, identifier, or `glob*` pattern.
        pattern: String,
        /// Filter by symbol kind (function, method, class, struct, …).
        /// Repeat or comma-separate to allow multiple.
        #[arg(long, value_delimiter = ',')]
        kind: Vec<String>,
        /// Filter by language (typescript, java, python, …).
        #[arg(long, value_delimiter = ',')]
        language: Vec<String>,
        /// Only show exported symbols (`--exported`) or only non-exported
        /// (`--exported=false`).
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        exported: Option<bool>,
        /// Match symbols whose file path starts with this prefix.
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Find indexed symbols with no inbound references or call edges.
    Unused {
        /// Filter by symbol kind (function, method, class, struct, ...).
        /// Repeat or comma-separate to allow multiple.
        #[arg(long, value_delimiter = ',')]
        kind: Vec<String>,
        /// Filter by language (typescript, java, python, ...).
        #[arg(long, value_delimiter = ',')]
        language: Vec<String>,
        /// Only show exported symbols (`--exported`) or only non-exported
        /// (`--exported=false`).
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        exported: Option<bool>,
        /// Match symbols whose file path starts with this prefix.
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Bundle body + dep signatures + caller signatures + tests for an agent
    /// to "understand" a symbol in one tool call.
    ContextPack {
        symbol: String,
        /// Token budget (default 1500).
        #[arg(long, default_value_t = 1500)]
        budget: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Map a git range to changed symbols + their PageRank-impacted callers.
    DiffImpact {
        /// Base git ref (e.g. `main`, `origin/main`, `HEAD~5`).
        from: String,
        /// Tip ref (defaults to `HEAD`).
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value_t = 3)]
        depth: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List the imports declared in a file or directory.
    Imports {
        path: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List files that import a given module / source path.
    ImportedBy {
        source: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return just the signature (and, for containers, member signatures) of a symbol.
    Signature {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Symbols that share callers with the target — the cluster to refactor together.
    Siblings {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Return the minimal set of tests whose call graph touches the symbol.
    TestsFor {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Find the shortest call path from one symbol to another (A calls … calls B).
    Connect {
        from: String,
        to: String,
        #[arg(long, default_value_t = 8)]
        depth: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Export the call graph as Graphviz DOT or Mermaid (whole graph, or a
    /// neighbourhood with `--from`).
    Export {
        /// Output format.
        #[arg(long, value_enum, default_value_t = GraphFormat::Mermaid)]
        format: GraphFormat,
        /// Restrict to the forward call subgraph rooted at this symbol.
        #[arg(long)]
        from: Option<String>,
        /// Traversal depth when `--from` is given.
        #[arg(long, default_value_t = 3)]
        depth: usize,
        /// Maximum number of edges to emit.
        #[arg(long, default_value_t = 800)]
        limit: usize,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Run a benchmark and emit the perf chart used in the README.
    Bench {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        probe: Option<String>,
        /// Synthetic-repo size when no --path is given.
        #[arg(long, default_value_t = 50)]
        scale: usize,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// (Re)build the memory-mapped snapshot used by the MCP server.
    Snapshot {
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
    },
    /// Run the MCP server over stdio.
    Mcp {
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
    },
    /// Start a tiny interactive query shell.
    Shell {
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            path,
            db,
            full,
            no_snapshot,
            graph_engine,
        } => {
            let engine: GraphEngineKind = graph_engine.into();
            if matches!(engine, GraphEngineKind::Cozo) && !cfg!(feature = "cozo") {
                return Err(anyhow!(
                    "Cozo backend not compiled in. Rebuild with `cargo install --features cozo`."
                ));
            }
            let options = IndexOptions {
                full,
                build_snapshot: !no_snapshot,
            };
            let report = indexer::index_path_with(&path, &db, options)?;
            let mode = match report.mode {
                indexer::IndexMode::Full => "full",
                indexer::IndexMode::Incremental => "incremental",
            };
            println!(
                "[{mode}] indexed {} files (+{} reused, -{} removed), {} symbols, {} references into {} in {}ms",
                report.files_indexed,
                report.files_reused,
                report.files_removed,
                report.symbols_indexed,
                report.references_indexed,
                db.display(),
                report.elapsed_ms
            );
        }
        Commands::Watch {
            path,
            db,
            full,
            no_snapshot,
            poll_ms,
            debounce_ms,
            once,
        } => {
            let options = WatchOptions {
                poll_interval: Duration::from_millis(poll_ms),
                debounce: Duration::from_millis(debounce_ms),
                index_options: IndexOptions {
                    full,
                    build_snapshot: !no_snapshot,
                },
                once,
            };
            watch::watch_path(&path, &db, options)?;
        }
        Commands::Doctor { root, db, json } => {
            let result = doctor::run(DoctorOptions { root, db_path: db })?;
            print_result(result, json)?;
        }
        Commands::Init {
            root,
            db,
            git_hooks,
            mcp_configs,
            force,
            json,
        } => {
            let result = init::run(InitOptions {
                root,
                db_path: db,
                git_hooks,
                mcp_configs,
                force,
            })?;
            print_result(result, json)?;
        }
        Commands::Completions { shell } => {
            print!("{}", completions::generate(shell.into()));
        }
        Commands::FindDefinition { symbol, db, json } => {
            print_result(query::find_definition(&db, &symbol)?, json)?;
        }
        Commands::FindReferences { symbol, db, json } => {
            print_result(query::find_references(&db, &symbol)?, json)?;
        }
        Commands::GetOutline { path, db, json } => {
            print_result(query::get_outline(&db, &path)?, json)?;
        }
        Commands::ExpandSymbol { symbol, db, json } => {
            print_result(query::expand_symbol(&db, &symbol)?, json)?;
        }
        Commands::Impact {
            symbol,
            db,
            depth,
            json,
        } => {
            print_result(query::impact(&db, &symbol, depth)?, json)?;
        }
        Commands::Validate { symbol, db, json } => {
            print_result(query::validate(&db, &symbol)?, json)?;
        }
        Commands::ValidateSnippet {
            language,
            file,
            db,
            json,
        } => {
            let code = match file {
                Some(path) => std::fs::read_to_string(path)?,
                None => {
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            let result = query::validate_snippet(&db, &code, language.into())?;
            print_result(result, json)?;
        }
        Commands::Stats { db, json } => {
            print_result(query::stats(&db)?, json)?;
        }
        Commands::TestsFor { symbol, db, json } => {
            print_result(query::tests_for(&db, &symbol)?, json)?;
        }
        Commands::Connect {
            from,
            to,
            depth,
            db,
            json,
        } => {
            print_result(query::connect(&db, &from, &to, depth)?, json)?;
        }
        Commands::Export {
            format,
            from,
            depth,
            limit,
            db,
            json,
        } => {
            print_result(
                query::export(&db, format.as_str(), from.as_deref(), depth, limit)?,
                json,
            )?;
        }
        Commands::ContextPack {
            symbol,
            budget,
            db,
            json,
        } => {
            print_result(query::context_pack(&db, &symbol, budget)?, json)?;
        }
        Commands::DiffImpact {
            from,
            to,
            depth,
            db,
            json,
        } => {
            print_result(query::diff_impact(&db, &from, to.as_deref(), depth)?, json)?;
        }
        Commands::Imports { path, db, json } => {
            print_result(query::imports(&db, &path)?, json)?;
        }
        Commands::ImportedBy { source, db, json } => {
            print_result(query::imported_by(&db, &source)?, json)?;
        }
        Commands::Signature { symbol, db, json } => {
            print_result(query::signature(&db, &symbol)?, json)?;
        }
        Commands::Siblings { symbol, db, json } => {
            print_result(query::siblings(&db, &symbol)?, json)?;
        }
        Commands::Search {
            pattern,
            kind,
            language,
            exported,
            path,
            limit,
            db,
            json,
        } => {
            let options = SearchOptions {
                kinds: kind,
                languages: language,
                exported,
                path_prefix: path,
                limit,
            };
            print_result(query::search(&db, &pattern, options)?, json)?;
        }
        Commands::Unused {
            kind,
            language,
            exported,
            path,
            limit,
            db,
            json,
        } => {
            let options = UnusedOptions {
                kinds: kind,
                languages: language,
                exported,
                path_prefix: path,
                limit,
            };
            print_result(query::unused(&db, options)?, json)?;
        }
        Commands::Bench {
            path,
            probe,
            scale,
            out,
            json,
        } => {
            let result = bench::run(BenchOptions {
                path,
                probe_symbol: probe,
                scale: Some(scale),
            })?;
            if let Some(out_path) = out {
                std::fs::write(&out_path, &result.chart)?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("{}", result.chart);
            }
        }
        Commands::Snapshot { db } => {
            let conn = db::open(&db)?;
            let snapshot_path = db
                .parent()
                .map(|p| p.join("snapshot.bin"))
                .unwrap_or_else(|| PathBuf::from("snapshot.bin"));
            snapshot::build(&conn, &snapshot_path)?;
            println!("snapshot written to {}", snapshot_path.display());
        }
        Commands::Mcp { db } => mcp::serve_stdio(&db)?,
        Commands::Shell { db } => query::shell(&db)?,
    }

    Ok(())
}

fn print_result<T>(value: T, json: bool) -> Result<()>
where
    T: serde::Serialize + std::fmt::Display,
{
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{value}");
    }
    Ok(())
}
