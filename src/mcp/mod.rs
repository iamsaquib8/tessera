mod http;
mod protocol;
mod stdio;
mod tools;

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

pub use http::serve_http;
pub use protocol::{negotiate, LEGACY_VERSIONS, MODERN_VERSION, PROTOCOL_VERSIONS};
pub use stdio::serve_stdio;
pub use tools::{registry, ToolError, ToolSpec};

use crate::workspace::Workspace;
use protocol::{
    error_response, result_response, RpcError, INVALID_REQUEST, META_VERSION_KEY, METHOD_NOT_FOUND,
    PARSE_ERROR,
};

const INSTRUCTIONS: &str = "Tessera answers code-navigation questions from a local, deterministic graph of this repository. Before editing a symbol, call `blast` (who is affected) or `context` (body, callers, tests). After writing code that calls project APIs, call `verify` with the snippet. Use `refs` instead of grep for call sites and `find` instead of opening files to locate definitions. Results are paginated; pass `cursor` to continue.";

pub struct ServerState {
    pub ws: Workspace,
    building: Arc<Mutex<Option<String>>>,
}

impl ServerState {
    pub fn new(ws: Workspace) -> Self {
        Self {
            ws,
            building: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_building(&self, message: Option<String>) {
        *self.building.lock().unwrap_or_else(|e| e.into_inner()) = message;
    }

    pub fn building(&self) -> Option<String> {
        self.building
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn building_handle(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.building)
    }
}

#[derive(Debug)]
pub enum Outcome {
    Reply(Value),
    Silent,
}

pub(crate) struct Parsed {
    pub id: Option<Value>,
    pub method: Option<String>,
    pub params: Value,
}

pub(crate) fn parse(raw: &str) -> Result<Parsed, RpcError> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| RpcError::new(PARSE_ERROR, format!("Parse error: {e}")))?;
    let Some(object) = value.as_object() else {
        return Err(RpcError::new(
            INVALID_REQUEST,
            "Expected a single JSON-RPC request object (batches are not supported)",
        ));
    };
    Ok(Parsed {
        id: object.get("id").cloned(),
        method: object
            .get("method")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        params: object.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

pub(crate) fn requested_version(params: &Value) -> Option<&str> {
    params
        .get("_meta")
        .and_then(|meta| meta.get(META_VERSION_KEY))
        .and_then(Value::as_str)
}

pub fn handle(state: &ServerState, raw: &str) -> Outcome {
    let parsed = match parse(raw) {
        Ok(parsed) => parsed,
        Err(error) => return Outcome::Reply(error_response(Value::Null, &error)),
    };
    let Some(id) = parsed.id else {
        return Outcome::Silent;
    };
    let Some(method) = parsed.method else {
        return Outcome::Reply(error_response(
            id,
            &RpcError::new(INVALID_REQUEST, "Request is missing `method`"),
        ));
    };
    Outcome::Reply(match dispatch(state, &method, &parsed.params) {
        Ok(result) => result_response(id, result),
        Err(error) => error_response(id, &error),
    })
}

pub(crate) fn dispatch(
    state: &ServerState,
    method: &str,
    params: &Value,
) -> Result<Value, RpcError> {
    let version = requested_version(params);
    if let Some(version) = version {
        if !PROTOCOL_VERSIONS.contains(&version) {
            return Err(RpcError::unsupported_version(version));
        }
    }
    let modern = version == Some(MODERN_VERSION);
    let mut result = match method {
        "initialize" => initialize(params),
        "server/discover" => discover(),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools::list() }),
        "tools/call" => tools::call(state, params)?,
        _ => {
            return Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("Method not found: {method}"),
            ))
        }
    };
    if modern {
        result["resultType"] = json!("complete");
    }
    Ok(result)
}

fn server_info() -> Value {
    json!({ "name": "tessera", "title": "Tessera", "version": env!("CARGO_PKG_VERSION") })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    json!({
        "protocolVersion": negotiate(requested),
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

fn discover() -> Value {
    json!({
        "resultType": "complete",
        "supportedVersions": PROTOCOL_VERSIONS,
        "capabilities": { "tools": { "listChanged": false } },
        "_meta": { "io.modelcontextprotocol/serverInfo": server_info() },
        "instructions": INSTRUCTIONS,
    })
}
