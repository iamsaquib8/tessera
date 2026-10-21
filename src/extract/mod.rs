use anyhow::{anyhow, Result};
use tree_sitter::{Node, Parser};

use crate::types::{
    IndexedBinding, IndexedImport, IndexedReference, IndexedSymbol, IndexedTypeHint, Language,
    Receiver,
};

mod lang;

const SIGNATURE_MAX_CHARS: usize = 180;
/// Receiver chains longer than this are not followed.
const MAX_MEMBER_PATH: usize = 4;

#[derive(Debug, Default)]
pub struct ParsedFile {
    pub symbols: Vec<IndexedSymbol>,
    pub references: Vec<IndexedReference>,
    pub imports: Vec<IndexedImport>,
    pub bindings: Vec<IndexedBinding>,
    pub hints: Vec<IndexedTypeHint>,
    pub module: Option<String>,
    pub has_errors: bool,
    /// Contract facts; empty unless parsed with [`parse_file_at`].
    pub facts: Vec<crate::facts::IndexedFact>,
}

pub trait LanguageSupport: Sync {
    fn language(&self) -> Language;
    fn grammar(&self) -> tree_sitter::Language;
    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol>;
    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference>;
    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport>;
    fn skip_ref_names(&self) -> &'static [&'static str] {
        &[]
    }
    fn references(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedReference>) {
        if let Some(reference) = self.reference(cx, node) {
            out.push(reference);
        }
    }
    fn bindings(&self, _cx: &Cx<'_>, _node: Node<'_>, _out: &mut Vec<IndexedBinding>) {}
    fn hints(&self, _cx: &Cx<'_>, _node: Node<'_>, _out: &mut Vec<IndexedTypeHint>) {}
    fn module(&self, _cx: &Cx<'_>, _node: Node<'_>) -> Option<String> {
        None
    }
}

pub fn support_for(language: Language) -> &'static dyn LanguageSupport {
    lang::support_for(language)
}

/// The arguments in a call's argument list, comments aside; `None` for a
/// tagged template, whose "arguments" are a template string.
pub fn argument_count(args: Node<'_>) -> Option<u16> {
    if args.kind() == "template_string" {
        return None;
    }
    let mut cursor = args.walk();
    let count = args
        .named_children(&mut cursor)
        .filter(|arg| !arg.is_extra())
        .count();
    Some(count as u16)
}

pub fn parse_file(language: Language, content: &str) -> Result<ParsedFile> {
    Ok(parse(language, content)?.0)
}

/// Parses an indexed file: symbols and references, plus its contract facts,
/// some of which depend on where the file lives.
pub fn parse_file_at(language: Language, path: &str, content: &str) -> Result<ParsedFile> {
    if language.is_data() {
        return Ok(ParsedFile {
            facts: crate::facts::extract(language, path, content, None),
            ..ParsedFile::default()
        });
    }
    let (mut out, tree) = parse(language, content)?;
    out.facts = crate::facts::extract(language, path, content, Some(&tree));
    Ok(out)
}

fn parse(language: Language, content: &str) -> Result<(ParsedFile, tree_sitter::Tree)> {
    let support = support_for(language);
    let mut parser = Parser::new();
    parser.set_language(&support.grammar())?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("tree-sitter returned no parse tree"))?;
    let mut out = ParsedFile {
        symbols: Vec::new(),
        references: Vec::new(),
        imports: Vec::new(),
        bindings: Vec::new(),
        hints: Vec::new(),
        module: None,
        has_errors: tree.root_node().has_error(),
        facts: Vec::new(),
    };
    let mut cx = Cx::new(language, content);
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        cx.leave_scopes_before(node.start_byte());
        if let Some(symbol) = support.symbol(&cx, node) {
            cx.scope.push(ScopeFrame {
                name: symbol.qualified_name.clone(),
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                index: out.symbols.len(),
            });
            out.symbols.push(symbol);
        }
        let mut found = Vec::new();
        support.references(&cx, node, &mut found);
        let skip = support.skip_ref_names();
        out.references.extend(
            found
                .into_iter()
                .filter(|r| !skip.contains(&r.symbol_name.as_str())),
        );
        if let Some(import) = support.import(&cx, node) {
            out.imports.push(import);
        }
        support.bindings(&cx, node, &mut out.bindings);
        support.hints(&cx, node, &mut out.hints);
        if out.module.is_none() {
            out.module = support.module(&cx, node);
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                drop(cursor);
                return Ok((out, tree));
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    name: String,
    start_byte: usize,
    end_byte: usize,
    index: usize,
}

pub struct Cx<'a> {
    pub language: Language,
    content: &'a str,
    lines: Vec<&'a str>,
    scope: Vec<ScopeFrame>,
}

impl<'a> Cx<'a> {
    fn new(language: Language, content: &'a str) -> Self {
        Self {
            language,
            content,
            lines: content.lines().collect(),
            scope: Vec::new(),
        }
    }

    fn leave_scopes_before(&mut self, start_byte: usize) {
        while self
            .scope
            .last()
            .is_some_and(|scope| start_byte >= scope.end_byte)
        {
            self.scope.pop();
        }
    }

    pub fn current_scope(&self) -> Option<&str> {
        self.scope.last().map(|scope| scope.name.as_str())
    }

    /// The innermost scope around `node`, not counting a symbol `node` itself
    /// declares (a module-level `const x = new X()` is in the file's scope).
    pub fn scope_around(&self, node: Node<'_>) -> Option<&str> {
        self.scope
            .iter()
            .rev()
            .find(|frame| frame.start_byte != node.start_byte())
            .map(|frame| frame.name.as_str())
    }

    pub fn text(&self, node: Node<'_>) -> String {
        node.utf8_text(self.content.as_bytes())
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    pub fn signature(&self, node: Node<'_>) -> String {
        let text = self.text(node);
        let first_line = text.lines().next().unwrap_or_default().trim();
        truncate_chars(first_line, SIGNATURE_MAX_CHARS)
    }

    pub fn qualify(&self, name: &str) -> String {
        self.current_scope()
            .map(|parent| format!("{parent}.{name}"))
            .unwrap_or_else(|| name.to_string())
    }

    pub fn symbol(
        &self,
        name: String,
        kind: &str,
        node: Node<'_>,
        exported: bool,
    ) -> IndexedSymbol {
        IndexedSymbol {
            qualified_name: self.qualify(&name),
            name,
            kind: kind.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature: self.signature(node),
            exported,
            container: self.current_scope().map(ToOwned::to_owned),
            arity: None,
            default_export: false,
        }
    }

    pub fn reference(&self, symbol_name: String, kind: &str, node: Node<'_>) -> IndexedReference {
        IndexedReference {
            symbol_name,
            from_qualified_name: self.current_scope().map(ToOwned::to_owned),
            from_symbol: self.scope.last().map(|frame| frame.index),
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            context: self
                .lines
                .get(node.start_position().row)
                .map(|line| line.trim().to_string())
                .unwrap_or_default(),
            kind: kind.to_string(),
            receiver: Receiver::None,
            receiver_name: None,
            arg_count: None,
        }
    }

    pub fn call_reference(
        &self,
        callee: Node<'_>,
        kind: &str,
        node: Node<'_>,
    ) -> Option<IndexedReference> {
        let name = self.called_name(callee)?;
        let mut reference = self.reference(name, kind, name_node(callee));
        let receiver = receiver_node(callee).or_else(|| {
            ["object", "receiver", "scope"]
                .iter()
                .find_map(|field| node.child_by_field_name(field))
        });
        if let Some(receiver) = receiver {
            let (kind, name) = self.classify_receiver(receiver);
            reference.receiver = kind;
            reference.receiver_name = name;
        }
        reference.arg_count = node
            .child_by_field_name("arguments")
            .and_then(argument_count);
        Some(reference)
    }

    pub fn classify_receiver(&self, node: Node<'_>) -> (Receiver, Option<String>) {
        if node.kind() == "member_expression" {
            if let Some(path) = self.member_path(node) {
                return (Receiver::Name, Some(path));
            }
        }
        if let Some(class) = self.constructed_class(node) {
            return (Receiver::Name, Some(format!("new:{class}")));
        }
        let text = self.text(node);
        match (node.kind(), text.as_str()) {
            (_, "this" | "self" | "$this") => (Receiver::This, None),
            (_, "super") => (Receiver::Super, None),
            (
                "identifier"
                | "property_identifier"
                | "type_identifier"
                | "constant"
                | "name"
                | "variable_name"
                | "field_identifier",
                _,
            ) => (
                Receiver::Name,
                Some(text.trim_start_matches('$').to_string()),
            ),
            _ => (Receiver::Other, None),
        }
    }

    /// `X` in `new X()` (or `new a.X<T>()`), so a call on it resolves on `X`.
    fn constructed_class(&self, node: Node<'_>) -> Option<String> {
        let class = match node.kind() {
            "new_expression" => node.child_by_field_name("constructor")?,
            "object_creation_expression" => node.child_by_field_name("type")?,
            _ => return None,
        };
        let text = self.text(class);
        let name = text.split('<').next().unwrap_or_default().trim();
        let plain = !name.is_empty()
            && name.split('.').all(|part| {
                part.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            });
        plain.then(|| name.to_string())
    }

    /// `a.b.c` or `this.a.b` as text when every link is a plain name, so the
    /// resolver can follow field types; `None` for calls, indexing and the like.
    pub fn member_path(&self, node: Node<'_>) -> Option<String> {
        let mut parts = Vec::new();
        let mut current = node;
        while current.kind() == "member_expression" {
            let property = current.child_by_field_name("property")?;
            if property.kind() != "property_identifier" {
                return None;
            }
            parts.push(self.text(property));
            current = current.child_by_field_name("object")?;
            if parts.len() > MAX_MEMBER_PATH {
                return None;
            }
        }
        match current.kind() {
            "this" => parts.push("this".to_string()),
            "identifier" => parts.push(self.text(current)),
            _ => return None,
        }
        parts.reverse();
        Some(parts.join("."))
    }

    pub fn import(&self, source: String, kind: &str, node: Node<'_>) -> Option<IndexedImport> {
        if source.is_empty() {
            return None;
        }
        Some(IndexedImport {
            source,
            line: node.start_position().row + 1,
            kind: kind.to_string(),
        })
    }

    pub fn called_name(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "identifier" | "property_identifier" | "type_identifier" => Some(self.text(node)),
            "attribute" => node
                .child_by_field_name("attribute")
                .map(|child| self.text(child))
                .or_else(|| Some(last_identifier(&self.text(node)))),
            "member_expression" | "selector_expression" | "member_access_expression" => node
                .child_by_field_name("property")
                .or_else(|| node.child_by_field_name("field"))
                .or_else(|| node.child_by_field_name("name"))
                .map(|child| self.text(child))
                .or_else(|| Some(last_identifier(&self.text(node)))),
            "qualified_identifier" | "scoped_identifier" => node
                .child_by_field_name("name")
                .map(|child| self.text(child))
                .or_else(|| Some(last_identifier(&self.text(node)))),
            "field_expression" => node
                .child_by_field_name("field")
                .map(|child| self.text(child))
                .or_else(|| Some(last_identifier(&self.text(node)))),
            "subscript" => node
                .child_by_field_name("value")
                .and_then(|child| self.called_name(child)),
            // `make("handle")(1, 2)`: the called thing is a value, with no
            // name of its own; the last identifier inside it is not one.
            "call_expression"
            | "call"
            | "invocation_expression"
            | "method_invocation"
            | "parenthesized_expression"
            | "await_expression"
            | "new_expression"
            | "object_creation_expression"
            | "arrow_function"
            | "function_expression"
            | "function"
            | "lambda"
            | "subscript_expression"
            | "index_expression"
            | "element_access_expression"
            | "string"
            | "template_string" => None,
            _ => Some(last_identifier(&self.text(node))).filter(|name| !name.is_empty()),
        }
    }
}

/// Where a callee's name is written: the property of `a.b`, the field of
/// `a->b`, the last segment of `A::b`, or the callee itself. References are
/// located there so a call on a continuation line points at that line.
fn name_node(callee: Node<'_>) -> Node<'_> {
    let fields: &[&str] = match callee.kind() {
        "member_expression" => &["property"],
        "attribute" => &["attribute"],
        "field_expression" => &["field"],
        "selector_expression" => &["field"],
        "member_access_expression" => &["name"],
        "scoped_identifier" | "qualified_identifier" => &["name"],
        _ => &[],
    };
    fields
        .iter()
        .find_map(|field| callee.child_by_field_name(field))
        .unwrap_or(callee)
}

fn receiver_node(callee: Node<'_>) -> Option<Node<'_>> {
    let field = match callee.kind() {
        "member_expression" | "attribute" => "object",
        "field_expression" => "value",
        "selector_expression" => "operand",
        "member_access_expression" => "expression",
        "scoped_identifier" => "path",
        "qualified_identifier" => "scope",
        _ => return None,
    };
    callee
        .child_by_field_name(field)
        .or_else(|| callee.child_by_field_name("argument"))
}

pub fn strip_quotes(s: String) -> String {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .to_string()
}

pub fn first_child_with_kinds<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

pub fn first_named_child(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|child| child.is_named());
    found
}

pub fn has_child_kind(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|child| child.kind() == kind);
    found
}

pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((byte, _)) => format!("{}…", &s[..byte]),
        None => s.to_string(),
    }
}

pub fn last_identifier(text: &str) -> String {
    text.rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .find(|part| !part.is_empty())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_cuts_on_char_boundary() {
        let long = "é".repeat(300);
        let cut = truncate_chars(&long, 180);
        assert_eq!(cut.chars().count(), 181);
        assert!(cut.ends_with('…'));
        assert_eq!(truncate_chars("short", 180), "short");
    }

    #[test]
    fn deeply_nested_input_does_not_overflow_the_stack() {
        let source = format!("const x = {}{};", "[".repeat(50_000), "]".repeat(50_000));
        let handle = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || parse_file(Language::JavaScript, &source))
            .unwrap();
        handle.join().expect("parser thread panicked").unwrap();
    }

    #[test]
    fn member_call_is_located_at_the_called_name() {
        let parsed = parse_file(
            Language::TypeScript,
            "export function f() {\n  return repo\n    .find(1)\n    .then(g);\n}\n",
        )
        .unwrap();
        let at = |name: &str| {
            parsed
                .references
                .iter()
                .find(|r| r.symbol_name == name)
                .map(|r| (r.line, r.column))
                .unwrap()
        };
        assert_eq!(at("find"), (3, 6));
        assert_eq!(at("then"), (4, 6));
    }

    #[test]
    fn long_non_ascii_signature_does_not_panic() {
        let source = format!("function f() {{ //x {}\n}}\n", "é".repeat(200));
        let parsed = parse_file(Language::JavaScript, &source).unwrap();
        assert!(parsed.symbols[0].signature.chars().count() <= 181);
    }
}
