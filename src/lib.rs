//! # tessera-codegraph
//!
//! Local semantic code graph for AI coding agents. Index a repository into
//! SQLite + a memory-mapped snapshot; ask deterministic questions about
//! symbols, references, blast radius, and hallucinated identifiers.
//!
//! ```no_run
//! use tessera_codegraph::{Index, IndexOptions};
//!
//! # fn main() -> anyhow::Result<()> {
//! Index::build("./my-repo", "./my-repo/.tessera/tessera.db", IndexOptions::default())?;
//! let idx = Index::open("./my-repo/.tessera/tessera.db")?;
//! let impact = idx.impact("findById", 4)?;
//! for caller in impact.callers.iter().take(5) {
//!     println!(
//!         "{:5.1}  {} @ {}:{}",
//!         caller.criticality,
//!         caller.symbol.qualified_name,
//!         caller.symbol.path,
//!         caller.symbol.start_line
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! The library binary `tessera` is also built from this crate; see the README.

pub mod bench;
pub mod bloom;
pub mod completions;
pub mod config;
pub mod db;
pub mod doctor;
pub mod engine;
pub mod indexer;
pub mod init;
pub mod mcp;
pub mod mcp_http;
pub mod query;
pub mod snapshot;
pub mod types;
pub mod watch;

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;

pub use indexer::{IndexMode, IndexOptions, IndexReport};
pub use types::{
    AlternativeQuery, BenchQuery, BenchResult, BenchSavings, ContextPack, CriticalityBreakdown,
    DefinitionResult, DiffChangedSymbol, DiffImpactResult, DiffImpactedSymbol, ExpandResult,
    GraphEngineKind, ImpactCaller, ImpactResult, ImportRecord, ImportedByResult, ImportsResult,
    KindCount, Language, LanguageCount, OutlineResult, QueryMeta, ReferenceRecord,
    ReferencesResult, SearchHit, SearchOptions, SearchResult, Sibling, SiblingsResult,
    SignatureLine, SignatureResult, SnippetReferenceCheck, StatsResult, SymbolRecord,
    SymbolSuggestion, TestsForResult, TopFanout, UnusedOptions, UnusedResult, UnusedSymbol,
    ValidateResult, ValidateSnippetResult,
};

/// High-level handle to a Tessera index. Holds a single SQLite connection and,
/// when present, a memory-mapped snapshot for hot-path queries.
pub struct Index {
    conn: Connection,
    db_path: PathBuf,
}

impl Index {
    /// Open an existing index. Returns an error if the database doesn't exist
    /// or can't be migrated to the current schema.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let conn = db::open_existing(&db_path)?;
        Ok(Self { conn, db_path })
    }

    /// Build (or refresh) an index from a repository on disk. This is the
    /// library equivalent of `tessera index <root>`.
    pub fn build(
        root: impl AsRef<Path>,
        db_path: impl AsRef<Path>,
        options: IndexOptions,
    ) -> Result<IndexReport> {
        indexer::index_path_with(root.as_ref(), db_path.as_ref(), options)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn find_definition(&self, symbol: &str) -> Result<DefinitionResult> {
        query::find_definition_conn(&self.conn, symbol)
    }

    pub fn find_references(&self, symbol: &str) -> Result<ReferencesResult> {
        query::find_references_conn(&self.conn, symbol)
    }

    pub fn outline(&self, path: impl AsRef<Path>) -> Result<OutlineResult> {
        query::get_outline_conn(&self.conn, path.as_ref())
    }

    pub fn expand(&self, symbol: &str) -> Result<ExpandResult> {
        query::expand_symbol_conn(&self.conn, symbol)
    }

    pub fn impact(&self, symbol: &str, depth: usize) -> Result<ImpactResult> {
        query::impact_conn(&self.conn, symbol, depth)
    }

    pub fn validate(&self, symbol: &str) -> Result<ValidateResult> {
        query::validate_conn(&self.conn, symbol)
    }

    pub fn validate_snippet(
        &self,
        code: &str,
        language: Language,
    ) -> Result<ValidateSnippetResult> {
        query::validate_snippet_conn(&self.conn, code, language)
    }

    pub fn stats(&self) -> Result<StatsResult> {
        query::stats_conn(&self.conn, &self.db_path)
    }

    pub fn tests_for(&self, symbol: &str) -> Result<TestsForResult> {
        query::tests_for_conn(&self.conn, symbol)
    }

    pub fn search(&self, pattern: &str, options: SearchOptions) -> Result<SearchResult> {
        query::search_conn(&self.conn, pattern, options)
    }

    pub fn unused(&self, options: UnusedOptions) -> Result<UnusedResult> {
        query::unused_conn(&self.conn, options)
    }

    pub fn context_pack(&self, symbol: &str, budget_tokens: usize) -> Result<ContextPack> {
        query::context_pack_conn(&self.conn, symbol, budget_tokens)
    }

    pub fn diff_impact(
        &self,
        from_ref: &str,
        to_ref: Option<&str>,
        depth: usize,
    ) -> Result<DiffImpactResult> {
        query::diff_impact_conn(&self.conn, from_ref, to_ref, depth)
    }

    pub fn imports(&self, path: &str) -> Result<ImportsResult> {
        query::imports_conn(&self.conn, path)
    }

    pub fn imported_by(&self, source: &str) -> Result<ImportedByResult> {
        query::imported_by_conn(&self.conn, source)
    }

    pub fn signature(&self, symbol: &str) -> Result<SignatureResult> {
        query::signature_conn(&self.conn, symbol)
    }

    pub fn siblings(&self, symbol: &str) -> Result<SiblingsResult> {
        query::siblings_conn(&self.conn, symbol)
    }
}
