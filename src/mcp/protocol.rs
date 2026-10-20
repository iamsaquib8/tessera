use serde_json::{json, Value};

pub const MODERN_VERSION: &str = "2026-07-28";
pub const PROTOCOL_VERSIONS: &[&str] = &[
    MODERN_VERSION,
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const META_VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const HEADER_MISMATCH: i64 = -32020;
pub const UNSUPPORTED_VERSION: i64 = -32022;

#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn unsupported_version(requested: &str) -> Self {
        Self {
            code: UNSUPPORTED_VERSION,
            message: "Unsupported protocol version".to_string(),
            data: Some(json!({ "supported": PROTOCOL_VERSIONS, "requested": requested })),
        }
    }
}

pub fn negotiate(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|r| LEGACY_VERSIONS.iter().find(|v| **v == r))
        .copied()
        .unwrap_or(LEGACY_VERSIONS[0])
}

pub fn error_response(id: Value, error: &RpcError) -> Value {
    let mut body = json!({ "code": error.code, "message": error.message });
    if let Some(data) = &error.data {
        body["data"] = data.clone();
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": body })
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
