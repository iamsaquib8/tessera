use tree_sitter::Node;

use crate::extract::{first_named_child, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct CSharp;

impl LanguageSupport for CSharp {
    fn language(&self) -> Language {
        Language::CSharp
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_c_sharp::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "class_declaration" => "class",
            "interface_declaration" => "interface",
            "struct_declaration" => "struct",
            "enum_declaration" => "enum",
            "record_declaration" => "record",
            "method_declaration" => "method",
            "constructor_declaration" => "constructor",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        let mut cursor = node.walk();
        let public = node
            .children(&mut cursor)
            .any(|child| child.kind() == "modifier" && cx.text(child) == "public");
        Some(cx.symbol(name, kind, node, public))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "invocation_expression" => {
                cx.call_reference(node.child_by_field_name("function")?, "call", node)
            }
            "object_creation_expression" => {
                cx.call_reference(node.child_by_field_name("type")?, "new", node)
            }
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        match node.kind() {
            "using_directive" => {
                let name = node
                    .child_by_field_name("name")
                    .or_else(|| first_named_child(node))?;
                cx.import(cx.text(name), "using", node)
            }
            _ => None,
        }
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &["base", "this", "nameof", "typeof"]
    }
}
