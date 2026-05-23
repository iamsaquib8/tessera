use std::io::Read;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

use tessera_codegraph::bench::{self, BenchOptions};
use tessera_codegraph::db;
use tessera_codegraph::indexer::{self, IndexOptions};
use tessera_codegraph::mcp;
use tessera_codegraph::query;
use tessera_codegraph::snapshot;
use tessera_codegraph::types::{GraphEngineKind, Language, SearchOptions};

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
    /// Return the minimal set of tests whose call graph touches the symbol.
    TestsFor {
        symbol: String,
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
