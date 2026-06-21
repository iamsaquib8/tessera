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
    C,
    /// C++. `.h` headers are parsed by the C++ grammar too — it is a strict
    /// superset of C, so it extracts both C and C++ headers correctly.
    Cpp,
    CSharp,
    Ruby,
    Php,
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
            "c" => Some(Self::C),
            // `.h` is parsed with the C++ grammar (a C superset) so headers
            // from both C and C++ projects extract cleanly.
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "h" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "rb" => Some(Self::Ruby),
            "php" | "phtml" => Some(Self::Php),
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
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" | "cc" => Some(Self::Cpp),
            "csharp" | "c#" | "cs" => Some(Self::CSharp),
            "ruby" | "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
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
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Php => "php",
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
pub struct IndexedImport {
    pub source: String,
    pub line: usize,
    pub kind: String,
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
            writeln!(f, "No definitions found.")?;
            return writeln!(
                f,
                "  hint: run `tessera search <name>` for fuzzy lookup, `tessera validate <name>` for near-misses, or re-index with `tessera index . --full` if the DB is stale."
            );
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
            writeln!(f, "No references found.")?;
            return writeln!(
                f,
                "  hint: run `tessera impact <symbol>` for transitive callers, or `tessera search <symbol>` to confirm the indexed name."
            );
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
            writeln!(
                f,
                "  hint: check the path prefix, run `tessera stats`, or re-index with `tessera index . --full`."
            )?;
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
            writeln!(f, "No symbol found.")?;
            return writeln!(
                f,
                "  hint: run `tessera find-definition <symbol>` or `tessera search <symbol>` to find the indexed name."
            );
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
            writeln!(
                f,
                "  hint: run `tessera find-references {}` for direct refs, `tessera validate {}` for near-misses, or re-index if the call graph is stale.",
                self.symbol, self.symbol
            )?;
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
        } else if !self.exists {
            writeln!(
                f,
                "  hint: run `tessera search {}` for broader fuzzy matching, or re-index with `tessera index . --full` if this symbol should exist.",
                self.query
            )?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub symbol: Option<SymbolRecord>,
    pub body: Option<String>,
    pub dependency_signatures: Vec<SignatureLine>,
    pub caller_signatures: Vec<SignatureLine>,
    pub tests: Vec<SymbolRecord>,
    pub budget_tokens: usize,
    pub meta: QueryMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureLine {
    pub qualified_name: String,
    pub kind: String,
    pub path: String,
    pub line: usize,
    pub signature: String,
}

impl Display for ContextPack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(symbol) = &self.symbol else {
            return writeln!(f, "No symbol found.");
        };
        writeln!(
            f,
            "{} {} at {}:{} (budget ~{} tokens)",
            symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line, self.budget_tokens
        )?;
        if let Some(body) = &self.body {
            writeln!(f, "\n# body\n{body}")?;
        }
        if !self.dependency_signatures.is_empty() {
            writeln!(f, "\n# dependencies")?;
            for sig in &self.dependency_signatures {
                writeln!(f, "  {} @ {}:{}", sig.qualified_name, sig.path, sig.line)?;
                if !sig.signature.is_empty() {
                    writeln!(f, "    {}", sig.signature)?;
                }
            }
        }
        if !self.caller_signatures.is_empty() {
            writeln!(f, "\n# callers")?;
            for sig in &self.caller_signatures {
                writeln!(f, "  {} @ {}:{}", sig.qualified_name, sig.path, sig.line)?;
                if !sig.signature.is_empty() {
                    writeln!(f, "    {}", sig.signature)?;
                }
            }
        }
        if !self.tests.is_empty() {
            writeln!(f, "\n# tests")?;
            for test in &self.tests {
                writeln!(
                    f,
                    "  {} @ {}:{}",
                    test.qualified_name, test.path, test.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanQueryResult {
    pub query: String,
    pub inferred_intent: String,
    pub steps: Vec<PlanStep>,
    pub meta: QueryMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: usize,
    pub tool: String,
    pub command: String,
    pub reason: String,
    pub expected_tokens: usize,
}

impl Display for PlanQueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Plan for {:?} ({})", self.query, self.inferred_intent)?;
        for step in &self.steps {
            writeln!(
                f,
                "  {}. {}  ~{} tokens",
                step.order, step.command, step.expected_tokens
            )?;
            writeln!(f, "     {}", step.reason)?;
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditPrepResult {
    pub symbol: String,
    pub validate: ValidateResult,
    pub signature: SignatureResult,
    pub siblings: SiblingsResult,
    pub context: ContextPack,
    pub tests: TestsForResult,
    pub next_steps: Vec<PlanStep>,
    pub meta: QueryMeta,
}

impl Display for EditPrepResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Edit prep for {}", self.symbol)?;
        writeln!(f, "  exists: {}", self.validate.exists)?;
        if let Some(symbol) = &self.signature.symbol {
            writeln!(
                f,
                "  target: {} {} @ {}:{}",
                symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line
            )?;
        }
        writeln!(
            f,
            "  context: body={} deps={} callers={} tests={}",
            self.context.body.is_some(),
            self.context.dependency_signatures.len(),
            self.context.caller_signatures.len(),
            self.tests.tests.len()
        )?;
        if !self.siblings.siblings.is_empty() {
            writeln!(f, "  siblings:")?;
            for sibling in self.siblings.siblings.iter().take(5) {
                writeln!(
                    f,
                    "    {} ({} shared callers)",
                    sibling.qualified_name, sibling.shared_callers
                )?;
            }
        }
        if !self.next_steps.is_empty() {
            writeln!(f, "\nNext steps:")?;
            for step in &self.next_steps {
                writeln!(f, "  {}. {}", step.order, step.command)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffImpactResult {
    pub from_ref: String,
    pub to_ref: String,
    pub changed_files: usize,
    pub changed_symbols: Vec<DiffChangedSymbol>,
    pub impacted: Vec<DiffImpactedSymbol>,
    pub meta: QueryMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffChangedSymbol {
    pub symbol: SymbolRecord,
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffImpactedSymbol {
    pub symbol: SymbolRecord,
    pub via: String,
    pub criticality: f32,
}

impl Display for DiffImpactResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Diff impact {}..{}  ·  {} changed files, {} changed symbols, {} impacted",
            self.from_ref,
            self.to_ref,
            self.changed_files,
            self.changed_symbols.len(),
            self.impacted.len()
        )?;
        if !self.changed_symbols.is_empty() {
            writeln!(f, "\n# changed")?;
            for c in &self.changed_symbols {
                writeln!(
                    f,
                    "  +{:>3} -{:<3} {} @ {}:{}",
                    c.added_lines,
                    c.removed_lines,
                    c.symbol.qualified_name,
                    c.symbol.path,
                    c.symbol.start_line
                )?;
            }
        }
        if !self.impacted.is_empty() {
            writeln!(f, "\n# impacted callers (top {})", self.impacted.len())?;
            for i in &self.impacted {
                writeln!(
                    f,
                    "  score {:>5.1}  {} (via {}) @ {}:{}",
                    i.criticality,
                    i.symbol.qualified_name,
                    i.via,
                    i.symbol.path,
                    i.symbol.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRecord {
    pub source: String,
    pub from_path: String,
    pub line: usize,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportsResult {
    pub path: String,
    pub imports: Vec<ImportRecord>,
    pub meta: QueryMeta,
}

impl Display for ImportsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.imports.is_empty() {
            writeln!(f, "No imports under {}.", self.path)?;
        } else {
            writeln!(f, "Imports for {}", self.path)?;
            for imp in &self.imports {
                writeln!(f, "  {} ({}) @ line {}", imp.source, imp.kind, imp.line)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedByResult {
    pub source: String,
    pub importers: Vec<ImportRecord>,
    pub meta: QueryMeta,
}

impl Display for ImportedByResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.importers.is_empty() {
            writeln!(f, "Nothing imports {}.", self.source)?;
        } else {
            writeln!(
                f,
                "{} is imported by ({}):",
                self.source,
                self.importers.len()
            )?;
            for imp in &self.importers {
                writeln!(f, "  {} @ line {}", imp.from_path, imp.line)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureResult {
    pub symbol: Option<SymbolRecord>,
    pub members: Vec<SignatureLine>,
    pub meta: QueryMeta,
}

impl Display for SignatureResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(symbol) = &self.symbol else {
            return writeln!(f, "No symbol found.");
        };
        writeln!(
            f,
            "{} {} @ {}:{}",
            symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line
        )?;
        if !symbol.signature.is_empty() {
            writeln!(f, "  {}", symbol.signature)?;
        }
        if !self.members.is_empty() {
            writeln!(f, "  members:")?;
            for m in &self.members {
                writeln!(f, "    {:<9} {}", m.kind, m.signature)?;
            }
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiblingsResult {
    pub symbol: String,
    pub siblings: Vec<Sibling>,
    pub meta: QueryMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sibling {
    pub qualified_name: String,
    pub shared_callers: usize,
    pub path: Option<String>,
    pub line: Option<usize>,
}

impl Display for SiblingsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.siblings.is_empty() {
            writeln!(f, "No siblings found for {}.", self.symbol)?;
        } else {
            writeln!(f, "Siblings of {} (by shared callers)", self.symbol)?;
            for s in &self.siblings {
                let where_ = match (&s.path, s.line) {
                    (Some(p), Some(l)) => format!("{p}:{l}"),
                    _ => "—".to_string(),
                };
                writeln!(
                    f,
                    "  {:>3}  {} @ {}",
                    s.shared_callers, s.qualified_name, where_
                )?;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnusedOptions {
    /// Restrict to these symbol kinds (function, method, class, struct, ...).
    /// Empty means "any".
    pub kinds: Vec<String>,
    /// Restrict to these languages (typescript, java, ...). Empty means "any".
    pub languages: Vec<String>,
    /// `Some(true)` = only exported; `Some(false)` = only non-exported.
    pub exported: Option<bool>,
    /// Match symbols whose file path starts with this prefix.
    pub path_prefix: Option<String>,
    /// Maximum number of hits to return.
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnusedSymbol {
    pub symbol: SymbolRecord,
    pub inbound_refs: usize,
    pub inbound_edges: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnusedResult {
    pub symbols: Vec<UnusedSymbol>,
    pub meta: QueryMeta,
}

impl Display for UnusedResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.symbols.is_empty() {
            writeln!(f, "No unused symbols found.")?;
            writeln!(
                f,
                "  hint: relax filters such as `--kind`, `--language`, `--path`, or `--exported=false`."
            )?;
        } else {
            writeln!(f, "Unused symbols ({}):", self.symbols.len())?;
            for unused in &self.symbols {
                let symbol = &unused.symbol;
                writeln!(
                    f,
                    "  {:<9} {} at {}:{}",
                    symbol.kind, symbol.qualified_name, symbol.path, symbol.start_line
                )?;
            }
        }
        write_meta(f, &self.meta)
    }
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
            writeln!(
                f,
                "  hint: try a wider pattern such as `*{}*`, remove filters, or run `tessera validate {}` for near-misses.",
                self.query, self.query
            )?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathNode {
    pub qualified_name: String,
    pub kind: String,
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResult {
    pub from: String,
    pub to: String,
    pub found: bool,
    /// Ordered call path from `from` to `to` (inclusive of both endpoints).
    pub path: Vec<PathNode>,
    pub meta: QueryMeta,
}

impl Display for ConnectResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.found {
            writeln!(
                f,
                "No call path from {} to {} (within search depth).",
                self.from, self.to
            )?;
            return write_meta(f, &self.meta);
        }
        writeln!(
            f,
            "Call path {} → {} ({} hops)",
            self.from,
            self.to,
            self.path.len().saturating_sub(1)
        )?;
        for (i, node) in self.path.iter().enumerate() {
            let arrow = if i == 0 { "  " } else { "  ↳ " };
            writeln!(
                f,
                "{}{} @ {}:{}",
                arrow, node.qualified_name, node.path, node.line
            )?;
        }
        write_meta(f, &self.meta)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub format: String,
    pub scope: String,
    pub nodes: usize,
    pub edges: usize,
    pub truncated: bool,
    /// The rendered graph in the requested format (DOT or Mermaid).
    pub diagram: String,
    pub meta: QueryMeta,
}

impl Display for ExportResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The diagram itself IS the output so it pipes cleanly into
        // `dot`/mermaid. Counts + truncation go to the meta line as a comment.
        write!(f, "{}", self.diagram)
    }
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
