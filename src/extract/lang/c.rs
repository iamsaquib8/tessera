use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, last_identifier, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct C;

pub(super) const C_FAMILY_SKIP: &[&str] = &[
    "sizeof",
    "printf",
    "fprintf",
    "sprintf",
    "malloc",
    "free",
    "memcpy",
    "memset",
    "strlen",
    "strcmp",
    "assert",
    "static_cast",
    "reinterpret_cast",
    "dynamic_cast",
    "const_cast",
];

impl LanguageSupport for C {
    fn language(&self) -> Language {
        Language::C
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_c::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        c_family_symbol(cx, node, false)
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call_expression" => {
                cx.call_reference(node.child_by_field_name("function")?, "call", node)
            }
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        c_include(cx, node)
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        C_FAMILY_SKIP
    }
}

pub(super) fn c_family_symbol(cx: &Cx<'_>, node: Node<'_>, cpp: bool) -> Option<IndexedSymbol> {
    let has_body = node.child_by_field_name("body").is_some();
    let kind = match node.kind() {
        "function_definition" => {
            let name = declarator_name(cx, node.child_by_field_name("declarator")?)?;
            if name.is_empty() {
                return None;
            }
            return Some(cx.symbol(name, "function", node, !has_static(cx, node)));
        }
        "struct_specifier" if has_body => "struct",
        "enum_specifier" if has_body => "enum",
        "union_specifier" if has_body => "union",
        "class_specifier" if cpp && has_body => "class",
        "namespace_definition" if cpp => "namespace",
        _ => return None,
    };
    let name = cx.text(node.child_by_field_name("name")?);
    Some(cx.symbol(name, kind, node, true))
}

pub(super) fn c_include(cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
    if node.kind() != "preproc_include" {
        return None;
    }
    let path = node
        .child_by_field_name("path")
        .or_else(|| first_child_with_kinds(node, &["string_literal", "system_lib_string"]))?;
    let cleaned = cx
        .text(path)
        .trim_matches(|c: char| matches!(c, '"' | '<' | '>' | ' '))
        .to_string();
    cx.import(cleaned, "include", node)
}

fn declarator_name(cx: &Cx<'_>, node: Node<'_>) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" | "destructor_name"
        | "operator_name" => Some(cx.text(node)),
        "qualified_identifier" => node
            .child_by_field_name("name")
            .map(|n| cx.text(n))
            .or_else(|| Some(last_identifier(&cx.text(node)))),
        "function_declarator"
        | "pointer_declarator"
        | "reference_declarator"
        | "parenthesized_declarator"
        | "array_declarator"
        | "init_declarator" => declarator_name(cx, node.child_by_field_name("declarator")?),
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
            children
                .into_iter()
                .find_map(|child| declarator_name(cx, child))
        }
    }
}

fn has_static(cx: &Cx<'_>, node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| child.kind() == "storage_class_specifier" && cx.text(child) == "static");
    found
}
