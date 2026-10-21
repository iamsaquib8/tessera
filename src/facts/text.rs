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

const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// OpenAPI 3 and Swagger 2 `paths`, from JSON or YAML: one `route_def` per
/// operation.
pub fn openapi(content: &str) -> Vec<IndexedFact> {
    let mut out = Vec::new();
    let fact = |method: &str, path: &str, line: usize, evidence: &str| {
        let normalised = super::http::normalise(path).unwrap_or_else(|| path.to_string());
        let method = method.to_ascii_uppercase();
        IndexedFact::new(
            kind::ROUTE_DEF,
            super::http::key(&method, &normalised),
            format!("{method} {normalised}"),
            line,
            "openapi",
            evidence,
        )
    };
    if content.trim_start().starts_with('{') {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            return out;
        };
        let Some(paths) = value["paths"].as_object() else {
            return out;
        };
        for (path, operations) in paths {
            let line = content
                .find(&format!("\"{path}\""))
                .map_or(1, |at| content[..at].matches('\n').count() + 1);
            for method in HTTP_METHODS {
                if operations.get(*method).is_some() {
                    out.push(fact(method, path, line, path));
                }
            }
        }
        return out;
    }
    // YAML: keys under `paths:`, then method keys one level deeper.
    let mut in_paths = false;
    let mut path_indent = None;
    let mut current: Option<(String, usize)> = None;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with('#') || trimmed.trim().is_empty() {
            continue;
        }
        let indent = trimmed.len() - trimmed.trim_start().len();
        if indent == 0 {
            in_paths = trimmed == "paths:";
            current = None;
            continue;
        }
        if !in_paths {
            continue;
        }
        let Some(key) = trimmed
            .trim_start()
            .strip_suffix(':')
            .or_else(|| trimmed.trim_start().split_once(": ").map(|(k, _)| k))
        else {
            continue;
        };
        let key = key.trim_matches(['"', '\'']);
        let path_level = *path_indent.get_or_insert(indent);
        if indent == path_level && key.starts_with('/') {
            current = Some((key.to_string(), indent));
        } else if let Some((path, path_at)) = &current {
            if indent > *path_at && HTTP_METHODS.contains(&key) && indent <= path_at + 4 {
                out.push(fact(key, path, index + 1, trimmed.trim()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_operations_in_yaml_and_json() {
        let yaml = "openapi: 3.0.0\npaths:\n  /users/{id}:\n    get:\n      summary: x\n    delete:\n      summary: y\n  '/users':\n    post: {}\ncomponents:\n  schemas: {}\n";
        let keys: Vec<(String, usize)> =
            openapi(yaml).into_iter().map(|f| (f.key, f.line)).collect();
        assert_eq!(
            keys,
            vec![
                ("http GET /users/{}".to_string(), 4),
                ("http DELETE /users/{}".to_string(), 6),
                ("http POST /users".to_string(), 9)
            ]
        );
        let json = r#"{"paths": {"/pets/{petId}": {"get": {}, "parameters": []}}}"#;
        let keys: Vec<String> = openapi(json).into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["http GET /pets/{}"]);
    }

    #[test]
    fn rpcs_belong_to_their_service() {
        let facts = proto(
            "package shop;\nservice Cart {\n  rpc Get(Req) returns (Res) {}\n  // rpc Old(Req) returns (Res);\n}\nmessage Req {\n  string rpc = 1;\n}\nservice Ads {\n  rpc List(A) returns (B);\n}\n",
        );
        let keys: Vec<(&str, usize)> = facts.iter().map(|f| (f.key.as_str(), f.line)).collect();
        assert_eq!(keys, vec![("grpc cart/get", 3), ("grpc ads/list", 10)]);
    }
}
