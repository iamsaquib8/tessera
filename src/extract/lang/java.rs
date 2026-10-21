use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, has_child_kind, Cx, LanguageSupport};
use crate::types::{
    BindingKind, HintOrigin, IndexedBinding, IndexedImport, IndexedReference, IndexedSymbol,
    IndexedTypeHint, Language, Receiver,
};

pub struct Java;

// java.lang types and annotations: implicitly imported, never defined in the
// repository, so type, heritage and annotation references to them are noise.
const JAVA_LANG: &[&str] = &[
    "String",
    "Object",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Character",
    "Boolean",
    "Double",
    "Float",
    "Void",
    "Number",
    "Class",
    "Iterable",
    "Comparable",
    "Runnable",
    "Thread",
    "Throwable",
    "Exception",
    "RuntimeException",
    "Error",
    "StringBuilder",
    "CharSequence",
    "Enum",
    "Record",
    "AutoCloseable",
    "Override",
    "Deprecated",
    "SuppressWarnings",
    "FunctionalInterface",
    "SafeVarargs",
];

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
        let mut symbol = cx.symbol(name, kind, node, is_public(node));
        if matches!(kind, "method" | "constructor") {
            symbol.arity = node.child_by_field_name("parameters").map(arity);
        }
        Some(symbol)
    }

    fn reference(&self, _cx: &Cx<'_>, _node: Node<'_>) -> Option<IndexedReference> {
        None
    }

    fn references(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedReference>) {
        match node.kind() {
            "method_invocation" => {
                if let Some(name) = node.child_by_field_name("name") {
                    out.extend(cx.call_reference(name, "call", node));
                }
            }
            "object_creation_expression" => {
                if let Some(head) = node.child_by_field_name("type").map(type_head) {
                    let mut reference = type_reference(cx, head, "new", node);
                    reference.arg_count = node
                        .child_by_field_name("arguments")
                        .map(|args| args.named_child_count() as u16);
                    out.push(reference);
                }
            }
            "superclass" => out.extend(heritage(cx, node, "extends")),
            "super_interfaces" => out.extend(heritage(cx, node, "implements")),
            "extends_interfaces" => out.extend(heritage(cx, node, "extends")),
            "marker_annotation" | "annotation" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let reference = type_reference(cx, name, "decorator", node);
                    if !JAVA_LANG.contains(&reference.symbol_name.as_str()) {
                        out.push(reference);
                    }
                }
            }
            "type_identifier" | "scoped_type_identifier" if is_type_use(cx, node) => {
                let reference = type_reference(cx, node, "type", node);
                if !JAVA_LANG.contains(&reference.symbol_name.as_str()) {
                    out.push(reference);
                }
            }
            _ => {}
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

    fn bindings(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedBinding>) {
        if node.kind() != "import_declaration" {
            return;
        }
        let Some(path) =
            first_child_with_kinds(node, &["scoped_identifier", "identifier"]).map(|p| cx.text(p))
        else {
            return;
        };
        let is_static = has_child_kind(node, "static");
        let wildcard = has_child_kind(node, "asterisk");
        let (owner, last) = path.rsplit_once('.').unwrap_or(("", path.as_str()));
        let (local, imported, source, kind) = match (is_static, wildcard) {
            (true, true) => ("*".to_string(), None, path.clone(), BindingKind::JavaStatic),
            (true, false) => (
                last.to_string(),
                Some(last.to_string()),
                owner.to_string(),
                BindingKind::JavaStatic,
            ),
            (false, true) => (
                "*".to_string(),
                None,
                path.clone(),
                BindingKind::JavaWildcard,
            ),
            (false, false) => (
                last.to_string(),
                Some(last.to_string()),
                path.clone(),
                BindingKind::JavaSingle,
            ),
        };
        out.push(IndexedBinding {
            local,
            imported,
            source,
            kind,
            line: node.start_position().row + 1,
        });
    }

    fn hints(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedTypeHint>) {
        let scope = cx.current_scope().map(ToOwned::to_owned);
        let mut push = |name: String, type_name: String, origin| {
            out.push(IndexedTypeHint {
                scope: scope.clone(),
                name,
                type_name,
                origin,
            })
        };
        match node.kind() {
            "field_declaration" | "local_variable_declaration" => {
                let Some(ty) = node.child_by_field_name("type") else {
                    return;
                };
                let declared = (cx.text(ty) != "var").then(|| type_name(cx, ty)).flatten();
                let field = node.kind() == "field_declaration";
                let mut cursor = node.walk();
                let declarators: Vec<Node<'_>> = node
                    .children_by_field_name("declarator", &mut cursor)
                    .collect();
                for declarator in declarators {
                    let Some(name) = declarator.child_by_field_name("name") else {
                        continue;
                    };
                    let name = if field {
                        format!("this.{}", cx.text(name))
                    } else {
                        cx.text(name)
                    };
                    let origin = if field {
                        HintOrigin::Field
                    } else {
                        HintOrigin::Declared
                    };
                    if let Some(ty) = &declared {
                        push(name, ty.clone(), origin);
                    } else if let Some(value) = declarator.child_by_field_name("value") {
                        match value.kind() {
                            "object_creation_expression" => {
                                if let Some(ty) = value
                                    .child_by_field_name("type")
                                    .and_then(|t| type_name(cx, t))
                                {
                                    push(name, ty, HintOrigin::New);
                                }
                            }
                            "method_invocation" => {
                                if let Some(callee) = callee_text(cx, value) {
                                    push(name, format!("call:{callee}"), HintOrigin::Return);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "formal_parameter" => {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|t| type_name(cx, t));
                if let (Some(ty), Some(name)) = (ty, node.child_by_field_name("name")) {
                    push(cx.text(name), ty, HintOrigin::Param);
                }
            }
            "method_declaration" => {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|t| type_name(cx, t));
                if let (Some(ty), Some(name)) = (ty, node.child_by_field_name("name")) {
                    push(cx.text(name), ty, HintOrigin::Return);
                }
            }
            _ => {}
        }
    }

    fn module(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<String> {
        if node.kind() != "package_declaration" {
            return None;
        }
        first_child_with_kinds(node, &["scoped_identifier", "identifier"]).map(|p| cx.text(p))
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        &["super", "this"]
    }
}

/// `Foo<Bar>` → `Foo`; other type nodes are returned unchanged.
fn type_head(node: Node<'_>) -> Node<'_> {
    if node.kind() == "generic_type" {
        if let Some(head) =
            first_child_with_kinds(node, &["type_identifier", "scoped_type_identifier"])
        {
            return head;
        }
    }
    node
}

/// A reference to a possibly qualified type or annotation name: `a.b.Foo`
/// records `Foo` with receiver `a.b`.
fn type_reference(cx: &Cx<'_>, name: Node<'_>, kind: &str, at: Node<'_>) -> IndexedReference {
    let name = type_head(name);
    let text = cx.text(name);
    let text = text
        .split('<')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let (prefix, last) = match text.rsplit_once('.') {
        Some((prefix, last)) => (Some(prefix.to_string()), last.to_string()),
        None => (None, text),
    };
    let mut reference = cx.reference(last, kind, at);
    if let Some(prefix) = prefix {
        reference.receiver = Receiver::Name;
        reference.receiver_name = Some(prefix);
    }
    reference
}

fn heritage(cx: &Cx<'_>, node: Node<'_>, kind: &str) -> Vec<IndexedReference> {
    let list = first_child_with_kinds(node, &["type_list"]).unwrap_or(node);
    let mut cursor = list.walk();
    let types: Vec<Node<'_>> = list.named_children(&mut cursor).collect();
    types
        .into_iter()
        .filter(|ty| {
            matches!(
                ty.kind(),
                "type_identifier" | "scoped_type_identifier" | "generic_type"
            )
        })
        .map(|ty| type_reference(cx, ty, kind, ty))
        .filter(|r| !JAVA_LANG.contains(&r.symbol_name.as_str()))
        .collect()
}

/// Whether a type node is a use worth a `type` reference: not a declaration,
/// not a part of a larger qualified name, not the head of a `new` or heritage
/// clause (those record their own kinds), and not a type parameter.
fn is_type_use(cx: &Cx<'_>, node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if matches!(parent.kind(), "scoped_type_identifier" | "type_parameter") {
        return false;
    }
    let head_owner = if parent.kind() == "generic_type" && type_head(parent) == node {
        parent.parent()
    } else {
        Some(parent)
    };
    if head_owner.is_some_and(|owner| {
        matches!(
            owner.kind(),
            "object_creation_expression" | "superclass" | "type_list"
        )
    }) {
        return false;
    }
    let name = cx.text(node);
    name != "var" && !is_type_parameter(cx, node, &name)
}

fn is_type_parameter(cx: &Cx<'_>, node: Node<'_>, name: &str) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if let Some(params) = first_child_with_kinds(ancestor, &["type_parameters"]) {
            let mut cursor = params.walk();
            let declared = params.named_children(&mut cursor).any(|param| {
                first_child_with_kinds(param, &["type_identifier", "identifier"])
                    .is_some_and(|id| cx.text(id) == name)
            });
            if declared {
                return true;
            }
        }
        current = ancestor.parent();
    }
    false
}

fn type_name(cx: &Cx<'_>, node: Node<'_>) -> Option<String> {
    let head = match node.kind() {
        "type_identifier" | "scoped_type_identifier" => node,
        "generic_type" => Some(type_head(node)).filter(|head| *head != node)?,
        _ => return None,
    };
    let name = cx.text(head);
    (!is_type_parameter(cx, head, &name)).then_some(name)
}

fn callee_text(cx: &Cx<'_>, invocation: Node<'_>) -> Option<String> {
    let name = cx.text(invocation.child_by_field_name("name")?);
    match invocation.child_by_field_name("object") {
        None => Some(name),
        Some(object) if matches!(object.kind(), "identifier" | "field_access" | "this") => {
            Some(format!("{}.{name}", cx.text(object)))
        }
        Some(_) => None,
    }
}

fn arity(params: Node<'_>) -> (u16, Option<u16>) {
    let mut cursor = params.walk();
    let (mut fixed, mut variadic) = (0u16, false);
    for param in params.named_children(&mut cursor) {
        match param.kind() {
            "formal_parameter" => fixed += 1,
            "spread_parameter" => variadic = true,
            _ => {}
        }
    }
    (fixed, (!variadic).then_some(fixed))
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
