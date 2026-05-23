use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::query;

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
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => handle_request(db_path, request),
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

fn handle_request(db_path: &Path, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "tessera", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "tools": {} }
        })),
        "notifications/initialized" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(db_path, &request.params),
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

fn call_tool(db_path: &Path, params: &Value) -> std::result::Result<Value, String> {
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
            serde_json::to_value(query::find_definition(db_path, &symbol).map_err(to_string)?)
        }
        "find_references" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::find_references(db_path, &symbol).map_err(to_string)?)
        }
        "get_outline" => {
            let path = arg_string(&args, "path")?;
            serde_json::to_value(query::get_outline(db_path, Path::new(&path)).map_err(to_string)?)
        }
        "expand_symbol" => {
            let symbol = arg_string(&args, "symbol")?;
            serde_json::to_value(query::expand_symbol(db_path, &symbol).map_err(to_string)?)
        }
        "impact" => {
            let symbol = arg_string(&args, "symbol")?;
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(4) as usize;
            serde_json::to_value(query::impact(db_path, &symbol, depth).map_err(to_string)?)
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
            "description": "Return transitive callers ranked by criticality.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 10 }
                },
                "required": ["symbol"]
            }
        }
    ])
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
