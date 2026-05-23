use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    JavaScript,
    TypeScript,
    Python,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "tsx" | "mts" | "cts" => Some(Self::TypeScript),
            "py" | "pyw" => Some(Self::Python),
            _ => None,
        }
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JavaScript => f.write_str("javascript"),
            Self::TypeScript => f.write_str("typescript"),
            Self::Python => f.write_str("python"),
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
                    "  score {:>4} depth {} fanout {:>3} {} at {}:{}",
                    caller.criticality,
                    caller.depth,
                    caller.fanout,
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
    pub fanout: usize,
    pub criticality: usize,
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
