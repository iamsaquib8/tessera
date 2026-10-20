//! # tessera-codegraph
//!
//! Local semantic code graph for AI coding agents. Index a repository into
//! SQLite; ask deterministic questions about
//! symbols, references, blast radius, and hallucinated identifiers.
//!
//! ```no_run
//! use tessera_codegraph::query::BlastArgs;
//! use tessera_codegraph::{Index, IndexOptions};
//!
//! # fn main() -> anyhow::Result<()> {
//! Index::build("./my-repo", "./my-repo/.tessera/tessera.db", IndexOptions::default())?;
//! let idx = Index::open("./my-repo/.tessera/tessera.db")?;
//! let blast = idx.blast(&BlastArgs {
//!     symbol: Some("findById".into()),
//!     ..BlastArgs::default()
//! })?;
//! for caller in blast.callers.iter().take(5) {
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

pub mod config;
pub mod db;
pub mod extract;
pub mod fresh;
pub mod indexer;
pub mod init;
pub mod mcp;
pub mod output;
pub mod query;
pub mod types;
pub mod walk;
pub mod watch;
pub mod workspace;

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;

pub use indexer::{IndexMode, IndexOptions, IndexReport};
pub use query::{
    BlastArgs, BlastResult, ContextArgs, ContextResult, ExportGroupBy, ExportOptions, FindArgs,
    FindResult, PathArgs, RefsArgs, RefsResult, SearchArgs, SearchResults, StatusResult,
    VerifyArgs, VerifyResult,
};
pub use types::{ConnectResult, ExportResult, Language, ReferenceRecord, SymbolRecord};
pub use workspace::Workspace;

/// Read-only handle to a Tessera index.
pub struct Index {
    conn: Connection,
    db_path: PathBuf,
    root: PathBuf,
}

impl Index {
    /// Open an existing index read-only.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let conn = db::open_ro(&db_path)?;
        let root = db::get_meta(&conn, "root")?
            .map(PathBuf::from)
            .unwrap_or_else(|| db_path.parent().unwrap_or(Path::new(".")).to_path_buf());
        Ok(Self {
            conn,
            db_path,
            root,
        })
    }

    /// Build or refresh an index; the library equivalent of `tessera index <root>`.
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn workspace(&self) -> Workspace {
        Workspace {
            root: self.root.clone(),
            db_path: self.db_path.clone(),
        }
    }

    pub fn find(&self, args: &FindArgs) -> Result<FindResult> {
        query::find(&self.conn, &self.root, args)
    }

    pub fn search(&self, args: &SearchArgs) -> Result<SearchResults> {
        query::search(&self.conn, args)
    }

    pub fn refs(&self, args: &RefsArgs) -> Result<RefsResult> {
        query::refs(&self.conn, args)
    }

    pub fn blast(&self, args: &BlastArgs) -> Result<BlastResult> {
        query::blast(&self.conn, args)
    }

    pub fn path(&self, args: &PathArgs) -> Result<ConnectResult> {
        query::path(&self.conn, args)
    }

    pub fn context(&self, args: &ContextArgs) -> Result<ContextResult> {
        query::context(&self.conn, &self.root, args)
    }

    pub fn verify(&self, args: &VerifyArgs) -> Result<VerifyResult> {
        query::verify(&self.conn, args)
    }

    pub fn status(&self) -> Result<StatusResult> {
        query::status(&self.workspace())
    }

    pub fn export(&self, options: ExportOptions) -> Result<ExportResult> {
        query::export_conn_with_options(&self.conn, options)
    }
}
