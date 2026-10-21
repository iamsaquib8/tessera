//! gRPC servers and clients in code, by the names protobuf code generators
//! give them in each language. Method calls are kept only in files that
//! create a client, and both sides are matched against `.proto` methods at
//! query time, so a method call is a gRPC call only when a service the file
//! is a client of declares that method.

use std::sync::LazyLock;

use regex::Regex;
use tree_sitter::{Node, Tree};

use super::{kind, IndexedFact};
use crate::types::Language;

static GO_CLIENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^New(\w+)Client$").unwrap());
static GO_SERVER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Register(\w+)Server$").unwrap());
static PY_STUB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)Stub$").unwrap());
static PY_SERVICER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)Servicer$").unwrap());
static PY_ADD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^add_(\w+)Servicer_to_server$").unwrap());
static JAVA_GRPC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)Grpc$").unwrap());
static JAVA_IMPL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)ImplBase$").unwrap());
static CS_CLIENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)Client$").unwrap());
static CS_BASE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+)Base$").unwrap());
const JAVA_STUBS: &[&str] = &[
    "newBlockingStub",
    "newStub",
    "newFutureStub",
    "newBlockingV2Stub",
];

struct Call {
    method: String,
    line: usize,
    /// The service, when the receiver is a client constructor.
    service: Option<String>,
    /// The receiver's last name segment: `stub` in `self.stub.GetCart()`.
    receiver: String,
    evidence: String,
}

/// A Python class, kept until the file is read so subclasses of a
/// servicer count too.
struct PyClass {
    name: String,
    bases: Vec<String>,
    methods: Vec<(String, usize, String)>,
    line: usize,
    evidence: String,
}

struct Scan<'a> {
    language: Language,
    source: &'a str,
    facts: Vec<IndexedFact>,
    calls: Vec<Call>,
    /// Variables holding a client: name → service.
    stubs: Vec<(String, String)>,
    classes: Vec<PyClass>,
}

impl Scan<'_> {
    fn text(&self, node: Node<'_>) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or_default()
    }

    fn line_text(&self, node: Node<'_>) -> String {
        self.source
            .lines()
            .nth(node.start_position().row)
            .unwrap_or_default()
            .to_string()
    }

    fn push(&mut self, kind: &'static str, name: String, node: Node<'_>, rule: &'static str) {
        let evidence = self.line_text(node);
        let key = format!("grpc {}", name.to_lowercase());
        self.facts.push(IndexedFact::new(
            kind,
            key,
            name,
            node.start_position().row + 1,
            rule,
            &evidence,
        ));
    }

    /// A client for `service` created at `node`; remembers the variable it
    /// is assigned to.
    fn client(&mut self, service: String, node: Node<'_>, rule: &'static str) {
        if let Some(variable) = assigned_to(self, node) {
            self.stubs.push((variable, service.clone()));
        }
        self.push(kind::RPC_CLIENT, service, node, rule);
    }

    fn call(&mut self, method: Node<'_>, receiver: Option<Node<'_>>, service: Option<String>) {
        let receiver = receiver
            .map(|r| last_segment(self.text(r)).to_string())
            .unwrap_or_default();
        let mut name = self.text(method).to_string();
        if self.language == Language::CSharp {
            if let Some(stripped) = name.strip_suffix("Async") {
                name = stripped.to_string();
            }
        }
        if name.is_empty() {
            return;
        }
        let evidence = self.line_text(method);
        self.calls.push(Call {
            method: name,
            line: method.start_position().row + 1,
            service,
            receiver,
            evidence,
        });
    }
}

/// The variable or field a value is assigned to: `stub` in
/// `self.stub = XStub(ch)`, `cl` in `cl := pb.NewXClient(conn)`.
fn assigned_to(scan: &Scan<'_>, node: Node<'_>) -> Option<String> {
    let mut current = node;
    for _ in 0..4 {
        let parent = current.parent()?;
        let target = match parent.kind() {
            "short_var_declaration"
            | "assignment_statement"
            | "assignment"
            | "assignment_expression" => parent.child_by_field_name("left"),
            "variable_declarator" => parent
                .child_by_field_name("name")
                .or_else(|| parent.named_child(0)),
            "var_spec" => parent.child_by_field_name("name"),
            // `NewXClient(conn).GetCart(…)`: the client is used, not stored.
            "selector_expression"
            | "member_expression"
            | "attribute"
            | "member_access_expression"
            | "method_invocation"
            | "field_access" => return None,
            _ => {
                current = parent;
                continue;
            }
        };
        let text = scan.text(target?);
        let first = text.split(',').next().unwrap_or(text);
        return Some(last_segment(first).to_string()).filter(|name| !name.is_empty());
    }
    None
}

fn last_segment(text: &str) -> &str {
    text.rsplit(['.', ':']).next().unwrap_or(text).trim()
}

fn service_of(regex: &Regex, name: &str) -> Option<String> {
    regex
        .captures(last_segment(name))
        .map(|c| c[1].to_string())
        .filter(|s| !s.is_empty())
}

/// Whether a file can hold gRPC code at all; most files are skipped
/// without a walk.
fn relevant(language: Language, source: &str) -> bool {
    match language {
        Language::Go => source.contains("Client(") || source.contains("context.Context"),
        Language::JavaScript | Language::TypeScript | Language::Tsx => {
            source.contains("grpc") || source.contains("Grpc")
        }
        Language::Java | Language::CSharp => source.contains("Grpc"),
        Language::Python => source.contains("grpc") || source.contains("_pb2"),
        _ => false,
    }
}

pub fn code(language: Language, source: &str, tree: &Tree) -> Vec<IndexedFact> {
    if !relevant(language, source) {
        return Vec::new();
    }
    let mut scan = Scan {
        language,
        source,
        facts: Vec::new(),
        calls: Vec::new(),
        stubs: Vec::new(),
        classes: Vec::new(),
    };
    let mut cursor = tree.walk();
    loop {
        visit(&mut scan, cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return finish(scan);
            }
        }
    }
}

fn finish(mut scan: Scan<'_>) -> Vec<IndexedFact> {
    for call in std::mem::take(&mut scan.calls) {
        let service = call.service.or_else(|| {
            scan.stubs
                .iter()
                .find(|(variable, _)| *variable == call.receiver)
                .map(|(_, service)| service.clone())
        });
        let Some(service) = service else {
            continue;
        };
        scan.facts.push(IndexedFact::new(
            kind::RPC_CALL,
            format!(
                "grpc {}/{}",
                service.to_lowercase(),
                call.method.to_lowercase()
            ),
            format!("{service}/{}", call.method),
            call.line,
            "grpc.call",
            &call.evidence,
        ));
    }
    python_servicers(&mut scan);
    scan.facts
}

/// Python classes deriving from a generated `XServicer`, directly or
/// through another class in the file.
fn python_servicers(scan: &mut Scan<'_>) {
    let mut services: Vec<Option<String>> = scan
        .classes
        .iter()
        .map(|class| {
            class
                .bases
                .iter()
                .find_map(|base| service_of(&PY_SERVICER, base))
        })
        .collect();
    loop {
        let mut changed = false;
        for index in 0..scan.classes.len() {
            if services[index].is_some() {
                continue;
            }
            let inherited = scan.classes[index].bases.iter().find_map(|base| {
                scan.classes
                    .iter()
                    .position(|c| c.name == last_segment(base))
                    .and_then(|parent| services[parent].clone())
            });
            if inherited.is_some() {
                services[index] = inherited;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (class, service) in std::mem::take(&mut scan.classes).into_iter().zip(services) {
        let Some(service) = service else {
            continue;
        };
        scan.facts.push(IndexedFact::new(
            kind::RPC_SERVER,
            format!("grpc {}", service.to_lowercase()),
            service.clone(),
            class.line,
            "grpc.python.servicer",
            &class.evidence,
        ));
        for (method, line, evidence) in class.methods {
            scan.facts.push(IndexedFact::new(
                kind::RPC_IMPL,
                format!("grpc {}/{}", service.to_lowercase(), method.to_lowercase()),
                format!("{service}/{method}"),
                line,
                "grpc.python.impl",
                &evidence,
            ));
        }
    }
}

fn visit(scan: &mut Scan<'_>, node: Node<'_>) {
    match scan.language {
        Language::Go => go(scan, node),
        Language::Python => python(scan, node),
        Language::Java => java(scan, node),
        Language::CSharp => csharp(scan, node),
        Language::JavaScript | Language::TypeScript | Language::Tsx => javascript(scan, node),
        _ => {}
    }
}

fn go(scan: &mut Scan<'_>, node: Node<'_>) {
    match node.kind() {
        "call_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            if function.kind() != "selector_expression" {
                return;
            }
            let Some(field) = function.child_by_field_name("field") else {
                return;
            };
            let name = scan.text(field).to_string();
            if let Some(service) = service_of(&GO_CLIENT, &name) {
                scan.client(service, node, "grpc.go.client");
            } else if let Some(service) = service_of(&GO_SERVER, &name) {
                scan.push(kind::RPC_SERVER, service.clone(), field, "grpc.go.server");
            } else if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                let chained = function
                    .child_by_field_name("operand")
                    .filter(|operand| operand.kind() == "call_expression")
                    .and_then(|call| call.child_by_field_name("function"))
                    .and_then(|f| f.child_by_field_name("field"))
                    .and_then(|f| service_of(&GO_CLIENT, scan.text(f)));
                scan.call(field, function.child_by_field_name("operand"), chained);
            }
        }
        // `func (s *server) GetQuote(ctx context.Context, in *pb.GetQuoteRequest)`:
        // the shape of a generated server interface method.
        "method_declaration" => {
            let (Some(name), Some(params)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("parameters"),
            ) else {
                return;
            };
            let mut cursor = params.walk();
            let types: Vec<String> = params
                .named_children(&mut cursor)
                .filter(|p| p.kind() == "parameter_declaration")
                .filter_map(|p| p.child_by_field_name("type"))
                .map(|t| scan.text(t).replace(' ', ""))
                .collect();
            let shaped = types.len() == 2
                && types[0] == "context.Context"
                && types[1].starts_with('*')
                && types[1].contains('.');
            let method = scan.text(name).to_string();
            if shaped && method.starts_with(|c: char| c.is_ascii_uppercase()) {
                scan.push(kind::RPC_IMPL, format!("*/{method}"), name, "grpc.go.impl");
            }
        }
        _ => {}
    }
}

fn python(scan: &mut Scan<'_>, node: Node<'_>) {
    match node.kind() {
        "class_definition" => {
            let (Some(name), Some(body)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("body"),
            ) else {
                return;
            };
            let bases = node
                .child_by_field_name("superclasses")
                .map(|bases| {
                    let mut cursor = bases.walk();
                    let names: Vec<String> = bases
                        .named_children(&mut cursor)
                        .map(|base| scan.text(base).to_string())
                        .collect();
                    names
                })
                .unwrap_or_default();
            let mut methods = Vec::new();
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                let function = match child.kind() {
                    "function_definition" => Some(child),
                    "decorated_definition" => child.child_by_field_name("definition"),
                    _ => None,
                };
                let Some(method) = function.and_then(|f| f.child_by_field_name("name")) else {
                    continue;
                };
                let text = scan.text(method).to_string();
                if !text.starts_with('_') {
                    methods.push((
                        text,
                        method.start_position().row + 1,
                        scan.line_text(method),
                    ));
                }
            }
            let class = PyClass {
                name: scan.text(name).to_string(),
                bases,
                methods,
                line: node.start_position().row + 1,
                evidence: scan.line_text(node),
            };
            scan.classes.push(class);
        }
        "call" => {
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let name = scan.text(function).to_string();
            if let Some(service) = service_of(&PY_STUB, &name) {
                scan.client(service, node, "grpc.python.stub");
            } else if let Some(service) = service_of(&PY_ADD, &name) {
                scan.push(
                    kind::RPC_SERVER,
                    service.clone(),
                    function,
                    "grpc.python.server",
                );
            } else if function.kind() == "attribute" {
                let Some(method) = function.child_by_field_name("attribute") else {
                    return;
                };
                if scan
                    .text(method)
                    .starts_with(|c: char| c.is_ascii_uppercase())
                {
                    let chained = function
                        .child_by_field_name("object")
                        .filter(|object| object.kind() == "call")
                        .and_then(|call| call.child_by_field_name("function"))
                        .and_then(|f| service_of(&PY_STUB, scan.text(f)));
                    scan.call(method, function.child_by_field_name("object"), chained);
                }
            }
        }
        _ => {}
    }
}

fn java(scan: &mut Scan<'_>, node: Node<'_>) {
    match node.kind() {
        "class_declaration" => {
            let service = node
                .child_by_field_name("superclass")
                .and_then(|s| s.named_child(0))
                .and_then(|t| service_of(&JAVA_IMPL, scan.text(t).split('<').next().unwrap_or("")));
            let (Some(service), Some(body)) = (service, node.child_by_field_name("body")) else {
                return;
            };
            scan.push(
                kind::RPC_SERVER,
                service.clone(),
                node,
                "grpc.java.impl_base",
            );
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                if child.kind() != "method_declaration" {
                    continue;
                }
                if let Some(name) = child.child_by_field_name("name") {
                    let method = scan.text(name).to_string();
                    scan.push(
                        kind::RPC_IMPL,
                        format!("{service}/{method}"),
                        name,
                        "grpc.java.impl",
                    );
                }
            }
        }
        "method_invocation" => {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            let method = scan.text(name).to_string();
            let object = node.child_by_field_name("object");
            if JAVA_STUBS.contains(&method.as_str()) {
                if let Some(service) = object.and_then(|o| service_of(&JAVA_GRPC, scan.text(o))) {
                    scan.client(service, node, "grpc.java.stub");
                }
            } else if let Some(object) = object {
                let chained = Some(object)
                    .filter(|o| o.kind() == "method_invocation")
                    .filter(|o| {
                        o.child_by_field_name("name")
                            .is_some_and(|n| JAVA_STUBS.contains(&scan.text(n)))
                    })
                    .and_then(|o| o.child_by_field_name("object"))
                    .and_then(|o| service_of(&JAVA_GRPC, scan.text(o)));
                scan.call(name, Some(object), chained);
            }
        }
        _ => {}
    }
}

fn csharp(scan: &mut Scan<'_>, node: Node<'_>) {
    match node.kind() {
        "class_declaration" => {
            let mut cursor = node.walk();
            let bases = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "base_list");
            let service = bases.and_then(|bases| {
                let mut cursor = bases.walk();
                let found = bases.named_children(&mut cursor).find_map(|base| {
                    service_of(&CS_BASE, scan.text(base).split('<').next().unwrap_or(""))
                });
                found
            });
            let Some(service) = service else {
                return;
            };
            let mut cursor = node.walk();
            let Some(body) = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "declaration_list")
            else {
                return;
            };
            scan.push(kind::RPC_SERVER, service.clone(), node, "grpc.csharp.base");
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                if child.kind() != "method_declaration" {
                    continue;
                }
                let mut modifiers = child.walk();
                let overrides = child
                    .named_children(&mut modifiers)
                    .any(|m| m.kind() == "modifier" && scan.text(m) == "override");
                if let (true, Some(name)) = (overrides, child.child_by_field_name("name")) {
                    let method = scan.text(name).to_string();
                    scan.push(
                        kind::RPC_IMPL,
                        format!("{service}/{method}"),
                        name,
                        "grpc.csharp.impl",
                    );
                }
            }
        }
        // `new CartService.CartServiceClient(channel)`: the generated client
        // class nested in the service's class.
        "object_creation_expression" => {
            let Some(ty) = node.child_by_field_name("type") else {
                return;
            };
            let text = scan.text(ty).to_string();
            let mut parts = text.rsplit('.');
            let (Some(name), Some(outer)) = (parts.next(), parts.next()) else {
                return;
            };
            if let Some(service) = service_of(&CS_CLIENT, name) {
                if service == outer.trim() {
                    scan.client(service, node, "grpc.csharp.client");
                }
            }
        }
        "invocation_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            if function.kind() == "member_access_expression" {
                if let Some(name) = function.child_by_field_name("name") {
                    scan.call(name, function.child_by_field_name("expression"), None);
                }
            }
        }
        _ => {}
    }
}

fn javascript(scan: &mut Scan<'_>, node: Node<'_>) {
    match node.kind() {
        "call_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            if function.kind() != "member_expression" {
                return;
            }
            let Some(property) = function.child_by_field_name("property") else {
                return;
            };
            if scan.text(property) == "addService" {
                add_service(scan, node);
            } else {
                scan.call(property, function.child_by_field_name("object"), None);
            }
        }
        // `new shopProto.CurrencyService(address, credentials)`.
        "new_expression" => {
            let Some(constructor) = node.child_by_field_name("constructor") else {
                return;
            };
            if constructor.kind() != "member_expression" {
                return;
            }
            let name = last_segment(scan.text(constructor)).to_string();
            if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                scan.client(name, node, "grpc.node.client");
            }
        }
        _ => {}
    }
}

/// `server.addService(proto.X.service, {method: handler, other})`.
fn add_service(scan: &mut Scan<'_>, call: Node<'_>) {
    let Some(args) = call.child_by_field_name("arguments") else {
        return;
    };
    let (Some(definition), Some(handlers)) = (args.named_child(0), args.named_child(1)) else {
        return;
    };
    let definition = scan.text(definition).to_string();
    let Some(service) = definition
        .strip_suffix(".service")
        .map(|s| last_segment(s).to_string())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    scan.push(
        kind::RPC_SERVER,
        service.clone(),
        call,
        "grpc.node.add_service",
    );
    if handlers.kind() != "object" {
        return;
    }
    let mut cursor = handlers.walk();
    for entry in handlers.named_children(&mut cursor) {
        let (key, handler) = match entry.kind() {
            "shorthand_property_identifier" => (entry, Some(scan.text(entry).to_string())),
            "pair" => {
                let Some(key) = entry.child_by_field_name("key") else {
                    continue;
                };
                let handler = entry
                    .child_by_field_name("value")
                    .and_then(|value| handler_name(scan, value));
                (key, handler)
            }
            _ => continue,
        };
        let method = scan.text(key).trim_matches(['"', '\'']).to_string();
        let mut fact = IndexedFact::new(
            kind::RPC_IMPL,
            format!("grpc {}/{}", service.to_lowercase(), method.to_lowercase()),
            format!("{service}/{method}"),
            key.start_position().row + 1,
            "grpc.node.impl",
            &scan.line_text(key),
        );
        fact.handler = handler;
        scan.facts.push(fact);
    }
}

/// The function a handler value names: `convert`, `Server.convert`, or
/// `Server.convert.bind(this)`.
fn handler_name(scan: &Scan<'_>, value: Node<'_>) -> Option<String> {
    match value.kind() {
        "identifier" => Some(scan.text(value).to_string()),
        "member_expression" => value
            .child_by_field_name("property")
            .map(|p| scan.text(p).to_string()),
        "call_expression" => {
            let function = value.child_by_field_name("function")?;
            let bound = function.kind() == "member_expression"
                && function
                    .child_by_field_name("property")
                    .is_some_and(|p| scan.text(p) == "bind");
            if bound {
                handler_name(scan, function.child_by_field_name("object")?)
            } else {
                None
            }
        }
        _ => None,
    }
}
