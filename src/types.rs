use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    JavaScript,
    TypeScript,
    /// TypeScript with JSX — parsed by the TSX grammar so `<Component />`
    /// elements get extracted as references. Stored as "typescript" in the DB.
    Tsx,
    Python,
    Go,
    Rust,
    Java,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "py" | "pyw" => Some(Self::Python),
            "go" => Some(Self::Go),
            "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "javascript" | "js" | "jsx" => Some(Self::JavaScript),
            "typescript" | "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "python" | "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "rust" | "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    /// Logical family — TSX and TS share the "typescript" label in the DB
    /// because callers/agents don't care which sub-grammar parsed the file.
    pub fn family(&self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript | Self::Tsx => "typescript",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
        }
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.family())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEngineKind {
    Sqlite,
    Cozo,
}

impl Display for GraphEngineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite => f.write_str("sqlite"),
            Self::Cozo => f.write_str("cozo"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub id: i64,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub file_id: i64,
    pub path: String,
    pub language: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
    pub exported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceRecord {
    pub id: i64,
    pub symbol_name: String,
    pub from_symbol_id: Option<i64>,
    pub from_symbol: Option<String>,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub context: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedSymbol {
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
    pub exported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedReference {
    pub symbol_name: String,
    pub from_qualified_name: Option<String>,
    pub line: usize,
    pub column: usize,
    pub context: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionResult {
    pub matches: Vec<SymbolRecord>,
    pub meta: QueryMeta,
}

impl Display for DefinitionResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.matches.is_empty() {
            return writeln!(f, "No definitions found.");
        }
        for symbol in &self.matches {
            writeln!(
                f,
                "{} {} at {}:{}",
                symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line
            )?;
            if !symbol.signature.is_empty() {
                writeln!(f, "  {}", symbol.signature)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencesResult {
    pub references: Vec<ReferenceRecord>,
    pub meta: QueryMeta,
}

impl Display for ReferencesResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.references.is_empty() {
            return writeln!(f, "No references found.");
        }
        for reference in &self.references {
            writeln!(
                f,
                "{}:{}:{} {}",
                reference.path, reference.line, reference.column, reference.context
            )?;
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineResult {
    pub path: String,
    pub symbols: Vec<SymbolRecord>,
    pub meta: QueryMeta,
}

impl Display for OutlineResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.symbols.is_empty() {
            writeln!(f, "No indexed symbols under {}.", self.path)?;
        } else {
            writeln!(f, "Outline for {}", self.path)?;
            for symbol in &self.symbols {
                writeln!(
                    f,
                    "  {:<9} {:<36} {}:{}",
                    symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandResult {
    pub symbol: Option<SymbolRecord>,
    pub body: Option<String>,
    pub dependencies: Vec<ReferenceRecord>,
    pub meta: QueryMeta,
}

impl Display for ExpandResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(symbol) = &self.symbol else {
            return writeln!(f, "No symbol found.");
        };
        writeln!(
            f,
            "{} {} at {}:{}-{}",
            symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line, symbol.end_line
        )?;
        if let Some(body) = &self.body {
            writeln!(f, "\n{body}")?;
        }
        if !self.dependencies.is_empty() {
            writeln!(f, "\nImmediate dependencies:")?;
            for dep in &self.dependencies {
                writeln!(f, "  {} at line {}", dep.symbol_name, dep.line)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactResult {
    pub symbol: String,
    pub callers: Vec<ImpactCaller>,
    pub meta: QueryMeta,
}

impl Display for ImpactResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.callers.is_empty() {
            writeln!(f, "No callers found for {}.", self.symbol)?;
        } else {
            writeln!(f, "Impact for {}", self.symbol)?;
            for caller in &self.callers {
                writeln!(
                    f,
                    "  score {:>5.1} depth {} fanout {:>3} {} at {}:{}",
                    caller.criticality,
                    caller.depth,
                    caller.breakdown.fanout_in,
                    caller.symbol.qualified_name,
                    caller.symbol.path,
                    caller.symbol.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactCaller {
    pub symbol: SymbolRecord,
    pub depth: usize,
    pub criticality: f32,
    pub breakdown: CriticalityBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalityBreakdown {
    pub pagerank: f32,
    pub fanout_in: usize,
    pub fanout_out: usize,
    pub exported: bool,
    pub test_coverage: usize,
    pub depth_decay: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMeta {
    pub tokens_returned_estimate: usize,
    pub alternative_queries: Vec<AlternativeQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeQuery {
    pub tool: String,
    pub tokens_estimate: usize,
    pub fidelity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSuggestion {
    pub qualified_name: String,
    pub name: String,
    pub path: String,
    pub line: usize,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResult {
    pub query: String,
    pub exists: bool,
    pub bloom_hit: bool,
    pub candidates: Vec<SymbolSuggestion>,
    pub meta: QueryMeta,
}

impl Display for ValidateResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exists {
            writeln!(f, "✓ {} exists in the graph.", self.query)?;
        } else {
            writeln!(f, "✗ {} not found (bloom={}).", self.query, self.bloom_hit)?;
        }
        if !self.candidates.is_empty() {
            writeln!(f, "Nearest candidates:")?;
            for candidate in &self.candidates {
                writeln!(
                    f,
                    "  {:>4.2}  {} at {}:{}",
                    candidate.confidence, candidate.qualified_name, candidate.path, candidate.line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetReferenceCheck {
    pub symbol_name: String,
    pub line: usize,
    pub column: usize,
    pub exists: bool,
    pub candidates: Vec<SymbolSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateSnippetResult {
    pub language: String,
    pub total_calls: usize,
    pub unresolved_calls: usize,
    pub checks: Vec<SnippetReferenceCheck>,
    pub meta: QueryMeta,
}

impl Display for ValidateSnippetResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Validated {} calls ({} unresolved) in {} snippet.",
            self.total_calls, self.unresolved_calls, self.language
        )?;
        for check in &self.checks {
            let mark = if check.exists { "✓" } else { "✗" };
            writeln!(
                f,
                "  {} {} at line {} col {}",
                mark, check.symbol_name, check.line, check.column
            )?;
            if !check.exists {
                for candidate in check.candidates.iter().take(3) {
                    writeln!(
                        f,
                        "      -> maybe {} ({:.2})",
                        candidate.qualified_name, candidate.confidence
                    )?;
                }
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Restrict to these symbol kinds (function, method, class, struct, …).
    /// Empty means "any".
    pub kinds: Vec<String>,
    /// Restrict to these languages (typescript, java, …). Empty means "any".
    pub languages: Vec<String>,
    /// `Some(true)` = only exported; `Some(false)` = only non-exported.
    pub exported: Option<bool>,
    /// Match symbols whose file path starts with this prefix.
    pub path_prefix: Option<String>,
    /// Maximum number of hits to return.
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub symbol: SymbolRecord,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub meta: QueryMeta,
}

impl Display for SearchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hits.is_empty() {
            writeln!(f, "No symbols matched {:?}.", self.query)?;
        } else {
            writeln!(f, "Search for {:?} ({} hits)", self.query, self.hits.len())?;
            for hit in &self.hits {
                writeln!(
                    f,
                    "  {:>4.2}  {:<9} {:<40} {}:{}",
                    hit.score,
                    hit.symbol.kind,
                    hit.symbol.qualified_name,
                    hit.symbol.path,
                    hit.symbol.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResult {
    pub files: usize,
    pub symbols: usize,
    pub references: usize,
    pub edges: usize,
    pub languages: Vec<LanguageCount>,
    pub kinds: Vec<KindCount>,
    pub top_fanout: Vec<TopFanout>,
    pub db_path: String,
    pub snapshot_present: bool,
    pub meta: QueryMeta,
}

impl Display for StatsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Tessera index at {} (snapshot={})",
            self.db_path, self.snapshot_present
        )?;
        writeln!(
            f,
            "  files={}  symbols={}  references={}  edges={}",
            self.files, self.symbols, self.references, self.edges
        )?;
        if !self.languages.is_empty() {
            write!(f, "  languages: ")?;
            let parts: Vec<String> = self
                .languages
                .iter()
                .map(|lc| format!("{}={}", lc.language, lc.count))
                .collect();
            writeln!(f, "{}", parts.join(", "))?;
        }
        if !self.kinds.is_empty() {
            write!(f, "  kinds: ")?;
            let parts: Vec<String> = self
                .kinds
                .iter()
                .map(|kc| format!("{}={}", kc.kind, kc.count))
                .collect();
            writeln!(f, "{}", parts.join(", "))?;
        }
        if !self.top_fanout.is_empty() {
            writeln!(f, "  top fanout:")?;
            for tf in &self.top_fanout {
                writeln!(f, "    {:>4} {}", tf.callers, tf.qualified_name)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageCount {
    pub language: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindCount {
    pub kind: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopFanout {
    pub qualified_name: String,
    pub callers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestsForResult {
    pub symbol: String,
    pub tests: Vec<SymbolRecord>,
    pub meta: QueryMeta,
}

impl Display for TestsForResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tests.is_empty() {
            writeln!(f, "No tests transitively touch {}.", self.symbol)?;
        } else {
            writeln!(f, "Tests touching {} ({}):", self.symbol, self.tests.len())?;
            for test in &self.tests {
                writeln!(
                    f,
                    "  {} at {}:{}",
                    test.qualified_name, test.path, test.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub path: String,
    pub files: usize,
    pub symbols: usize,
    pub references: usize,
    pub index_full_ms: u128,
    pub index_incremental_ms: u128,
    pub queries: Vec<BenchQuery>,
    pub savings: Vec<BenchSavings>,
    pub chart: String,
}

impl Display for BenchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.chart)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchQuery {
    pub name: String,
    pub ms: u128,
    pub tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSavings {
    pub label: String,
    pub raw_tokens: usize,
    pub tessera_tokens: usize,
    pub ratio: f32,
}

fn write_meta(f: &mut fmt::Formatter<'_>, meta: &QueryMeta) -> fmt::Result {
    writeln!(
        f,
        "\n_meta: ~{} tokens returned; alternatives: {}",
        meta.tokens_returned_estimate,
        meta.alternative_queries
            .iter()
            .map(|query| format!(
                "{} (~{} tokens, {:.2} fidelity)",
                query.tool, query.tokens_estimate, query.fidelity
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
