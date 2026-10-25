//! Contract facts: what a file serves or calls across a service boundary
//! (gRPC methods, HTTP routes, topics, tables). Facts are extracted per file
//! at index time; two facts with the same key are a link, joined at query
//! time, so incremental indexing needs no link pass.

pub mod config;
pub mod grpc;
pub mod http;
pub mod packs;
pub mod text;

use std::path::Path;

use serde::Serialize;
use tree_sitter::Tree;

use crate::types::Language;

/// Fact kinds. Providers and consumers of one contract share a key.
pub mod kind {
    /// A gRPC method declared in a `.proto` file.
    pub const RPC_DEF: &str = "rpc_def";
    /// A file registers or implements a gRPC service.
    pub const RPC_SERVER: &str = "rpc_server";
    /// A method implementing a gRPC method.
    pub const RPC_IMPL: &str = "rpc_impl";
    /// A file creates a client (stub) for a gRPC service.
    pub const RPC_CLIENT: &str = "rpc_client";
    /// A call that may be a gRPC method call; kept when the file has a
    /// client for a service with that method.
    pub const RPC_CALL: &str = "rpc_call";
    /// An HTTP route a server handles.
    pub const ROUTE: &str = "route";
    /// An HTTP request a client makes.
    pub const HTTP_CALL: &str = "http_call";
    /// An HTTP operation declared in an OpenAPI document.
    pub const ROUTE_DEF: &str = "route_def";
    /// Code that publishes to a topic or queue.
    pub const PRODUCER: &str = "producer";
    /// Code that listens on a topic or queue.
    pub const CONSUMER: &str = "consumer";
    /// A table a migration creates or alters.
    pub const TABLE: &str = "table";
    /// An ORM model mapped to a table.
    pub const ENTITY: &str = "entity";
    /// A SQL statement in code that reads or writes a table.
    pub const SQL: &str = "sql";
    /// A GraphQL root field in a schema: `type Query { user }`.
    pub const GQL_DEF: &str = "graphql_def";
    /// A resolver for a root field.
    pub const GQL_RESOLVER: &str = "graphql_resolver";
    /// An operation that selects a root field.
    pub const GQL_OP: &str = "graphql_op";
    /// A file that sets a configuration key (`.env`, Compose, Dockerfile,
    /// Spring properties). The value is never kept.
    pub const CONFIG_SET: &str = "config_set";
    /// Code that reads a configuration key.
    pub const CONFIG_READ: &str = "config_read";
    /// A router mounted under a path (`app.use("/api", users)`): key
    /// `mount <local name>`, name the prefix.
    pub const MOUNT: &str = "mount";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexedFact {
    pub kind: &'static str,
    /// Normalised: `grpc cartservice/getcart`, `grpc */getcart` when the
    /// service is not known in this file, `grpc cartservice` for a whole
    /// service.
    pub key: String,
    /// The key as written, for display: `CartService/GetCart`.
    pub name: String,
    pub line: usize,
    pub confidence: &'static str,
    /// The rule that produced it, such as `grpc.go.client`.
    pub rule: &'static str,
    /// The matched source text, trimmed.
    pub evidence: String,
    /// The handler's name when it is not the code around the fact
    /// (`addService(…, {convert})` names the function `convert`).
    pub handler: Option<String>,
}

impl IndexedFact {
    pub fn new(
        kind: &'static str,
        key: String,
        name: String,
        line: usize,
        rule: &'static str,
        evidence: &str,
    ) -> Self {
        Self {
            kind,
            key,
            name,
            line,
            confidence: "high",
            rule,
            evidence: trim_evidence(evidence),
            handler: None,
        }
    }
}

/// Evidence is one line of at most 160 characters.
fn trim_evidence(text: &str) -> String {
    let line = text.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    let mut out: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        out.push('…');
    }
    out
}

/// Facts in one file. `tree` is the parse tree for source languages and
/// `None` for data files.
pub fn extract(
    language: Language,
    path: &str,
    content: &str,
    tree: Option<&Tree>,
) -> Vec<IndexedFact> {
    if is_generated(path, content) {
        return Vec::new();
    }
    let mut facts = match (language, tree) {
        (Language::Proto, _) => text::proto(content),
        (Language::OpenApi, _) => text::openapi(content),
        (Language::Sql, _) => text::sql(content),
        (Language::Prisma, _) => text::prisma(content),
        (Language::GraphQl, _) => text::graphql(content, 0),
        (Language::Config, _) => config::file(path, content),
        (_, Some(tree)) => {
            let mut facts = grpc::code(language, content, tree);
            facts.extend(packs::run(language, content, tree));
            facts.extend(text::sql_strings(content));
            facts.extend(text::graphql_in_code(content));
            facts.extend(config::reads(content));
            if matches!(
                language,
                Language::JavaScript | Language::TypeScript | Language::Tsx
            ) {
                facts.extend(http::next_routes(path, content));
            }
            facts
        }
        _ => Vec::new(),
    };
    facts.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.key.cmp(&b.key)));
    facts.dedup();
    facts
}

/// Code generated from a schema (protobuf stubs and the like) defines the
/// contract's plumbing, not a server or a client.
pub fn is_generated(path: &str, content: &str) -> bool {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    const SUFFIXES: &[&str] = &[
        ".pb.go",
        "_pb2.py",
        "_pb2_grpc.py",
        "_pb2.pyi",
        "_pb.js",
        "_grpc_pb.js",
        "_pb.ts",
        "_grpc_pb.ts",
        "Grpc.java",
        "Grpc.cs",
        ".pb.cs",
    ];
    if SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
        return true;
    }
    let head: String = content.chars().take(2_000).collect();
    [
        "DO NOT EDIT",
        "<auto-generated",
        "@generated",
        "Generated by the protocol buffer compiler",
    ]
    .iter()
    .any(|marker| head.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_files_are_recognised() {
        assert!(is_generated("a/demo_grpc.pb.go", ""));
        assert!(is_generated(
            "a/x.py",
            "# Generated by the protocol buffer compiler.  DO NOT EDIT!"
        ));
        assert!(!is_generated("a/server.go", "package main"));
    }
}
