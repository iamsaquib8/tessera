use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, strip_quotes, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Ruby;

impl LanguageSupport for Ruby {
    fn language(&self) -> Language {
        Language::Ruby
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_ruby::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "method" | "singleton_method" => "method",
            "class" => "class",
            "module" => "module",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(name, kind, node, true))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call" | "method_call" => {
                cx.call_reference(node.child_by_field_name("method")?, "call", node)
            }
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        if node.kind() != "call" {
            return None;
        }
        let method = cx.text(node.child_by_field_name("method")?);
        if !matches!(method.as_str(), "require" | "require_relative" | "load") {
            return None;
        }
        let args = node.child_by_field_name("arguments")?;
        let string_arg = first_child_with_kinds(args, &["string", "string_content"])?;
        cx.import(strip_quotes(cx.text(string_arg)), "require", node)
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &[
            "require",
            "require_relative",
            "load",
            "puts",
            "print",
            "p",
            "raise",
        ]
    }
}
