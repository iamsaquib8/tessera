use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, strip_quotes, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct Go;

impl LanguageSupport for Go {
    fn language(&self) -> Language {
        Language::Go
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        match node.kind() {
            "function_declaration" => {
                let name = cx.text(node.child_by_field_name("name")?);
                let exported = is_go_exported(&name);
                Some(cx.symbol(name, "function", node, exported))
            }
            "method_declaration" => {
                let name = cx.text(node.child_by_field_name("name")?);
                let receiver = node
                    .child_by_field_name("receiver")
                    .and_then(|recv| receiver_type(cx, recv));
                let exported = is_go_exported(&name);
                let mut symbol = cx.symbol(name, "method", node, exported);
                symbol.qualified_name = receiver
                    .map(|t| format!("{t}.{}", symbol.name))
                    .unwrap_or_else(|| symbol.name.clone());
                Some(symbol)
            }
            "type_spec" => {
                let name = cx.text(node.child_by_field_name("name")?);
                let kind = match node.child_by_field_name("type")?.kind() {
                    "struct_type" => "struct",
                    "interface_type" => "interface",
                    _ => "type",
                };
                let exported = is_go_exported(&name);
                Some(cx.symbol(name, kind, node, exported))
            }
            _ => None,
        }
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
        match node.kind() {
            "import_spec" => {
                let path = node
                    .child_by_field_name("path")
                    .or_else(|| first_child_with_kinds(node, &["interpreted_string_literal"]))?;
                cx.import(strip_quotes(cx.text(path)), "import", node)
            }
            _ => None,
        }
    }
}

fn receiver_type(cx: &Cx<'_>, recv: Node<'_>) -> Option<String> {
    let mut cursor = recv.walk();
    let declaration = recv
        .children(&mut cursor)
        .find(|child| child.kind() == "parameter_declaration")?;
    let mut type_node = declaration.child_by_field_name("type")?;
    while type_node.kind() == "pointer_type" {
        match type_node.named_child(0) {
            Some(inner) => type_node = inner,
            None => break,
        }
    }
    Some(cx.text(type_node))
}

fn is_go_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}
