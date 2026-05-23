use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser};
use walkdir::{DirEntry, WalkDir};

use crate::db;
use crate::types::{IndexedReference, IndexedSymbol, Language};

#[derive(Debug, Clone)]
pub struct IndexReport {
    pub files_indexed: usize,
    pub symbols_indexed: usize,
    pub references_indexed: usize,
    pub elapsed_ms: u128,
}

pub fn index_path(root: &Path, db_path: &Path) -> Result<IndexReport> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let conn = db::open(db_path)?;
    db::reset(&conn)?;
    db::set_meta(&conn, "root", &root.to_string_lossy())?;

    let mut report = IndexReport {
        files_indexed: 0,
        symbols_indexed: 0,
        references_indexed: 0,
        elapsed_ms: 0,
    };

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(should_enter)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let Some(language) = language_for_path(path) else {
            continue;
        };
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let rel_path = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let parsed = parse_file(language, &content)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let sha = format!("{:x}", Sha256::digest(content.as_bytes()));
        let file_id = db::insert_file(&conn, &rel_path, language, &sha, content.lines().count())?;
        let symbol_ids = db::insert_symbols(&conn, file_id, &parsed.symbols)?;
        let ref_count = db::insert_references(&conn, file_id, &parsed.references)?;

        report.files_indexed += 1;
        report.symbols_indexed += symbol_ids.len();
        report.references_indexed += ref_count;
    }

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn should_enter(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git"
            | ".hg"
            | ".svn"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".tessera"
    )
}

fn language_for_path(path: &Path) -> Option<Language> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(Language::from_extension)
}

#[derive(Debug)]
struct ParsedFile {
    symbols: Vec<IndexedSymbol>,
    references: Vec<IndexedReference>,
}

fn parse_file(language: Language, content: &str) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    match language {
        Language::JavaScript => parser.set_language(tree_sitter_javascript::language())?,
        Language::TypeScript => {
            parser.set_language(tree_sitter_typescript::language_typescript())?
        }
        Language::Python => parser.set_language(tree_sitter_python::language())?,
    }
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("tree-sitter returned no parse tree"))?;
    let mut visitor = Visitor::new(language, content);
    visitor.walk(tree.root_node(), None);
    Ok(ParsedFile {
        symbols: visitor.symbols,
        references: visitor.references,
    })
}

struct Visitor<'a> {
    language: Language,
    content: &'a str,
    lines: Vec<&'a str>,
    scope: Vec<ScopeFrame>,
    symbols: Vec<IndexedSymbol>,
    references: Vec<IndexedReference>,
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    name: String,
    end_byte: usize,
}

impl<'a> Visitor<'a> {
    fn new(language: Language, content: &'a str) -> Self {
        Self {
            language,
            content,
            lines: content.lines().collect(),
            scope: Vec::new(),
            symbols: Vec::new(),
            references: Vec::new(),
        }
    }

    fn walk(&mut self, node: Node<'a>, pending_export: Option<bool>) {
        while self
            .scope
            .last()
            .map(|scope| node.start_byte() >= scope.end_byte)
            .unwrap_or(false)
        {
            self.scope.pop();
        }

        if node.kind() == "export_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk(child, Some(true));
            }
            return;
        }

        if let Some(symbol) = self.symbol_from_node(node, pending_export.unwrap_or(false)) {
            let qname = symbol.qualified_name.clone();
            let end_byte = node.end_byte();
            self.symbols.push(symbol);
            self.scope.push(ScopeFrame {
                name: qname,
                end_byte,
            });
        }

        if let Some(reference) = self.reference_from_node(node) {
            self.references.push(reference);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child, None);
        }
    }

    fn symbol_from_node(&self, node: Node<'a>, exported: bool) -> Option<IndexedSymbol> {
        match self.language {
            Language::Python => self.python_symbol_from_node(node),
            Language::JavaScript | Language::TypeScript => {
                self.js_ts_symbol_from_node(node, exported)
            }
        }
    }

    fn python_symbol_from_node(&self, node: Node<'a>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "function_definition" => "function",
            "class_definition" => "class",
            _ => return None,
        };
        let name_node = node.child_by_field_name("name")?;
        let name = self.node_text(name_node);
        let signature = self.signature_until_body(node);
        Some(self.make_symbol(name, kind, node, signature, true))
    }

    fn js_ts_symbol_from_node(&self, node: Node<'a>, exported: bool) -> Option<IndexedSymbol> {
        let (name, kind) = match node.kind() {
            "function_declaration" | "generator_function_declaration" => (
                self.node_text(node.child_by_field_name("name")?),
                "function",
            ),
            "class_declaration" => (self.node_text(node.child_by_field_name("name")?), "class"),
            "method_definition" | "method_signature" => {
                (self.node_text(node.child_by_field_name("name")?), "method")
            }
            "variable_declarator" => {
                let value = node.child_by_field_name("value")?;
                if !matches!(
                    value.kind(),
                    "arrow_function" | "function" | "function_expression"
                ) {
                    return None;
                }
                (
                    self.node_text(node.child_by_field_name("name")?),
                    "function",
                )
            }
            _ => return None,
        };
        let signature = self.signature_until_body(node);
        Some(self.make_symbol(
            name,
            kind,
            node,
            signature,
            exported || self.has_export_ancestor(node),
        ))
    }

    fn make_symbol(
        &self,
        name: String,
        kind: &str,
        node: Node<'a>,
        signature: String,
        exported: bool,
    ) -> IndexedSymbol {
        let qualified_name = self
            .scope
            .last()
            .map(|parent| format!("{}.{}", parent.name, name))
            .unwrap_or_else(|| name.clone());
        IndexedSymbol {
            name,
            qualified_name,
            kind: kind.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature,
            exported,
        }
    }

    fn reference_from_node(&self, node: Node<'a>) -> Option<IndexedReference> {
        let function_node = match (self.language, node.kind()) {
            (Language::Python, "call") => node.child_by_field_name("function")?,
            (Language::JavaScript | Language::TypeScript, "call_expression") => {
                node.child_by_field_name("function")?
            }
            _ => return None,
        };
        let symbol_name = self.called_name(function_node)?;
        if symbol_name == "require" || symbol_name == "super" {
            return None;
        }
        Some(IndexedReference {
            symbol_name,
            from_qualified_name: self.scope.last().map(|scope| scope.name.clone()),
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            context: self
                .lines
                .get(node.start_position().row)
                .map(|line| line.trim().to_string())
                .unwrap_or_default(),
            kind: "call".to_string(),
        })
    }

    fn called_name(&self, node: Node<'a>) -> Option<String> {
        match node.kind() {
            "identifier" | "property_identifier" => Some(self.node_text(node)),
            "attribute" => node
                .child_by_field_name("attribute")
                .map(|child| self.node_text(child))
                .or_else(|| Some(last_identifier(self.node_text(node)))),
            "member_expression" => node
                .child_by_field_name("property")
                .map(|child| self.node_text(child))
                .or_else(|| Some(last_identifier(self.node_text(node)))),
            "subscript" => node
                .child_by_field_name("value")
                .and_then(|child| self.called_name(child)),
            _ => Some(last_identifier(self.node_text(node))).filter(|name| !name.is_empty()),
        }
    }

    fn node_text(&self, node: Node<'a>) -> String {
        node.utf8_text(self.content.as_bytes())
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn signature_until_body(&self, node: Node<'a>) -> String {
        let text = self.node_text(node);
        let first_line = text.lines().next().unwrap_or_default().trim();
        if first_line.len() <= 180 {
            first_line.to_string()
        } else {
            format!("{}...", &first_line[..180])
        }
    }

    fn has_export_ancestor(&self, node: Node<'a>) -> bool {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if current.kind() == "export_statement" {
                return true;
            }
            parent = current.parent();
        }
        false
    }
}

fn last_identifier(text: String) -> String {
    text.rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .find(|part| !part.is_empty())
        .unwrap_or_default()
        .to_string()
}

#[allow(dead_code)]
fn _normalize(path: &Path) -> PathBuf {
    path.components().collect()
}
