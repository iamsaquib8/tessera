use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use tessera_codegraph::db;
use tessera_codegraph::fresh::{self, Freshness, Refresh};
use tessera_codegraph::hook::{self, HookOptions};
use tessera_codegraph::indexer::{self, IndexMode, IndexOptions, IndexReport};
use tessera_codegraph::init::{self, InitOptions};
use tessera_codegraph::output::Confidence;
use tessera_codegraph::query::{
    self, BlastArgs, ContextArgs, FindArgs, PathArgs, RefsArgs, SearchArgs, VerifyArgs,
};
use tessera_codegraph::types::Language;
use tessera_codegraph::watch::{self, WatchOptions};
use tessera_codegraph::{mcp, ExportGroupBy, ExportOptions, Index, Workspace};

const EXIT_FINDINGS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INDEX: u8 = 3;

#[derive(Debug, Parser)]
#[command(name = "tessera", version)]
#[command(
    about = "Deterministic edit safety for AI coding agents: blast radius before an edit, verification after it."
)]
struct Cli {
    /// Repository root. Defaults to the nearest ancestor with .tessera/ or .git.
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Index database. Defaults to <root>/.tessera/tessera.db.
    #[arg(long, global = true, value_name = "FILE", env = "TESSERA_DB")]
    db: Option<PathBuf>,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Suppress progress messages on stderr.
    #[arg(long, short, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Wire Tessera into the agents this project uses: shows a diff, then asks.
    Init {
        /// Apply without asking.
        #[arg(long, short = 'y', conflicts_with = "dry_run")]
        yes: bool,
        /// Only show what would change.
        #[arg(long)]
        dry_run: bool,
        /// Set up this agent even if it is not detected; repeatable.
        #[arg(long = "agent", value_name = "NAME")]
        agents: Vec<String>,
        /// Also change user-wide config in your home directory.
        #[arg(long)]
        global: bool,
        /// Also add usage instructions to AGENTS.md, CLAUDE.md, and similar files.
        #[arg(long)]
        with_instructions: bool,
        /// Install a git pre-commit hook that runs `verify --staged`.
        #[arg(long)]
        git_hook: bool,
    },
    /// Show index health, freshness, and setup problems.
    Status,
    /// Find a symbol's definition, signature, and body, or outline a file.
    Find {
        /// Symbol name: `name`, `Class.method`, or `A::B`.
        symbol: Option<String>,
        /// Outline a file or directory instead of looking up a symbol.
        #[arg(long)]
        path: Option<String>,
        /// Include each definition's source body.
        #[arg(long)]
        body: bool,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Search symbols by fuzzy or glob pattern.
    Search {
        /// Pattern, e.g. `findBy*` or `usr`.
        pattern: String,
        /// Only these kinds (function, method, class, …); repeatable.
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Only these languages; repeatable.
        #[arg(long = "language")]
        languages: Vec<String>,
        /// Only symbols under this path prefix.
        #[arg(long)]
        path: Option<String>,
        /// Only exported symbols.
        #[arg(long, conflicts_with = "internal")]
        exported: bool,
        /// Only non-exported symbols.
        #[arg(long)]
        internal: bool,
        /// Only functions and methods with no inbound references.
        #[arg(long)]
        unused: bool,
        #[command(flatten)]
        page: PageArgs,
    },
    /// List references to a symbol, or the files importing a file.
    Refs {
        /// Symbol name or repo-relative file path.
        target: String,
        /// Only references from test files.
        #[arg(long)]
        tests: bool,
        /// Leave out references below this confidence.
        #[arg(long, value_enum, value_name = "LEVEL")]
        min_confidence: Option<MinConfidence>,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Rank everything a change to a symbol or diff can affect, plus tests to run.
    Blast {
        /// Symbol to analyse.
        symbol: Option<String>,
        /// Analyse the changes since this git revision instead.
        #[arg(long, value_name = "REV", conflicts_with = "symbol")]
        diff: Option<String>,
        /// End revision for --diff (default HEAD).
        #[arg(long, value_name = "REV", requires = "diff")]
        to: Option<String>,
        /// Caller traversal depth.
        #[arg(long, default_value_t = 4)]
        depth: usize,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Check that symbols or calls in a snippet exist. Exits 1 on problems.
    Verify {
        /// Symbol name to check.
        symbol: Option<String>,
        /// Code to check: a file path, or `-` for stdin.
        #[arg(long, value_name = "FILE|-")]
        snippet: Option<String>,
        /// Language of the snippet.
        #[arg(long, value_enum)]
        language: Option<LangArg>,
        /// Check every change in the working tree against HEAD, untracked files included.
        #[arg(long, conflicts_with_all = ["symbol", "snippet", "staged", "diff"])]
        changed: bool,
        /// Check the staged changes (for a pre-commit hook).
        #[arg(long, conflicts_with_all = ["symbol", "snippet", "diff"])]
        staged: bool,
        /// Check the working tree against this git revision.
        #[arg(long, value_name = "REV", conflicts_with_all = ["symbol", "snippet"])]
        diff: Option<String>,
        /// Output: human (default), agent (one line per blocking problem), json, sarif, or markdown (a pull-request comment).
        #[arg(long, value_enum, default_value_t = VerifyFormat::Human)]
        format: VerifyFormat,
    },
    /// Contracts between services: routes, gRPC methods, topics and tables, with both sides.
    Map {
        /// Contract kind: route, rpc, topic, or table; or `service` for what each service calls and serves.
        kind: Option<String>,
        /// Only contracts whose name or files contain this.
        query: Option<String>,
    },
    /// A local web UI: change impact, contract map, symbols, search.
    Ui {
        /// Port on 127.0.0.1.
        #[arg(long, default_value_t = 4747)]
        port: u16,
        /// Open it in the browser.
        #[arg(long)]
        open: bool,
        /// Write a static copy to this directory instead of serving.
        #[arg(long, value_name = "DIR")]
        export: Option<PathBuf>,
    },
    /// One budgeted bundle to read before editing a symbol.
    Context {
        /// Symbol name.
        symbol: String,
        /// Approximate token budget.
        #[arg(long, default_value_t = 2_000)]
        budget: usize,
    },
    /// Shortest call path from one symbol to another.
    Path {
        /// Starting symbol.
        from: String,
        /// Target symbol.
        to: String,
        /// Maximum path length.
        #[arg(long, default_value_t = 8)]
        depth: usize,
    },
    /// Export the call graph as Mermaid or Graphviz DOT.
    Export {
        /// Output format.
        #[arg(long, value_enum, default_value_t = GraphFormat::Mermaid)]
        format: GraphFormat,
        /// Only the forward call subgraph rooted at this symbol.
        #[arg(long)]
        from: Option<String>,
        /// Traversal depth with --from.
        #[arg(long, default_value_t = 3)]
        depth: usize,
        /// Maximum edges to emit.
        #[arg(long, default_value_t = 800)]
        limit: usize,
        /// Group nodes into clusters.
        #[arg(long, value_enum, default_value_t = GraphGroupArg::None)]
        group_by: GraphGroupArg,
        /// Hide test files.
        #[arg(long)]
        collapse_tests: bool,
        /// Only edges between exported symbols.
        #[arg(long)]
        exported_only: bool,
        /// Also write a self-contained HTML preview.
        #[arg(long, value_name = "FILE")]
        html_out: Option<PathBuf>,
    },
    /// Build or refresh the index. Queries do this automatically.
    Index {
        /// Repository to index (defaults to --root or the discovered root).
        path: Option<PathBuf>,
        /// Rebuild from scratch.
        #[arg(long)]
        full: bool,
        /// Glob (relative to the root) to skip; repeatable. Remembered by later runs.
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude: Vec<String>,
        /// Forget excludes remembered from earlier runs.
        #[arg(long)]
        reset_excludes: bool,
    },
    /// Run as an agent hook: read the event JSON on stdin, answer in the agent's format.
    Hook {
        /// Agent whose output format to use: claude, codex, qwen, copilot, cursor.
        agent: String,
        /// Event: session-start, pre-edit, post-edit, stop (or the agent's own name).
        event: String,
    },
    /// Keep the index fresh while files change.
    Watch {
        /// Wait this long after a change before re-indexing.
        #[arg(long, default_value_t = 200)]
        debounce_ms: u64,
        /// Index once and exit.
        #[arg(long)]
        once: bool,
        /// Rebuild from scratch on each pass.
        #[arg(long)]
        full: bool,
    },
    /// Run the MCP server (stdio by default).
    Mcp {
        /// Serve over HTTP on this address instead, e.g. 127.0.0.1:8765.
        #[arg(long, value_name = "ADDR")]
        http: Option<String>,
    },
    /// Print a shell completion script.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, clap::Args)]
struct PageArgs {
    /// Maximum results per page.
    #[arg(long, default_value_t = 50)]
    limit: usize,
    /// Continue from a previous page's cursor.
    #[arg(long)]
    cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GraphFormat {
    Mermaid,
    Dot,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GraphGroupArg {
    None,
    File,
    Directory,
    Language,
}

impl From<GraphGroupArg> for ExportGroupBy {
    fn from(value: GraphGroupArg) -> Self {
        match value {
            GraphGroupArg::None => ExportGroupBy::None,
            GraphGroupArg::File => ExportGroupBy::File,
            GraphGroupArg::Directory => ExportGroupBy::Directory,
            GraphGroupArg::Language => ExportGroupBy::Language,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum VerifyFormat {
    Human,
    Agent,
    Json,
    Sarif,
    /// A pull-request comment: findings, callers, contracts, tests to run.
    Markdown,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MinConfidence {
    /// Resolved through scope, an import, `this`, a typed receiver or a static class.
    High,
    /// Also unique name matches.
    Medium,
    /// Also one of several same-named candidates.
    Low,
}

impl From<MinConfidence> for Confidence {
    fn from(level: MinConfidence) -> Self {
        match level {
            MinConfidence::High => Confidence::High,
            MinConfidence::Medium => Confidence::Medium,
            MinConfidence::Low => Confidence::Low,
        }
    }
}

#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn usage(message: &str) -> anyhow::Error {
    UsageError(message.to_string()).into()
}

fn main() -> ExitCode {
    // Deeply nested sources need more stack than Windows' 1 MiB main thread.
    let result = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .map_err(anyhow::Error::from)
        .and_then(|handle| {
            handle
                .join()
                .map_err(|_| anyhow!("tessera main thread panicked"))?
        });
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) if error.downcast_ref::<UsageError>().is_some() => {
            eprintln!("error: {error}");
            ExitCode::from(EXIT_USAGE)
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(EXIT_INDEX)
        }
    }
}

fn run() -> Result<u8> {
    let cli = Cli::parse();
    if let Commands::Hook { agent, event } = &cli.command {
        return Ok(run_hook(agent, event, &cli));
    }
    let cwd = std::env::current_dir()?;
    let ws = Workspace::resolve(cli.root.as_deref(), cli.db.as_deref(), &cwd);
    let needs_root = !matches!(cli.command, Commands::Completions { .. });
    if needs_root && cli.root.is_none() && ws.is_too_broad() {
        return Err(usage(&format!(
            "{} is your home directory or the filesystem root, not a project; run tessera inside a repository or pass --root <dir>",
            ws.root.display()
        )));
    }
    let out = Output {
        json: cli.json,
        quiet: cli.quiet,
    };

    match cli.command {
        Commands::Init {
            yes,
            dry_run,
            agents,
            global,
            with_instructions,
            git_hook,
        } => {
            if let Some(unknown) = agents
                .iter()
                .find(|id| tessera_codegraph::agents::by_id(id).is_none())
            {
                return Err(usage(&format!(
                    "unknown agent `{unknown}`; known agents: {}",
                    tessera_codegraph::agents::ids().join(", ")
                )));
            }
            let plan = init::plan(&InitOptions {
                root: ws.root.clone(),
                db_path: ws.db_path.clone(),
                agents,
                global,
                with_instructions,
                git_hook,
                home: tessera_codegraph::agents::home_dir(),
            })?;
            return run_init(&plan, yes, dry_run, &out);
        }
        Commands::Status => out.print(&query::status(&ws)?)?,
        Commands::Find {
            symbol,
            path,
            body,
            page,
        } => {
            if symbol.is_none() && path.is_none() {
                return Err(usage("pass a symbol name, or --path <file-or-dir>"));
            }
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.find(&FindArgs {
                symbol,
                path,
                include_body: body,
                limit: page.limit,
                cursor: page.cursor,
            })?)?;
        }
        Commands::Search {
            pattern,
            kinds,
            languages,
            path,
            exported,
            internal,
            unused,
            page,
        } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.search(&SearchArgs {
                pattern,
                kinds,
                languages,
                path,
                exported: (exported || internal).then_some(exported),
                unused,
                limit: page.limit,
                cursor: page.cursor,
            })?)?;
        }
        Commands::Refs {
            target,
            tests,
            min_confidence,
            page,
        } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.refs(&RefsArgs {
                target,
                tests_only: tests,
                min_confidence: min_confidence.map(Confidence::from),
                limit: page.limit,
                cursor: page.cursor,
            })?)?;
        }
        Commands::Blast {
            symbol,
            diff,
            to,
            depth,
            page,
        } => {
            if symbol.is_none() && diff.is_none() {
                return Err(usage("pass a symbol, or --diff <rev>"));
            }
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.blast(&BlastArgs {
                symbol,
                diff_from: diff,
                diff_to: to,
                depth,
                limit: page.limit,
                cursor: page.cursor,
            })?)?;
        }
        Commands::Verify {
            symbol,
            snippet,
            language,
            changed,
            staged,
            diff,
            format,
        } => {
            let changes = changed || staged || diff.is_some();
            if symbol.is_none() && snippet.is_none() && !changes {
                return Err(usage(
                    "pass a symbol, --snippet <file|-> with --language, or --changed",
                ));
            }
            if snippet.is_some() && language.is_none() {
                return Err(usage("--snippet needs --language"));
            }
            let snippet = snippet.map(|source| read_snippet(&source)).transpose()?;
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            if matches!(format, VerifyFormat::Markdown) {
                if !changes {
                    return Err(usage(
                        "--format markdown needs --changed, --staged, or --diff",
                    ));
                }
                let changes = if staged {
                    query::Changes::Staged
                } else if let Some(rev) = diff {
                    query::Changes::Since(rev)
                } else {
                    query::Changes::WorkingTree
                };
                let report = index.report(&changes)?;
                if out.json {
                    out.print_json(&report)?;
                } else {
                    print!("{}", report.markdown());
                }
                return Ok(if report.verify.ok { 0 } else { EXIT_FINDINGS });
            }
            let result = index.verify(&VerifyArgs {
                symbol,
                snippet,
                language: language.map(Language::from),
                changed,
                staged,
                diff_from: diff,
            })?;
            match format {
                _ if out.json => out.print_json(&result)?,
                VerifyFormat::Human => out.print(&result)?,
                VerifyFormat::Agent => println!("{}", result.agent_brief()),
                VerifyFormat::Json => out.print_json(&result)?,
                VerifyFormat::Sarif => {
                    println!("{}", serde_json::to_string_pretty(&result.sarif())?)
                }
                VerifyFormat::Markdown => unreachable!("handled above"),
            }
            if !result.ok {
                return Ok(EXIT_FINDINGS);
            }
        }
        Commands::Map { kind, query } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.map(&query::MapArgs { kind, query })?)?;
        }
        Commands::Ui { port, open, export } => match export {
            Some(dir) => {
                tessera_codegraph::ui::export(&ws, &dir)?;
                if !out.quiet {
                    eprintln!(
                        "Wrote the UI to {}; serve it from any static host.",
                        dir.display()
                    );
                }
            }
            None => tessera_codegraph::ui::serve(&ws, port, open)?,
        },
        Commands::Context { symbol, budget } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.context(&ContextArgs {
                symbol,
                budget_tokens: budget,
            })?)?;
        }
        Commands::Path { from, to, depth } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            out.print(&index.path(&PathArgs { from, to, depth })?)?;
        }
        Commands::Export {
            format,
            from,
            depth,
            limit,
            group_by,
            collapse_tests,
            exported_only,
            html_out,
        } => {
            let Some(index) = open_fresh(&ws, &out)? else {
                return Ok(0);
            };
            let format = match format {
                GraphFormat::Mermaid => "mermaid",
                GraphFormat::Dot => "dot",
            };
            let mut options = ExportOptions::new(format, from.as_deref(), depth, limit);
            options.group_by = group_by.into();
            options.collapse_tests = collapse_tests;
            options.exported_only = exported_only;
            options.html_out = html_out;
            out.print(&index.export(options)?)?;
        }
        Commands::Index {
            path,
            full,
            exclude,
            reset_excludes,
        } => {
            let root = path.map(|p| cwd.join(p)).unwrap_or_else(|| ws.root.clone());
            let report = indexer::index_path_with(
                &root,
                &ws.db_path,
                IndexOptions {
                    full,
                    excludes: (reset_excludes || !exclude.is_empty()).then_some(exclude),
                },
            )?;
            if out.json {
                out.print_json(&IndexSummary::from(&report))?;
            } else {
                print_report(&report, &ws.db_path);
            }
        }
        Commands::Watch {
            debounce_ms,
            once,
            full,
        } => {
            watch::watch_path(
                &ws.root,
                &ws.db_path,
                WatchOptions {
                    debounce: Duration::from_millis(debounce_ms),
                    index_options: IndexOptions {
                        full,
                        excludes: None,
                    },
                    once,
                },
            )?;
        }
        Commands::Mcp { http } => match http {
            Some(addr) => mcp::serve_http(ws, &addr)?,
            None => mcp::serve_stdio(ws)?,
        },
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "tessera",
                &mut std::io::stdout(),
            );
        }
        Commands::Hook { .. } => unreachable!("hooks run before the workspace is resolved"),
    }
    Ok(0)
}

struct Output {
    json: bool,
    quiet: bool,
}

impl Output {
    fn print<T: Serialize + std::fmt::Display>(&self, value: &T) -> Result<()> {
        if self.json {
            self.print_json(value)
        } else {
            print!("{value}");
            Ok(())
        }
    }

    fn print_json<T: Serialize>(&self, value: &T) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    }

    fn progress(&self, message: &str) {
        if !self.quiet {
            eprintln!("{message}");
        }
    }
}

fn run_init(plan: &init::InitPlan, yes: bool, dry_run: bool, out: &Output) -> Result<u8> {
    use std::io::{IsTerminal, Write};
    if !out.json {
        print!("{plan}");
    }
    let apply = if plan.changes.is_empty() || dry_run {
        false
    } else if yes {
        true
    } else if !out.json && std::io::stdin().is_terminal() {
        print!("\nApply these changes? [y/N] ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    } else {
        false
    };
    let applied = if apply {
        Some(init::apply(plan)?)
    } else {
        None
    };
    if out.json {
        let mut value = serde_json::to_value(plan)?;
        value["applied"] = serde_json::Value::Bool(apply);
        if let Some(applied) = &applied {
            value["written"] = serde_json::to_value(&applied.written)?;
            value["replanned"] = serde_json::to_value(&applied.replanned)?;
            value["skipped"] = serde_json::to_value(&applied.skipped)?;
        }
        out.print_json(&value)?;
        return Ok(0);
    }
    let summary = if plan.changes.is_empty() {
        "Nothing to change.".to_string()
    } else if let Some(applied) = &applied {
        let mut summary = format!("Wrote {} file(s).", applied.written.len());
        for path in &applied.replanned {
            summary.push_str(&format!(
                "\n{path} changed after the diff above; Tessera's entry was added to its new content."
            ));
        }
        for (path, why) in &applied.skipped {
            summary.push_str(&format!("\nLeft {path} alone: {why}."));
        }
        summary
    } else if dry_run {
        "Dry run: nothing written.".to_string()
    } else if yes || std::io::stdin().is_terminal() {
        "Nothing written.".to_string()
    } else {
        "Nothing written: re-run with --yes to apply these changes.".to_string()
    };
    println!("\n{summary}");
    Ok(0)
}

/// Hooks always exit 0: a Tessera failure prints nothing and never stops the agent.
fn run_hook(agent: &str, event: &str, cli: &Cli) -> u8 {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let options = HookOptions {
        root: cli.root.clone(),
        db: cli.db.clone(),
        exe: std::env::current_exe().ok(),
    };
    let result = std::panic::catch_unwind(|| hook::run(agent, event, &input, &options));
    let value = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            if !cli.quiet {
                eprintln!("tessera hook: {error:#}");
            }
            hook::fallback(agent, event, &input)
        }
        Err(_) => {
            eprintln!("tessera hook: internal error");
            hook::fallback(agent, event, &input)
        }
    };
    if let Some(value) = value {
        println!("{value}");
    }
    0
}

fn open_fresh(ws: &Workspace, out: &Output) -> Result<Option<Index>> {
    if fresh::check(ws)? != Freshness::Fresh {
        out.progress(&format!("indexing {} …", ws.root.display()));
        match fresh::refresh(ws)? {
            Refresh::Indexed(report) => {
                out.progress(&format!(
                    "indexed {} files, {} symbols in {}ms",
                    report.files_indexed + report.files_reused,
                    report.symbols_indexed,
                    report.elapsed_ms
                ));
                if !report.warnings.is_empty() {
                    out.progress(&format!(
                        "{} file(s) could not be indexed; run `tessera index` to see them",
                        report.warnings.len()
                    ));
                }
            }
            Refresh::Busy => out.progress(
                "another tessera process is indexing; answering from the existing index, which may be slightly behind",
            ),
        }
    }
    let index = Index::open_workspace(ws)?;
    if db::count_files(index.connection())? == 0 {
        let message = format!(
            "No supported source files found under {}.",
            ws.root.display()
        );
        if out.json {
            out.print_json(&serde_json::json!({ "files": 0, "message": message }))?;
        } else {
            println!("{message}");
        }
        return Ok(None);
    }
    Ok(Some(index))
}

fn read_snippet(source: &str) -> Result<String> {
    if source == "-" {
        let mut code = String::new();
        std::io::stdin().read_to_string(&mut code)?;
        Ok(code)
    } else {
        std::fs::read_to_string(source)
            .map_err(|error| anyhow!("cannot read snippet file {source}: {error}"))
    }
}

#[derive(Serialize)]
struct IndexSummary {
    mode: &'static str,
    files_indexed: usize,
    files_reused: usize,
    files_removed: usize,
    files_skipped: usize,
    symbols: usize,
    references: usize,
    elapsed_ms: u128,
    warnings: usize,
    resolve: Option<tessera_codegraph::resolve::ResolveReport>,
}

impl From<&IndexReport> for IndexSummary {
    fn from(report: &IndexReport) -> Self {
        Self {
            mode: mode_name(report.mode),
            files_indexed: report.files_indexed,
            files_reused: report.files_reused,
            files_removed: report.files_removed,
            files_skipped: report.files_skipped,
            symbols: report.symbols_indexed,
            references: report.references_indexed,
            elapsed_ms: report.elapsed_ms,
            warnings: report.warnings.len(),
            resolve: report.resolve.clone(),
        }
    }
}

fn mode_name(mode: IndexMode) -> &'static str {
    match mode {
        IndexMode::Full => "full",
        IndexMode::Incremental => "incremental",
    }
}

fn print_report(report: &IndexReport, db: &Path) {
    println!(
        "[{}] indexed {} files (+{} reused, -{} removed, {} skipped), {} symbols, {} references into {} in {}ms",
        mode_name(report.mode),
        report.files_indexed,
        report.files_reused,
        report.files_removed,
        report.files_skipped,
        report.symbols_indexed,
        report.references_indexed,
        db.display(),
        report.elapsed_ms
    );
    for warning in report.warnings.iter().take(5) {
        eprintln!("warning: {}: {}", warning.path, warning.message);
    }
    if report.warnings.len() > 5 {
        eprintln!("warning: … {} more", report.warnings.len() - 5);
    }
}
