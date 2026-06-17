use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db;
use crate::query;
use crate::types::{Language, SearchOptions, UnusedOptions};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub fn serve_stdio(db_path: &Path) -> Result<()> {
    let conn = db::open(db_path)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => handle_request(&conn, db_path, request),
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0",
                id: None,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: error.to_string(),
                }),
            },
        };
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }
    Ok(())
}

fn handle_request(conn: &Connection, db_path: &Path, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "tessera", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "tools": {} }
        })),
        "notifications/initialized" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(conn, db_path, &request.params),
        _ => Err(format!("unknown method: {}", request.method)),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(value),
            error: None,
        },
        Err(message) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        },
    }
}

fn call_tool(
    conn: &Connection,
    db_path: &Path,
    params: &Value,
) -> std::result::Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call params.name is required".to_string())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "find_definition" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::find_definition_conn(conn, &symbol).map_err(to_string)?)
        }
        "find_references" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::find_references_conn(conn, &symbol).map_err(to_string)?)
        }
        "get_outline" => {
            let path = arg_string(&args, "path")?;
            serde_json::to_value(
                query::get_outline_conn(conn, Path::new(&path)).map_err(to_string)?,
            )
        }
        "expand_symbol" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::expand_symbol_conn(conn, &symbol).map_err(to_string)?)
        }
        "impact" => {
            let symbol = arg_string(&args, "symbol")?;
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(4) as usize;
            serde_json::to_value(query::impact_conn(conn, &symbol, depth).map_err(to_string)?)
        }
        "validate" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::validate_conn(conn, &symbol).map_err(to_string)?)
        }
        "validate_snippet" => {
            let code = arg_string(&args, "code")?;
            let language = args
                .get("language")
                .and_then(Value::as_str)
                .and_then(Language::from_name)
                .ok_or_else(|| {
                    "argument `language` is required (typescript|tsx|javascript|python|go|rust|java|c|cpp|csharp|ruby|php)"
                        .to_string()
                })?;
            serde_json::to_value(
                query::validate_snippet_conn(conn, &code, language).map_err(to_string)?,
            )
        }
        "stats" => serde_json::to_value(query::stats_conn(conn, db_path).map_err(to_string)?),
        "tests_for" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::tests_for_conn(conn, &symbol).map_err(to_string)?)
        }
        "connect" => {
            let from = arg_string(&args, "from")?;
            let to = arg_string(&args, "to")?;
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(8) as usize;
            serde_json::to_value(query::connect_conn(conn, &from, &to, depth).map_err(to_string)?)
        }
        "export" => {
            let format = args
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("mermaid");
            let from = args.get("from").and_then(Value::as_str);
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(3) as usize;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(800) as usize;
            serde_json::to_value(
                query::export_conn(conn, format, from, depth, limit).map_err(to_string)?,
            )
        }
        "context_pack" => {
            let symbol = arg_string(&args, "symbol")?;
            let budget = args
                .get("budget")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(1500);
            serde_json::to_value(
                query::context_pack_conn(conn, &symbol, budget).map_err(to_string)?,
            )
        }
        "diff_impact" => {
            let from = arg_string(&args, "from")?;
            let to = args
                .get("to")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let depth = args
                .get("depth")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(3);
            serde_json::to_value(
                query::diff_impact_conn(conn, &from, to.as_deref(), depth).map_err(to_string)?,
            )
        }
        "imports" => {
            let path = arg_string(&args, "path")?;
            serde_json::to_value(query::imports_conn(conn, &path).map_err(to_string)?)
        }
        "imported_by" => {
            let source = arg_string(&args, "source")?;
            serde_json::to_value(query::imported_by_conn(conn, &source).map_err(to_string)?)
        }
        "signature" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::signature_conn(conn, &symbol).map_err(to_string)?)
        }
        "siblings" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::siblings_conn(conn, &symbol).map_err(to_string)?)
        }
        "search" => {
            let pattern = arg_string(&args, "pattern")?;
            let kinds = string_array(&args, "kinds");
            let languages = string_array(&args, "languages");
            let exported = args.get("exported").and_then(Value::as_bool);
            let path_prefix = args
                .get("path_prefix")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(50);
            let options = SearchOptions {
                kinds,
                languages,
                exported,
                path_prefix,
                limit,
            };
            serde_json::to_value(query::search_conn(conn, &pattern, options).map_err(to_string)?)
        }
        "unused" => {
            let kinds = string_array(&args, "kinds");
            let languages = string_array(&args, "languages");
            let exported = args.get("exported").and_then(Value::as_bool);
            let path_prefix = args
                .get("path_prefix")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(50);
            let options = UnusedOptions {
                kinds,
                languages,
                exported,
                path_prefix,
                limit,
            };
            serde_json::to_value(query::unused_conn(conn, options).map_err(to_string)?)
        }
        _ => return Err(format!("unknown tool: {name}")),
    }
    .map_err(to_string)?;

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&result).map_err(to_string)?
            }
        ],
        "structuredContent": result
    }))
}

fn arg_string(args: &Value, key: &str) -> std::result::Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("argument `{key}` is required"))
}

fn string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn tools() -> Value {
    json!([
        {
            "name": "find_definition",
            "description": "Find exact file/line definitions and signatures for a symbol.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "find_references",
            "description": "Find callers or reference sites for a symbol with one-line context.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "get_outline",
            "description": "Return a semantic outline for a file or directory without function bodies.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        },
        {
            "name": "expand_symbol",
            "description": "Return a symbol body plus immediate dependencies.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "impact",
            "description": "Return transitive callers ranked by personalised PageRank with a criticality breakdown.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 10 }
                },
                "required": ["symbol"]
            }
        },
        {
            "name": "validate",
            "description": "Check whether a symbol exists in the graph; return near-miss candidates with confidence scores when it doesn't.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "validate_snippet",
            "description": "Parse a code snippet and check every call against the graph. Returns per-call resolution status plus near-miss suggestions for unresolved calls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "language": {
                        "type": "string",
                        "enum": ["typescript", "tsx", "javascript", "python", "go", "rust", "java", "c", "cpp", "csharp", "ruby", "php"]
                    }
                },
                "required": ["code", "language"]
            }
        },
        {
            "name": "stats",
            "description": "Summary statistics about the index: counts, languages, kinds, top fan-out symbols.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "tests_for",
            "description": "Return the minimal set of tests whose call graph transitively touches the given symbol.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "context_pack",
            "description": "Bundle a symbol's body + immediate-dep signatures + top caller signatures + relevant tests into a single token-budgeted response. Replaces 3-5 round trips an agent would otherwise make to prep an edit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "budget": { "type": "integer", "minimum": 200, "maximum": 8000, "description": "Approximate token budget for the entire response. Default 1500." }
                },
                "required": ["symbol"]
            }
        },
        {
            "name": "diff_impact",
            "description": "Given a git ref range, return the symbols that changed plus the PageRank-impacted callers downstream. The PR-review token saver.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Base ref (e.g. main, origin/main, HEAD~3)" },
                    "to": { "type": "string", "description": "Tip ref. Defaults to HEAD." },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 6 }
                },
                "required": ["from"]
            }
        },
        {
            "name": "imports",
            "description": "List the imports declared in a file or directory (use, import, require, etc.).",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }
        },
        {
            "name": "imported_by",
            "description": "List files that import a given module / source path. Inverse of `imports`.",
            "inputSchema": {
                "type": "object",
                "properties": { "source": { "type": "string" } },
                "required": ["source"]
            }
        },
        {
            "name": "signature",
            "description": "Ultra-cheap signature lookup. For class/struct/interface/trait/enum/record/impl, also returns child member signatures (no bodies).",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "siblings",
            "description": "Symbols that share callers with the target, ranked by overlap count. Useful for finding the cluster of related abstractions to refactor together.",
            "inputSchema": {
                "type": "object",
                "properties": { "symbol": { "type": "string" } },
                "required": ["symbol"]
            }
        },
        {
            "name": "search",
            "description": "Fuzzy / glob search across indexed symbols. Supports `*` wildcards and substring/identifier matching. Filterable by kind, language, exported, and path prefix. Use this instead of running grep + read on file contents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Substring, identifier, or `glob*` pattern" },
                    "kinds": { "type": "array", "items": { "type": "string" } },
                    "languages": { "type": "array", "items": { "type": "string" } },
                    "exported": { "type": "boolean" },
                    "path_prefix": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "required": ["pattern"]
            }
        },
        {
            "name": "connect",
            "description": "Find the shortest call path from one symbol to another (does A transitively call B, and how?). Deterministic graph traversal — returns the ordered chain of calls or reports no path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Source symbol (the caller side)." },
                    "to": { "type": "string", "description": "Target symbol to reach." },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 12, "description": "Max hops to search. Default 8." }
                },
                "required": ["from", "to"]
            }
        },
        {
            "name": "export",
            "description": "Export the call graph as Graphviz DOT or Mermaid. Whole graph by default, or the forward call subgraph rooted at `from`. Useful for visualising structure or embedding a diagram.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["mermaid", "dot"] },
                    "from": { "type": "string", "description": "Root the export at this symbol's forward call subgraph." },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 12, "description": "Traversal depth when `from` is set. Default 3." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 5000, "description": "Max edges to emit. Default 800." }
                }
            }
        },
        {
            "name": "unused",
            "description": "Find indexed symbols with no inbound references or call edges. Test files are excluded from the report.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kinds": { "type": "array", "items": { "type": "string" }, "description": "Optional symbol kind filters such as function, method, class, struct." },
                    "languages": { "type": "array", "items": { "type": "string" }, "description": "Optional language filters such as typescript, rust, java." },
                    "exported": { "type": "boolean", "description": "When set, only include symbols whose exported flag matches this value." },
                    "path_prefix": { "type": "string", "description": "Only include symbols whose file path starts with this prefix." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "Maximum number of symbols to return. Default 50." }
                }
            }
        }
    ])
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
