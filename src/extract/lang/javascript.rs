use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, strip_quotes, Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct JavaScript;

pub(super) const JS_SKIP: &[&str] = &["super"];

impl LanguageSupport for JavaScript {
    fn language(&self) -> Language {
        Language::JavaScript
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        js_symbol(cx, node)
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        js_reference(cx, node)
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        js_import(cx, node)
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        JS_SKIP
    }
}

pub(super) fn js_symbol(cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
    let (name, kind) = match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            (cx.text(node.child_by_field_name("name")?), "function")
        }
        "class_declaration" => (cx.text(node.child_by_field_name("name")?), "class"),
        "method_definition" | "method_signature" => {
            (cx.text(node.child_by_field_name("name")?), "method")
        }
        "variable_declarator" => {
            let value = node.child_by_field_name("value")?;
            if !matches!(
                value.kind(),
                "arrow_function" | "function" | "function_expression"
            ) {
                return None;
            }
            (cx.text(node.child_by_field_name("name")?), "function")
        }
        _ => return None,
    };
    Some(cx.symbol(name, kind, node, has_export_ancestor(node)))
}

pub(super) fn js_reference(cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
    match node.kind() {
        // Only component-looking JSX names (leading uppercase or `Foo.Bar`) count;
        // lowercase names are intrinsic HTML elements.
        "jsx_self_closing_element" | "jsx_opening_element" => {
            let name = jsx_component_name(cx, node.child_by_field_name("name")?)?;
            Some(cx.reference(name, "jsx", node))
        }
        "call_expression" => cx.call_reference(node.child_by_field_name("function")?, "call", node),
        _ => None,
    }
}

pub(super) fn js_import(cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
    match node.kind() {
        "import_statement" => {
            let src = node
                .child_by_field_name("source")
                .or_else(|| first_child_with_kinds(node, &["string"]))?;
            cx.import(strip_quotes(cx.text(src)), "import", node)
        }
        "call_expression" => {
            let name = cx.text(node.child_by_field_name("function")?);
            let kind = match name.as_str() {
                "require" => "require",
                "import" => "import",
                _ => return None,
            };
            let args = node.child_by_field_name("arguments")?;
            let string_arg = first_child_with_kinds(args, &["string"])?;
            cx.import(strip_quotes(cx.text(string_arg)), kind, node)
        }
        _ => None,
    }
}

fn jsx_component_name(cx: &Cx<'_>, name_node: Node<'_>) -> Option<String> {
    match name_node.kind() {
        "identifier" => {
            let text = cx.text(name_node);
            let first = text.chars().next()?;
            (first.is_ascii_uppercase() || first == '_').then_some(text)
        }
        "nested_identifier" | "member_expression" => {
            let last = name_node
                .child_by_field_name("property")
                .or_else(|| name_node.child_by_field_name("name"))
                .or_else(|| {
                    name_node.named_child(name_node.named_child_count().saturating_sub(1) as u32)
                })?;
            Some(cx.text(last))
        }
        _ => {
            let text = cx.text(name_node);
            text.chars()
                .next()
                .filter(char::is_ascii_uppercase)
                .map(|_| text)
        }
    }
}

fn has_export_ancestor(node: Node<'_>) -> bool {
    let mut parent = node.parent();
    while let Some(current) = parent {
        if current.kind() == "export_statement" {
            return true;
        }
        parent = current.parent();
    }
    false
}
