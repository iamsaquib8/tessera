//! Facts in data files, read as text: `.proto` services.

use std::sync::LazyLock;

use regex::Regex;

use super::{kind, IndexedFact};

static SERVICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*service\s+(\w+)\s*\{").expect("valid regex"));
static RPC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*rpc\s+(\w+)\s*\(").expect("valid regex"));

/// One `rpc_def` per rpc, keyed by service and method.
pub fn proto(content: &str) -> Vec<IndexedFact> {
    let mut out = Vec::new();
    let mut service: Option<(String, i32)> = None;
    let mut depth = 0i32;
    for (index, raw) in content.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or_default();
        if let Some(captures) = SERVICE.captures(line) {
            service = Some((captures[1].to_string(), depth));
        } else if let (Some(captures), Some((name, _))) = (RPC.captures(line), &service) {
            out.push(IndexedFact::new(
                kind::RPC_DEF,
                format!(
                    "grpc {}/{}",
                    name.to_lowercase(),
                    captures[1].to_lowercase()
                ),
                format!("{name}/{}", &captures[1]),
                index + 1,
                "grpc.proto.rpc",
                raw,
            ));
        }
        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if service.as_ref().is_some_and(|(_, start)| depth <= *start) {
            service = None;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpcs_belong_to_their_service() {
        let facts = proto(
            "package shop;\nservice Cart {\n  rpc Get(Req) returns (Res) {}\n  // rpc Old(Req) returns (Res);\n}\nmessage Req {\n  string rpc = 1;\n}\nservice Ads {\n  rpc List(A) returns (B);\n}\n",
        );
        let keys: Vec<(&str, usize)> = facts.iter().map(|f| (f.key.as_str(), f.line)).collect();
        assert_eq!(keys, vec![("grpc cart/get", 3), ("grpc ads/list", 10)]);
    }
}
