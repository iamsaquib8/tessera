use tree_sitter::Node;

use crate::extract::{first_named_child, has_child_kind, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Rust;

impl LanguageSupport for Rust {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "function_item" => "function",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            "mod_item" => "module",
            "impl_item" => {
                let name = cx.text(node.child_by_field_name("type")?);
                return Some(cx.symbol(name, "impl", node, false));
            }
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(
            name,
            kind,
            node,
            has_child_kind(node, "visibility_modifier"),
        ))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call_expression" => {
                cx.call_reference(node.child_by_field_name("function")?, "call", node)
            }
            "macro_invocation" => {
                cx.call_reference(node.child_by_field_name("macro")?, "macro", node)
            }
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        match node.kind() {
            "use_declaration" => {
                let arg = node
                    .child_by_field_name("argument")
                    .or_else(|| first_named_child(node))?;
                cx.import(cx.text(arg), "use", node)
            }
            _ => None,
        }
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &[
            "println",
            "eprintln",
            "format",
            "vec",
            "assert",
            "assert_eq",
        ]
    }
}
