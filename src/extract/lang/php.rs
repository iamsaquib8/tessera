use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Php;

impl LanguageSupport for Php {
    fn language(&self) -> Language {
        Language::Php
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_php::LANGUAGE_PHP.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "function_definition" => "function",
            "method_declaration" => "method",
            "class_declaration" => "class",
            "interface_declaration" => "interface",
            "trait_declaration" => "trait",
            "enum_declaration" => "enum",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(name, kind, node, is_public(cx, node)))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "function_call_expression" => {
                cx.call_reference(node.child_by_field_name("function")?, "call", node)
            }
            "member_call_expression" | "scoped_call_expression" => {
                cx.call_reference(node.child_by_field_name("name")?, "call", node)
            }
            "object_creation_expression" => cx.call_reference(
                first_child_with_kinds(node, &["name", "qualified_name"])?,
                "new",
                node,
            ),
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        if node.kind() != "namespace_use_declaration" {
            return None;
        }
        let clause = first_child_with_kinds(node, &["namespace_use_clause"]).unwrap_or(node);
        let name = first_child_with_kinds(clause, &["qualified_name", "namespace_name", "name"])?;
        cx.import(cx.text(name), "use", node)
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &[
            "require",
            "require_once",
            "include",
            "include_once",
            "echo",
            "isset",
        ]
    }
}

fn is_public(cx: &Cx<'_>, node: Node<'_>) -> bool {
    // Top-level functions are global; methods are public unless a visibility modifier says otherwise.
    if node.kind() != "method_declaration" {
        return true;
    }
    let mut cursor = node.walk();
    let modifiers: Vec<String> = node
        .children(&mut cursor)
        .filter(|child| child.kind() == "visibility_modifier")
        .map(|child| cx.text(child))
        .collect();
    modifiers.is_empty() || modifiers.iter().any(|m| m.eq_ignore_ascii_case("public"))
}
