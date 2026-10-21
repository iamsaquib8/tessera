use tree_sitter::Node;

use crate::extract::{first_child_with_kinds, strip_quotes, Cx, LanguageSupport};
use crate::types::{
    BindingKind, HintOrigin, IndexedBinding, IndexedImport, IndexedReference, IndexedSymbol,
    IndexedTypeHint, Language, Receiver,
};

pub struct JavaScript;

pub(super) const JS_SKIP: &[&str] = &["super"];

const COMPONENT_WRAPPERS: &[&str] = &["memo", "forwardRef", "observer"];
const FUNCTION_KINDS: &[&str] = &["arrow_function", "function_expression", "function"];
const DECLARATION_PARENTS: &[&str] = &[
    "class_declaration",
    "abstract_class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "type_parameter",
    "implements_clause",
    "extends_type_clause",
    "nested_type_identifier",
];

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

    fn reference(&self, _cx: &Cx<'_>, _node: Node<'_>) -> Option<IndexedReference> {
        None
    }

    fn references(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedReference>) {
        js_references(cx, node, out)
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        js_import(cx, node)
    }

    fn bindings(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedBinding>) {
        js_bindings(cx, node, out)
    }

    fn hints(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedTypeHint>) {
        js_hints(cx, node, out)
    }

    fn skip_ref_names(&self) -> &'static [&'static str] {
        JS_SKIP
    }
}

pub(super) fn js_symbol(cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
    // Ancestor walks are O(depth), so they run only for nodes that are symbols.
    let (name, kind, params) = match node.kind() {
        "function_declaration" | "generator_function_declaration" => (
            cx.text(node.child_by_field_name("name")?),
            "function",
            Some(node),
        ),
        "class_declaration" | "abstract_class_declaration" => {
            (cx.text(node.child_by_field_name("name")?), "class", None)
        }
        "interface_declaration" => (
            cx.text(node.child_by_field_name("name")?),
            "interface",
            None,
        ),
        "type_alias_declaration" => (cx.text(node.child_by_field_name("name")?), "type", None),
        "enum_declaration" => (cx.text(node.child_by_field_name("name")?), "enum", None),
        // In a class body a bodiless signature is an overload of the method
        // that follows; that method's symbol spans it.
        "method_signature" if node.parent().is_some_and(|p| p.kind() == "class_body") => {
            return None
        }
        "method_definition" | "method_signature" | "abstract_method_signature" => (
            cx.text(node.child_by_field_name("name")?),
            "method",
            Some(node),
        ),
        "variable_declarator" => {
            let name = node.child_by_field_name("name")?;
            let value = node.child_by_field_name("value")?;
            let function = if FUNCTION_KINDS.contains(&value.kind()) {
                Some(value)
            } else {
                wrapped_component(cx, value)
            };
            match function {
                Some(function) => (cx.text(name), "function", Some(function)),
                // A module-level constant is a symbol too, so calls in its
                // initializer (`export const x = register({ perform() {…} })`)
                // have an enclosing symbol.
                None if name.kind() == "identifier" && is_module_level(node) => {
                    (cx.text(name), "variable", None)
                }
                None => return None,
            }
        }
        // `onClick = () => {…}` and `static create = (…) => …` in a class body
        // behave as methods.
        "public_field_definition" | "field_definition" => {
            let value = node.child_by_field_name("value")?;
            if !FUNCTION_KINDS.contains(&value.kind()) {
                return None;
            }
            let name = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("property"))?;
            (cx.text(name), "method", Some(value))
        }
        "assignment_expression" => {
            let right = node.child_by_field_name("right")?;
            if !FUNCTION_KINDS.contains(&right.kind()) {
                return None;
            }
            let name = commonjs_export_name(cx, node.child_by_field_name("left")?)?;
            let mut symbol = cx.symbol(name, "function", node, true);
            symbol.arity = arity(cx, right);
            return Some(symbol);
        }
        kind if FUNCTION_KINDS.contains(&kind) => {
            if is_default_export(node) {
                ("default".to_string(), "function", Some(node))
            } else {
                (callback_name(cx, node)?, "callback", Some(node))
            }
        }
        _ => return None,
    };
    let mut symbol = cx.symbol(name, kind, node, has_export_ancestor(node));
    symbol.default_export = is_default_export(node);
    match node.kind() {
        "function_declaration" => {
            symbol.start_line = overload_start(cx, node, &symbol.name, "function_signature") + 1
        }
        "method_definition" => {
            symbol.start_line = overload_start(cx, node, &symbol.name, "method_signature") + 1
        }
        _ => {}
    }
    symbol.arity = params.and_then(|p| arity(cx, p));
    Some(symbol)
}

pub(super) fn js_references(cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedReference>) {
    match node.kind() {
        // Only component-looking JSX names (leading uppercase or `Foo.Bar`) count;
        // lowercase names are intrinsic HTML elements.
        "jsx_self_closing_element" | "jsx_opening_element" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            if let Some(name) = jsx_component_name(cx, name_node) {
                let mut reference = cx.reference(name, "jsx", node);
                // `<Menu.Item>` uses `Item` through `Menu`, and `Menu` itself.
                if matches!(name_node.kind(), "nested_identifier" | "member_expression") {
                    let text = cx.text(name_node);
                    if let Some((object, _)) = text.rsplit_once('.') {
                        reference.receiver = Receiver::Name;
                        reference.receiver_name = Some(object.to_string());
                        let root = object.split('.').next().unwrap_or(object);
                        out.push(cx.reference(root.to_string(), "jsx", node));
                    }
                }
                out.push(reference);
            }
        }
        // A decorator call is recorded once, as the decorator.
        "call_expression" if node.parent().is_none_or(|p| p.kind() != "decorator") => {
            if let Some(callee) = node.child_by_field_name("function") {
                out.extend(cx.call_reference(callee, "call", node));
            }
        }
        "new_expression" => {
            if let Some(constructor) = node.child_by_field_name("constructor") {
                out.extend(cx.call_reference(constructor, "new", node));
            }
        }
        "class_heritage" => {
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
            for child in children {
                match child.kind() {
                    "extends_clause" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            out.extend(cx.call_reference(value, "extends", child));
                        }
                    }
                    "implements_clause" => {
                        let mut inner = child.walk();
                        let types: Vec<Node<'_>> = child.named_children(&mut inner).collect();
                        for ty in types {
                            if let Some(name) = type_name(cx, ty) {
                                out.push(cx.reference(last_segment(&name), "implements", ty));
                            }
                        }
                    }
                    _ => out.extend(cx.call_reference(child, "extends", node)),
                }
            }
        }
        "decorator" => {
            let Some(expr) = node.named_child(0) else {
                return;
            };
            let callee = if expr.kind() == "call_expression" {
                expr.child_by_field_name("function").unwrap_or(expr)
            } else {
                expr
            };
            out.extend(cx.call_reference(callee, "decorator", node));
        }
        "type_identifier" => {
            let skip = node
                .parent()
                .is_some_and(|p| DECLARATION_PARENTS.contains(&p.kind()) || is_heritage_type(p))
                || is_type_parameter(cx, node);
            if !skip {
                out.push(cx.reference(cx.text(node), "type", node));
            }
        }
        "nested_type_identifier" => {
            let module = node.child_by_field_name("module");
            let name = node.child_by_field_name("name");
            if let (Some(module), Some(name)) = (module, name) {
                let mut reference = cx.reference(cx.text(name), "type", node);
                let (receiver, receiver_name) = cx.classify_receiver(module);
                reference.receiver = receiver;
                reference.receiver_name = receiver_name;
                out.push(reference);
            }
        }
        _ => {}
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
            let (kind, source) = require_call(cx, node)?;
            cx.import(source, kind, node)
        }
        _ => None,
    }
}

pub(super) fn js_bindings(cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedBinding>) {
    let line = node.start_position().row + 1;
    let mut push = |local: String, imported: Option<String>, source: &str, kind| {
        out.push(IndexedBinding {
            local,
            imported,
            source: source.to_string(),
            kind,
            line,
        })
    };
    match node.kind() {
        "import_statement" => {
            let Some(source) = node.child_by_field_name("source") else {
                return;
            };
            let source = strip_quotes(cx.text(source));
            let Some(clause) = first_child_with_kinds(node, &["import_clause"]) else {
                return;
            };
            let mut cursor = clause.walk();
            let parts: Vec<Node<'_>> = clause.named_children(&mut cursor).collect();
            for part in parts {
                match part.kind() {
                    "identifier" => push(
                        cx.text(part),
                        Some("default".into()),
                        &source,
                        BindingKind::Default,
                    ),
                    "namespace_import" => {
                        if let Some(id) = first_child_with_kinds(part, &["identifier"]) {
                            push(cx.text(id), None, &source, BindingKind::Namespace);
                        }
                    }
                    "named_imports" => {
                        for (local, imported) in specifiers(cx, part, "import_specifier") {
                            push(local, Some(imported), &source, BindingKind::Named);
                        }
                    }
                    _ => {}
                }
            }
        }
        "export_statement" => {
            let source = node
                .child_by_field_name("source")
                .map(|s| strip_quotes(cx.text(s)));
            let from = source.as_deref().unwrap_or(".");
            if let Some(clause) = first_child_with_kinds(node, &["export_clause"]) {
                for (local, imported) in specifiers(cx, clause, "export_specifier") {
                    push(local, Some(imported), from, BindingKind::ReExport);
                }
            } else if let Some(ns) = first_child_with_kinds(node, &["namespace_export"]) {
                if let Some(id) = ns.named_child(0) {
                    push(cx.text(id), Some("*".into()), from, BindingKind::ReExport);
                }
            } else if source.is_some() {
                push("*".into(), None, from, BindingKind::ReExportAll);
            } else if is_default_export_statement(node) {
                if let Some(value) = node.child_by_field_name("value") {
                    if value.kind() == "identifier" {
                        push(
                            "default".into(),
                            Some(cx.text(value)),
                            ".",
                            BindingKind::ReExport,
                        );
                    }
                }
            }
        }
        "variable_declarator" => {
            let (Some(name), Some(value)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ) else {
                return;
            };
            if let Some((_, source)) = require_call(cx, value) {
                match name.kind() {
                    "identifier" => push(cx.text(name), None, &source, BindingKind::Require),
                    "object_pattern" => {
                        for (local, imported) in object_pattern(cx, name) {
                            push(local, Some(imported), &source, BindingKind::RequireMember);
                        }
                    }
                    _ => {}
                }
            } else if value.kind() == "member_expression" {
                let object = value.child_by_field_name("object");
                let property = value.child_by_field_name("property");
                if let (Some(object), Some(property)) = (object, property) {
                    if let Some((_, source)) = require_call(cx, object) {
                        push(
                            cx.text(name),
                            Some(cx.text(property)),
                            &source,
                            BindingKind::RequireMember,
                        );
                    }
                }
            }
        }
        "assignment_expression" => {
            let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) else {
                return;
            };
            if cx.text(left) == "module.exports" {
                match right.kind() {
                    "identifier" => push(
                        "default".into(),
                        Some(cx.text(right)),
                        ".",
                        BindingKind::ReExport,
                    ),
                    "object" => {
                        for (local, imported) in object_literal(cx, right) {
                            push(local, Some(imported), ".", BindingKind::ReExport);
                        }
                    }
                    _ => {}
                }
            } else if let Some(name) = commonjs_export_name(cx, left) {
                let imported = if right.kind() == "identifier" {
                    cx.text(right)
                } else {
                    name.clone()
                };
                push(name, Some(imported), ".", BindingKind::ReExport);
            }
        }
        _ => {}
    }
}

pub(super) fn js_hints(cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedTypeHint>) {
    let scope = if node.kind() == "variable_declarator" {
        cx.scope_around(node).map(ToOwned::to_owned)
    } else {
        cx.current_scope().map(ToOwned::to_owned)
    };
    let mut push = |scope: Option<String>, name: String, type_name: String, origin| {
        out.push(IndexedTypeHint {
            scope,
            name,
            type_name,
            origin,
        })
    };
    match node.kind() {
        "variable_declarator" => {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            if name.kind() != "identifier" {
                return;
            }
            let name = cx.text(name);
            if let Some(ty) = node
                .child_by_field_name("type")
                .and_then(|t| type_name(cx, t))
            {
                push(scope.clone(), name.clone(), ty, HintOrigin::Declared);
            } else if let Some(value) = node.child_by_field_name("value") {
                if let Some(ty) = value_type(cx, value) {
                    let origin = if value.kind() == "new_expression" {
                        HintOrigin::New
                    } else {
                        HintOrigin::Return
                    };
                    push(scope, name, ty, origin);
                }
            }
        }
        "required_parameter" | "optional_parameter" => {
            let Some(pattern) = node.child_by_field_name("pattern") else {
                return;
            };
            if pattern.kind() != "identifier" {
                return;
            }
            let Some(ty) = node
                .child_by_field_name("type")
                .and_then(|t| type_name(cx, t))
            else {
                return;
            };
            let name = cx.text(pattern);
            push(scope.clone(), name.clone(), ty.clone(), HintOrigin::Param);
            let is_property = first_child_with_kinds(node, &["accessibility_modifier"]).is_some()
                || has_token(node, "readonly");
            if is_property {
                let class = scope
                    .as_deref()
                    .and_then(|s| s.rsplit_once('.'))
                    .map(|(c, _)| c.to_string());
                push(class, format!("this.{name}"), ty, HintOrigin::Field);
            }
        }
        "public_field_definition" | "field_definition" => {
            let Some(name) = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("property"))
            else {
                return;
            };
            let name = format!("this.{}", cx.text(name));
            if let Some(ty) = node
                .child_by_field_name("type")
                .and_then(|t| type_name(cx, t))
            {
                push(scope, name, ty, HintOrigin::Field);
            } else if let Some(ty) = node
                .child_by_field_name("value")
                .and_then(|v| value_type(cx, v))
            {
                push(scope, name, ty, HintOrigin::Field);
            }
        }
        "function_declaration" | "method_definition" | "method_signature" => {
            let (Some(name), Some(ret)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("return_type"),
            ) else {
                return;
            };
            if let Some(ty) = type_name(cx, ret) {
                push(scope, cx.text(name), ty, HintOrigin::Return);
            }
        }
        _ => {}
    }
}

fn value_type(cx: &Cx<'_>, value: Node<'_>) -> Option<String> {
    match value.kind() {
        "new_expression" => value.child_by_field_name("constructor").map(|c| cx.text(c)),
        "call_expression" => {
            let callee = value.child_by_field_name("function")?;
            let text = cx.text(callee);
            let skip = matches!(text.as_str(), "require" | "import")
                || COMPONENT_WRAPPERS.contains(&last_segment(&text).as_str());
            (!skip && matches!(callee.kind(), "identifier" | "member_expression"))
                .then(|| format!("call:{text}"))
        }
        "await_expression" => value.named_child(0).and_then(|v| value_type(cx, v)),
        // `const scene = app.scene` has whatever type the field has.
        "member_expression" => cx.member_path(value).map(|path| format!("path:{path}")),
        _ => None,
    }
}

fn type_name(cx: &Cx<'_>, node: Node<'_>) -> Option<String> {
    match node.kind() {
        "type_annotation" | "opting_type_annotation" | "asserts_annotation" => {
            node.named_child(0).and_then(|t| type_name(cx, t))
        }
        "type_identifier" | "nested_type_identifier" | "identifier" => Some(cx.text(node)),
        "generic_type" => node
            .child_by_field_name("name")
            .and_then(|n| type_name(cx, n)),
        "union_type" => {
            let mut cursor = node.walk();
            let members: Vec<String> = node
                .named_children(&mut cursor)
                .filter_map(|n| type_name(cx, n))
                .collect();
            (members.len() == 1).then(|| members[0].clone())
        }
        _ => None,
    }
}

fn arity(cx: &Cx<'_>, function: Node<'_>) -> Option<(u16, Option<u16>)> {
    if function.child_by_field_name("parameter").is_some() {
        return Some((1, Some(1)));
    }
    let params = function.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    let (mut required, mut total, mut variadic) = (0u16, 0u16, false);
    let children: Vec<Node<'_>> = params.named_children(&mut cursor).collect();
    for param in children {
        let pattern = param.child_by_field_name("pattern").unwrap_or(param);
        let optional = param.kind() == "optional_parameter"
            || param.child_by_field_name("value").is_some()
            || pattern.kind() == "assignment_pattern";
        if pattern.kind() == "rest_pattern" {
            variadic = true;
            continue;
        }
        if cx.text(pattern) == "this" {
            continue;
        }
        total += 1;
        if !optional {
            required = total;
        }
    }
    Some((required, (!variadic).then_some(total)))
}

fn callback_name(cx: &Cx<'_>, function: Node<'_>) -> Option<String> {
    let args = function.parent().filter(|p| p.kind() == "arguments")?;
    let call = args.parent().filter(|p| p.kind() == "call_expression")?;
    let callee = cx.text(call.child_by_field_name("function")?);
    if COMPONENT_WRAPPERS.contains(&last_segment(&callee).as_str())
        && call
            .parent()
            .is_some_and(|p| p.kind() == "variable_declarator")
    {
        return None;
    }
    let mut cursor = args.walk();
    let label = args
        .named_children(&mut cursor)
        .find(|a| matches!(a.kind(), "string" | "template_string"))
        .map(|s| cx.text(s));
    Some(match label {
        Some(label) => format!("{callee}({label})"),
        None => format!("{callee}(…)"),
    })
}

fn wrapped_component<'t>(cx: &Cx<'_>, value: Node<'t>) -> Option<Node<'t>> {
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = cx.text(value.child_by_field_name("function")?);
    if !COMPONENT_WRAPPERS.contains(&last_segment(&callee).as_str()) {
        return None;
    }
    let args = value.child_by_field_name("arguments")?;
    let first = args.named_child(0)?;
    FUNCTION_KINDS.contains(&first.kind()).then_some(first)
}

fn commonjs_export_name(cx: &Cx<'_>, left: Node<'_>) -> Option<String> {
    if left.kind() != "member_expression" {
        return None;
    }
    let object = cx.text(left.child_by_field_name("object")?);
    if object == "exports" || object == "module.exports" {
        Some(cx.text(left.child_by_field_name("property")?))
    } else {
        None
    }
}

fn require_call(cx: &Cx<'_>, node: Node<'_>) -> Option<(&'static str, String)> {
    if node.kind() != "call_expression" {
        return None;
    }
    let kind = match cx.text(node.child_by_field_name("function")?).as_str() {
        "require" => "require",
        "import" => "import",
        _ => return None,
    };
    let args = node.child_by_field_name("arguments")?;
    let string_arg = first_child_with_kinds(args, &["string"])?;
    Some((kind, strip_quotes(cx.text(string_arg))))
}

fn specifiers(cx: &Cx<'_>, list: Node<'_>, kind: &str) -> Vec<(String, String)> {
    let mut cursor = list.walk();
    let nodes: Vec<Node<'_>> = list
        .named_children(&mut cursor)
        .filter(|n| n.kind() == kind)
        .collect();
    nodes
        .into_iter()
        .filter_map(|spec| {
            let name = cx.text(spec.child_by_field_name("name")?);
            let alias = spec.child_by_field_name("alias").map(|a| cx.text(a));
            Some((alias.unwrap_or_else(|| name.clone()), name))
        })
        .collect()
}

fn object_pattern(cx: &Cx<'_>, pattern: Node<'_>) -> Vec<(String, String)> {
    let mut cursor = pattern.walk();
    let nodes: Vec<Node<'_>> = pattern.named_children(&mut cursor).collect();
    nodes
        .into_iter()
        .filter_map(|n| match n.kind() {
            "shorthand_property_identifier_pattern" => Some((cx.text(n), cx.text(n))),
            "pair_pattern" => Some((
                cx.text(n.child_by_field_name("value")?),
                cx.text(n.child_by_field_name("key")?),
            )),
            _ => None,
        })
        .collect()
}

fn object_literal(cx: &Cx<'_>, object: Node<'_>) -> Vec<(String, String)> {
    let mut cursor = object.walk();
    let nodes: Vec<Node<'_>> = object.named_children(&mut cursor).collect();
    nodes
        .into_iter()
        .filter_map(|n| match n.kind() {
            "shorthand_property_identifier" => Some((cx.text(n), cx.text(n))),
            "pair" => {
                let value = n.child_by_field_name("value")?;
                if value.kind() == "identifier" {
                    Some((cx.text(n.child_by_field_name("key")?), cx.text(value)))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
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

fn is_heritage_type(parent: Node<'_>) -> bool {
    parent.kind() == "generic_type"
        && parent
            .parent()
            .is_some_and(|g| matches!(g.kind(), "implements_clause" | "extends_type_clause"))
}

fn is_type_parameter(cx: &Cx<'_>, node: Node<'_>) -> bool {
    let name = cx.text(node);
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if let Some(params) = ancestor.child_by_field_name("type_parameters") {
            let mut cursor = params.walk();
            let declared = params
                .named_children(&mut cursor)
                .filter_map(|p| p.child_by_field_name("name"))
                .any(|n| cx.text(n) == name);
            if declared {
                return true;
            }
        }
        current = ancestor.parent();
    }
    false
}

fn has_token(node: Node<'_>, token: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| c.kind() == token);
    found
}

fn is_default_export_statement(node: Node<'_>) -> bool {
    node.kind() == "export_statement" && has_token(node, "default")
}

fn is_default_export(node: Node<'_>) -> bool {
    node.parent().is_some_and(is_default_export_statement)
}

fn last_segment(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

/// The first row of the TypeScript overload signatures (`function f(a: A);`)
/// directly before an implementation, so the symbol spans every declaration.
fn overload_start(cx: &Cx<'_>, function: Node<'_>, name: &str, signature_kind: &str) -> usize {
    let anchor = match function.parent() {
        Some(parent) if parent.kind() == "export_statement" => parent,
        _ => function,
    };
    let mut start = anchor.start_position().row;
    let mut previous = anchor.prev_named_sibling();
    while let Some(sibling) = previous {
        if sibling.kind() == "comment" {
            previous = sibling.prev_named_sibling();
            continue;
        }
        let signature = if sibling.kind() == "export_statement" {
            sibling.named_child(0)
        } else {
            Some(sibling)
        };
        let is_overload = signature.is_some_and(|sig| {
            sig.kind() == signature_kind
                && sig
                    .child_by_field_name("name")
                    .is_some_and(|n| cx.text(n) == name)
        });
        if !is_overload {
            break;
        }
        start = sibling.start_position().row;
        previous = sibling.prev_named_sibling();
    }
    start
}

/// A declarator in a `const`/`let`/`var` statement directly in the program,
/// optionally behind `export`.
fn is_module_level(declarator: Node<'_>) -> bool {
    let Some(declaration) = declarator.parent() else {
        return false;
    };
    if !matches!(
        declaration.kind(),
        "lexical_declaration" | "variable_declaration"
    ) {
        return false;
    }
    match declaration.parent() {
        Some(parent) if parent.kind() == "program" => true,
        Some(parent) if parent.kind() == "export_statement" => parent
            .parent()
            .is_some_and(|grandparent| grandparent.kind() == "program"),
        _ => false,
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
