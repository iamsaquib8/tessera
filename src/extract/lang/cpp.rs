use tree_sitter::Node;

use super::c::{c_family_symbol, c_include, C_FAMILY_SKIP};
use crate::extract::{first_child_with_kinds, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Cpp;

impl LanguageSupport for Cpp {
    fn language(&self) -> Language {
        Language::Cpp
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_cpp::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        c_family_symbol(cx, node, true)
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call_expression" => {
                cx.call_reference(node.child_by_field_name("function")?, "call", node)
            }
            "new_expression" => {
                let target = node.child_by_field_name("type").or_else(|| {
                    first_child_with_kinds(node, &["type_identifier", "qualified_identifier"])
                })?;
                cx.call_reference(target, "new", node)
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
