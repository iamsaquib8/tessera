use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Parser};
use walkdir::{DirEntry, WalkDir};

use crate::bloom::BloomFilter;
use crate::db;
use crate::snapshot;
use crate::types::{IndexedReference, IndexedSymbol, Language};

#[derive(Debug, Clone)]
pub struct IndexReport {
    pub files_indexed: usize,
    pub files_reused: usize,
    pub files_removed: usize,
    pub symbols_indexed: usize,
    pub references_indexed: usize,
    pub elapsed_ms: u128,
    pub mode: IndexMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexMode {
    Full,
    Incremental,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexOptions {
    pub full: bool,
    pub build_snapshot: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            full: false,
            build_snapshot: true,
        }
    }
}

pub fn index_path(root: &Path, db_path: &Path) -> Result<IndexReport> {
    index_path_with(root, db_path, IndexOptions::default())
}

pub fn index_path_with(root: &Path, db_path: &Path, options: IndexOptions) -> Result<IndexReport> {
    let started = Instant::now();
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let conn = db::open(db_path)?;
    if options.full {
        db::reset(&conn)?;
    }
    db::set_meta(&conn, "root", &root.to_string_lossy())?;

    let mode = if options.full {
        IndexMode::Full
    } else {
        IndexMode::Incremental
    };

    let mut report = IndexReport {
        files_indexed: 0,
        files_reused: 0,
        files_removed: 0,
        symbols_indexed: 0,
        references_indexed: 0,
        elapsed_ms: 0,
        mode,
    };

    let mut visited_ids: Vec<i64> = Vec::new();

    let tx = conn.unchecked_transaction()?;

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
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // binary or unreadable; skip silently
        };
        let rel_path = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let sha = format!("{:x}", Sha256::digest(content.as_bytes()));

        // Incremental: short-circuit when sha is unchanged.
        if !options.full {
            if let Some((existing_id, existing_sha)) = db::file_sha(&tx, &rel_path)? {
                if existing_sha == sha {
                    visited_ids.push(existing_id);
                    report.files_reused += 1;
                    continue;
                }
                db::delete_file_cascade(&tx, existing_id)?;
            }
        }

        let parsed = parse_file(language, &content)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let file_id = db::insert_file(&tx, &rel_path, language, &sha, content.lines().count())?;
        let symbol_ids = db::insert_symbols(&tx, file_id, &parsed.symbols)?;
        let ref_count = db::insert_references(&tx, file_id, &parsed.references)?;

        visited_ids.push(file_id);
        report.files_indexed += 1;
        report.symbols_indexed += symbol_ids.len();
        report.references_indexed += ref_count;
    }

    // Reconcile deletes only in incremental mode (full mode already reset).
    if !options.full {
        report.files_removed = db::delete_files_not_in(&tx, &visited_ids)?;
    }

    tx.commit()?;

    rebuild_bloom(&conn)?;

    if options.build_snapshot {
        let snapshot_path = snapshot_path(db_path);
        let _ = snapshot::build(&conn, &snapshot_path);
    }

    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn snapshot_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("snapshot.bin")
}

fn rebuild_bloom(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT name, qualified_name FROM symbols")?;
    let mut names: Vec<String> = Vec::new();
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let qualified: String = row.get(1)?;
        Ok((name, qualified))
    })?;
    for row in rows {
        let (name, qualified) = row?;
        names.push(name);
        names.push(qualified);
    }
    let expected = names.len().max(64);
    let mut bloom = BloomFilter::for_expected(expected, 0.01);
    for name in &names {
        bloom.insert(name);
    }
    let bytes = bloom.to_bytes();
    db::set_meta_blob(conn, "bloom_symbols", &bytes)?;
    Ok(())
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
pub struct ParsedFile {
    pub symbols: Vec<IndexedSymbol>,
    pub references: Vec<IndexedReference>,
}

pub fn parse_file(language: Language, content: &str) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    match language {
        Language::JavaScript => parser.set_language(tree_sitter_javascript::language())?,
        // TSX grammar is a superset of TS — using it for both means a plain
        // `.ts` file still parses cleanly *and* JSX-bearing `.tsx` files yield
        // jsx_element nodes for React-component reference extraction.
        Language::TypeScript | Language::Tsx => {
            parser.set_language(tree_sitter_typescript::language_tsx())?
        }
        Language::Python => parser.set_language(tree_sitter_python::language())?,
        Language::Go => parser.set_language(tree_sitter_go::language())?,
        Language::Rust => parser.set_language(tree_sitter_rust::language())?,
        Language::Java => parser.set_language(tree_sitter_java::language())?,
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

fn is_js_family(lang: Language) -> bool {
    matches!(
        lang,
        Language::JavaScript | Language::TypeScript | Language::Tsx
    )
}

struct Visitor<'a> {
    language: Language,
    content: &'a str,
    lines: Vec<&'a str>,
    scope: Vec<ScopeFrame>,
    symbols: Vec<IndexedSymbol>,
    references: Vec<IndexedReference>,
    skip_refs: HashSet<&'static str>,
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    name: String,
    end_byte: usize,
}

impl<'a> Visitor<'a> {
    fn new(language: Language, content: &'a str) -> Self {
        let skip_refs: HashSet<&'static str> = match language {
            Language::JavaScript | Language::TypeScript | Language::Tsx => {
                ["super"].into_iter().collect()
            }
            Language::Python => HashSet::new(),
            Language::Go => HashSet::new(),
            Language::Rust => [
                "println",
                "eprintln",
                "format",
                "vec",
                "assert",
                "assert_eq",
            ]
            .into_iter()
            .collect(),
            Language::Java => ["super", "this"].into_iter().collect(),
        };
        Self {
            language,
            content,
            lines: content.lines().collect(),
            scope: Vec::new(),
            symbols: Vec::new(),
            references: Vec::new(),
            skip_refs,
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

        if is_js_family(self.language) && node.kind() == "export_statement" {
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
            Language::JavaScript | Language::TypeScript | Language::Tsx => {
                self.js_ts_symbol_from_node(node, exported)
            }
            Language::Go => self.go_symbol_from_node(node),
            Language::Rust => self.rust_symbol_from_node(node),
            Language::Java => self.java_symbol_from_node(node),
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

    fn go_symbol_from_node(&self, node: Node<'a>) -> Option<IndexedSymbol> {
        match node.kind() {
            "function_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                let name = self.node_text(name_node);
                let signature = self.signature_until_body(node);
                let exported = is_go_exported(&name);
                Some(self.make_symbol(name, "function", node, signature, exported))
            }
            "method_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                let raw_name = self.node_text(name_node);
                let receiver_type = node
                    .child_by_field_name("receiver")
                    .and_then(|recv| self.go_receiver_type(recv));
                let qualified = receiver_type
                    .as_ref()
                    .map(|t| format!("{}.{}", t, raw_name))
                    .unwrap_or_else(|| raw_name.clone());
                let signature = self.signature_until_body(node);
                let exported = is_go_exported(&raw_name);
                Some(IndexedSymbol {
                    name: raw_name,
                    qualified_name: qualified,
                    kind: "method".to_string(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    signature,
                    exported,
                })
            }
            "type_spec" => {
                let name_node = node.child_by_field_name("name")?;
                let name = self.node_text(name_node);
                let type_node = node.child_by_field_name("type")?;
                let kind = match type_node.kind() {
                    "struct_type" => "struct",
                    "interface_type" => "interface",
                    _ => "type",
                };
                let signature = self.signature_until_body(node);
                let exported = is_go_exported(&name);
                Some(self.make_symbol(name, kind, node, signature, exported))
            }
            _ => None,
        }
    }

    fn go_receiver_type(&self, recv: Node<'a>) -> Option<String> {
        // receiver is parameter_list; walk for the first identifier inside a type
        let mut cursor = recv.walk();
        for child in recv.children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                if let Some(type_node) = child.child_by_field_name("type") {
                    return Some(self.unwrap_go_type(type_node));
                }
            }
        }
        None
    }

    fn unwrap_go_type(&self, node: Node<'a>) -> String {
        match node.kind() {
            "pointer_type" => {
                if let Some(inner) = node.named_child(0) {
                    self.unwrap_go_type(inner)
                } else {
                    self.node_text(node)
                }
            }
            "type_identifier" | "identifier" | "qualified_type" => self.node_text(node),
            _ => self.node_text(node),
        }
    }

    fn java_symbol_from_node(&self, node: Node<'a>) -> Option<IndexedSymbol> {
        let (name, kind) = match node.kind() {
            "class_declaration" => (self.node_text(node.child_by_field_name("name")?), "class"),
            "interface_declaration" => (
                self.node_text(node.child_by_field_name("name")?),
                "interface",
            ),
            "enum_declaration" => (self.node_text(node.child_by_field_name("name")?), "enum"),
            "record_declaration" => (self.node_text(node.child_by_field_name("name")?), "record"),
            "annotation_type_declaration" => (
                self.node_text(node.child_by_field_name("name")?),
                "annotation",
            ),
            "method_declaration" => (self.node_text(node.child_by_field_name("name")?), "method"),
            "constructor_declaration" => (
                self.node_text(node.child_by_field_name("name")?),
                "constructor",
            ),
            _ => return None,
        };
        let signature = self.signature_until_body(node);
        let exported = java_is_public(node);
        Some(self.make_symbol(name, kind, node, signature, exported))
    }

    fn rust_symbol_from_node(&self, node: Node<'a>) -> Option<IndexedSymbol> {
        let (name, kind) = match node.kind() {
            "function_item" => (
                self.node_text(node.child_by_field_name("name")?),
                "function",
            ),
            "struct_item" => (self.node_text(node.child_by_field_name("name")?), "struct"),
            "enum_item" => (self.node_text(node.child_by_field_name("name")?), "enum"),
            "trait_item" => (self.node_text(node.child_by_field_name("name")?), "trait"),
            "mod_item" => (self.node_text(node.child_by_field_name("name")?), "module"),
            "impl_item" => {
                let type_node = node.child_by_field_name("type")?;
                let name = self.node_text(type_node);
                let signature = self.signature_until_body(node);
                return Some(self.make_symbol(name, "impl", node, signature, false));
            }
            _ => return None,
        };
        let signature = self.signature_until_body(node);
        let exported = rust_is_pub(node);
        Some(self.make_symbol(name, kind, node, signature, exported))
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
        let kind = node.kind();
        // JSX is its own case because the "callable" isn't a function in the
        // grammar — it's the element name in <Component … />. Only register
        // names that look like React components (leading uppercase or
        // member-expression like `Foo.Bar`); lowercase identifiers are intrinsic
        // HTML elements and would just be noise.
        if is_js_family(self.language)
            && matches!(kind, "jsx_self_closing_element" | "jsx_opening_element")
        {
            let name_node = node.child_by_field_name("name")?;
            let symbol_name = self.jsx_component_name(name_node)?;
            return Some(IndexedReference {
                symbol_name,
                from_qualified_name: self.scope.last().map(|scope| scope.name.clone()),
                line: node.start_position().row + 1,
                column: node.start_position().column + 1,
                context: self
                    .lines
                    .get(node.start_position().row)
                    .map(|line| line.trim().to_string())
                    .unwrap_or_default(),
                kind: "jsx".to_string(),
            });
        }

        let (function_node, ref_kind) = match (self.language, kind) {
            (Language::Python, "call") => (node.child_by_field_name("function")?, "call"),
            (Language::JavaScript | Language::TypeScript | Language::Tsx, "call_expression") => {
                (node.child_by_field_name("function")?, "call")
            }
            (Language::Go, "call_expression") => (node.child_by_field_name("function")?, "call"),
            (Language::Rust, "call_expression") => (node.child_by_field_name("function")?, "call"),
            (Language::Rust, "macro_invocation") => (node.child_by_field_name("macro")?, "macro"),
            (Language::Java, "method_invocation") => (node.child_by_field_name("name")?, "call"),
            (Language::Java, "object_creation_expression") => {
                (node.child_by_field_name("type")?, "new")
            }
            _ => return None,
        };
        let symbol_name = self.called_name(function_node)?;
        if self.skip_refs.contains(symbol_name.as_str()) {
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
            kind: ref_kind.to_string(),
        })
    }

    fn jsx_component_name(&self, name_node: Node<'a>) -> Option<String> {
        match name_node.kind() {
            "identifier" => {
                let text = self.node_text(name_node);
                let first = text.chars().next()?;
                if first.is_ascii_uppercase() || first == '_' {
                    Some(text)
                } else {
                    None
                }
            }
            "nested_identifier" | "member_expression" => {
                // <Foo.Bar /> — record the rightmost segment as the component.
                let last = name_node
                    .child_by_field_name("property")
                    .or_else(|| name_node.child_by_field_name("name"))
                    .or_else(|| {
                        name_node.named_child(name_node.named_child_count().saturating_sub(1))
                    })?;
                Some(self.node_text(last))
            }
            _ => {
                let text = self.node_text(name_node);
                let first = text.chars().next()?;
                if first.is_ascii_uppercase() {
                    Some(text)
                } else {
                    None
                }
            }
        }
    }

    fn called_name(&self, node: Node<'a>) -> Option<String> {
        match node.kind() {
            "identifier" | "property_identifier" | "type_identifier" => Some(self.node_text(node)),
            "attribute" => node
                .child_by_field_name("attribute")
                .map(|child| self.node_text(child))
                .or_else(|| Some(last_identifier(self.node_text(node)))),
            "member_expression" | "selector_expression" => node
                .child_by_field_name("property")
                .or_else(|| node.child_by_field_name("field"))
                .map(|child| self.node_text(child))
                .or_else(|| Some(last_identifier(self.node_text(node)))),
            "field_expression" => node
                .child_by_field_name("field")
                .map(|child| self.node_text(child))
                .or_else(|| Some(last_identifier(self.node_text(node)))),
            "scoped_identifier" => node
                .child_by_field_name("name")
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

fn is_go_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn rust_is_pub(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            return true;
        }
    }
    false
}

fn java_is_public(node: Node<'_>) -> bool {
    // Java visibility lives in a `modifiers` child. In tree-sitter-java the
    // `public` modifier shows up as its own typed child node, so we don't need
    // the source bytes — walking the modifier subtree by kind() is enough.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "modifiers" {
            continue;
        }
        let mut inner = child.walk();
        for modifier in child.children(&mut inner) {
            if modifier.kind() == "public" {
                return true;
            }
        }
    }
    false
}

fn last_identifier(text: String) -> String {
    text.rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .find(|part| !part.is_empty())
        .unwrap_or_default()
        .to_string()
}
