use std::io::Read;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use super::protocol::{
    error_response, result_response, RpcError, HEADER_MISMATCH, INVALID_REQUEST, METHOD_NOT_FOUND,
    PROTOCOL_VERSIONS, UNSUPPORTED_VERSION,
};
use super::{dispatch, parse, requested_version, ServerState};
use crate::db;
use crate::fresh;
use crate::workspace::Workspace;

const WORKERS: usize = 4;
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;

pub fn serve_http(ws: Workspace, addr: &str) -> Result<()> {
    let server = Arc::new(Server::http(addr).map_err(|e| anyhow!("cannot bind {addr}: {e}"))?);
    eprintln!("Tessera MCP listening on http://{addr}/mcp (health: /health)");
    let state = Arc::new(ServerState::new(ws));
    let workers: Vec<_> = (0..WORKERS)
        .map(|_| {
            let server = Arc::clone(&server);
            let state = Arc::clone(&state);
            thread::spawn(move || {
                for request in server.incoming_requests() {
                    let _ = respond(&state, request);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn respond(state: &ServerState, mut request: Request) -> std::io::Result<()> {
    if let Some(origin) = header(&request, "Origin") {
        if !allowed_origin(&origin) {
            let body = error_response(
                Value::Null,
                &RpcError::new(INVALID_REQUEST, format!("Origin {origin} is not allowed")),
            );
            return request.respond(json_response(403, &body));
        }
    }
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    match (request.method(), path.as_str()) {
        (Method::Get, "/health") => {
            let body = health(&state.ws);
            request.respond(json_response(200, &body))
        }
        (Method::Post, "/mcp") => {
            let (status, body) = post(state, &mut request);
            match body {
                Some(body) => request.respond(json_response(status, &body)),
                None => request.respond(Response::empty(status)),
            }
        }
        (_, "/mcp") => request.respond(
            Response::from_string("Use POST for MCP requests")
                .with_status_code(405)
                .with_header(text_header("Allow", "POST")),
        ),
        _ => request.respond(Response::from_string("Not found").with_status_code(404)),
    }
}

fn post(state: &ServerState, request: &mut Request) -> (u16, Option<Value>) {
    let mut raw = String::new();
    if request
        .as_reader()
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut raw)
        .is_err()
    {
        let error = RpcError::new(INVALID_REQUEST, "Request body must be UTF-8 JSON");
        return (400, Some(error_response(Value::Null, &error)));
    }
    let parsed = match parse(&raw) {
        Ok(parsed) => parsed,
        Err(error) => return (400, Some(error_response(Value::Null, &error))),
    };
    let id = parsed.id.clone().unwrap_or(Value::Null);
    let header_version = header(request, "MCP-Protocol-Version");
    let body_version = requested_version(&parsed.params).map(ToOwned::to_owned);
    let reject = |code: i64, message: String| {
        (
            400,
            Some(error_response(id.clone(), &RpcError::new(code, message))),
        )
    };
    if let Some(version) = &body_version {
        if header_version.as_deref() != Some(version.as_str()) {
            return reject(
                HEADER_MISMATCH,
                format!(
                    "Header mismatch: MCP-Protocol-Version {:?} does not match body version {version}",
                    header_version
                ),
            );
        }
        if header(request, "Mcp-Method").as_deref() != parsed.method.as_deref() {
            return reject(
                HEADER_MISMATCH,
                "Header mismatch: Mcp-Method must equal the request method".to_string(),
            );
        }
        if parsed.method.as_deref() == Some("tools/call") {
            let name = parsed.params.get("name").and_then(Value::as_str);
            if header(request, "Mcp-Name").as_deref() != name {
                return reject(
                    HEADER_MISMATCH,
                    "Header mismatch: Mcp-Name must equal params.name".to_string(),
                );
            }
        }
    } else if let Some(version) = &header_version {
        if !PROTOCOL_VERSIONS.contains(&version.as_str()) {
            return (
                400,
                Some(error_response(id, &RpcError::unsupported_version(version))),
            );
        }
    }
    if parsed.id.is_none() {
        return (202, None);
    }
    let Some(method) = parsed.method else {
        return reject(INVALID_REQUEST, "Request is missing `method`".to_string());
    };
    match dispatch(state, &method, &parsed.params) {
        Ok(result) => (200, Some(result_response(id, result))),
        Err(error) if error.code == UNSUPPORTED_VERSION => (400, Some(error_response(id, &error))),
        Err(error) if error.code == METHOD_NOT_FOUND && body_version.is_some() => {
            (404, Some(error_response(id, &error)))
        }
        Err(error) => (200, Some(error_response(id, &error))),
    }
}

fn health(ws: &Workspace) -> Value {
    let freshness = fresh::check(ws).ok();
    let schema = db::open_ro(&ws.db_path)
        .ok()
        .and_then(|conn| db::schema_version(&conn).ok());
    json!({
        "ok": schema == Some(db::SCHEMA_VERSION),
        "service": "tessera-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "root": ws.root.display().to_string(),
        "db_path": ws.db_path.display().to_string(),
        "schema_version": schema,
        "freshness": freshness,
        "protocol_versions": PROTOCOL_VERSIONS,
        "endpoints": { "mcp": "/mcp", "health": "/health" }
    })
}

fn allowed_origin(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = if let Some(stripped) = rest.strip_prefix('[') {
        stripped.split(']').next().unwrap_or_default()
    } else {
        rest.split(':').next().unwrap_or_default()
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn text_header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

fn json_response(status: u16, body: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(text_header("Content-Type", "application/json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_rules() {
        for ok in [
            "http://localhost",
            "http://localhost:3000",
            "https://127.0.0.1:8765",
            "http://[::1]:9000",
        ] {
            assert!(allowed_origin(ok), "{ok}");
        }
        for bad in [
            "https://evil.example",
            "http://localhost.evil.example",
            "null",
            "file://",
        ] {
            assert!(!allowed_origin(bad), "{bad}");
        }
    }
}
