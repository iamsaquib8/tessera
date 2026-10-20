use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Java;

impl LanguageSupport for Java {
    fn language(&self) -> Language {
        Language::Java
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "class_declaration" => "class",
            "interface_declaration" => "interface",
            "enum_declaration" => "enum",
            "record_declaration" => "record",
            "annotation_type_declaration" => "annotation",
            "method_declaration" => "method",
            "constructor_declaration" => "constructor",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(name, kind, node, is_public(node)))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "method_invocation" => {
                cx.call_reference(node.child_by_field_name("name")?, "call", node)
            }
            "object_creation_expression" => {
                cx.call_reference(node.child_by_field_name("type")?, "new", node)
            }
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        match node.kind() {
            "import_declaration" => {
                let target =
                    first_child_with_kinds(node, &["scoped_identifier", "identifier", "asterisk"])?;
                cx.import(cx.text(target), "import", node)
            }
            _ => None,
        }
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &["super", "this"]
    }
}

fn is_public(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let modifiers: Vec<Node<'_>> = node
        .children(&mut cursor)
        .filter(|child| child.kind() == "modifiers")
        .collect();
    modifiers.into_iter().any(|modifiers| {
        let mut inner = modifiers.walk();
        let public = modifiers
            .children(&mut inner)
            .any(|modifier| modifier.kind() == "public");
        public
    })
}
