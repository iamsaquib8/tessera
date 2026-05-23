mod db;
mod indexer;
mod mcp;
mod query;
mod types;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tessera")]
#[command(about = "Semantic code graph and MCP server for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Index a repository into a SQLite semantic graph.
    Index {
        /// Repository or directory to index.
        path: PathBuf,
        /// SQLite database path.
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
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
    /// Return transitive callers ranked by simple criticality.
    Impact {
        symbol: String,
        #[arg(long, default_value = ".tessera/tessera.db")]
        db: PathBuf,
        #[arg(long, default_value_t = 4)]
        depth: usize,
        #[arg(long)]
        json: bool,
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
        Commands::Index { path, db } => {
            let report = indexer::index_path(&path, &db)?;
            println!(
                "Indexed {} files, {} symbols, {} references into {} in {}ms",
                report.files_indexed,
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
