use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Python;

impl LanguageSupport for Python {
    fn language(&self) -> Language {
        Language::Python
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "function_definition" => "function",
            "class_definition" => "class",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(name, kind, node, true))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call" => cx.call_reference(node.child_by_field_name("function")?, "call", node),
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        match node.kind() {
            "import_statement" => {
                let name = node
                    .child_by_field_name("name")
                    .or_else(|| first_child_with_kinds(node, &["dotted_name", "aliased_import"]))?;
                cx.import(cx.text(name), "import", node)
            }
            "import_from_statement" => {
                let module = node.child_by_field_name("module_name")?;
                cx.import(cx.text(module), "from", node)
            }
            _ => None,
        }
    }
}
