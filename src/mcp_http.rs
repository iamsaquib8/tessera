use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use anyhow::Result;
use serde_json::json;

use crate::db;
use crate::mcp;

pub fn serve(addr: &str, db_path: &Path) -> Result<()> {
    let conn = db::open_existing(db_path)?;
    let listener = TcpListener::bind(addr)?;
    println!("Tessera MCP HTTP listening on http://{addr}");
    println!("  POST /mcp     JSON-RPC MCP endpoint");
    println!("  GET  /sse     readiness event stream");
    println!("  GET  /health  health check");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_stream(&mut stream, &conn, db_path) {
                    let _ = write_response(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain; charset=utf-8",
                        &error.to_string(),
                    );
                }
            }
            Err(error) => eprintln!("mcp-http accept error: {error}"),
        }
    }
    Ok(())
}

fn handle_stream(
    stream: &mut TcpStream,
    conn: &rusqlite::Connection,
    db_path: &Path,
) -> Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if request_complete(&buffer) {
            break;
        }
        if buffer.len() > 2 * 1024 * 1024 {
            write_response(
                stream,
                "413 Payload Too Large",
                "text/plain; charset=utf-8",
                "request too large",
            )?;
            return Ok(());
        }
    }

    let request = String::from_utf8_lossy(&buffer);
    let Some((head, body)) = request.split_once("\r\n\r\n") else {
        write_response(
            stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "malformed HTTP request",
        )?;
        return Ok(());
    };
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    match (method, path) {
        ("GET", "/health") => {
            let json = serde_json::to_string(&health_payload(conn, db_path)?)?;
            write_response(stream, "200 OK", "application/json", &json)?;
        }
        ("GET", "/sse") => {
            let ready = json!({
                "endpoint": "/mcp",
                "service": "tessera-mcp-http",
                "version": env!("CARGO_PKG_VERSION")
            });
            let body = format!("event: ready\ndata: {ready}\n\n");
            write_raw(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                &body,
            )?;
        }
        ("POST", "/mcp") => {
            let response = mcp::handle_json_rpc(conn, db_path, body);
            let json = serde_json::to_string(&response)?;
            write_response(stream, "200 OK", "application/json", &json)?;
        }
        _ => write_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found",
        )?,
    }
    Ok(())
}

fn health_payload(conn: &rusqlite::Connection, db_path: &Path) -> Result<serde_json::Value> {
    let schema_version = db::get_meta(conn, "schema_version")?;
    let root = db::get_meta(conn, "root")?;
    let snapshot_path = snapshot_path_for(db_path);
    let expected_schema_version = db::SCHEMA_VERSION.to_string();
    Ok(json!({
        "ok": schema_version.as_deref() == Some(expected_schema_version.as_str()),
        "service": "tessera-mcp-http",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "mcp-json-rpc-over-http",
        "endpoints": {
            "mcp": "/mcp",
            "sse": "/sse",
            "health": "/health"
        },
        "db_path": db_path.display().to_string(),
        "root": root,
        "schema_version": schema_version,
        "expected_schema_version": db::SCHEMA_VERSION,
        "snapshot_path": snapshot_path.display().to_string(),
        "snapshot_exists": snapshot_path.exists()
    }))
}

fn snapshot_path_for(db_path: &Path) -> std::path::PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("snapshot.bin")
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    });
    match content_length {
        Some(length) => buffer.len() >= header_end + 4 + length,
        None => true,
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_raw(stream, &head, body)
}

fn write_raw(stream: &mut TcpStream, head: &str, body: &str) -> Result<()> {
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()?;
    Ok(())
}
