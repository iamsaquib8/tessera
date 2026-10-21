//! `tessera ui`: one embedded page and a JSON API on 127.0.0.1, or a static
//! export of both for a hosted demo. Nothing is loaded from the network.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use rusqlite::params;
use serde_json::{json, Value};

use crate::db;
use crate::fresh;
use crate::query::{BlastArgs, Changes, FindArgs, MapArgs, PathArgs, RefsArgs, SearchArgs};
use crate::workspace::Workspace;
use crate::Index;

const PAGE: &str = include_str!("page.html");
/// Symbols with a page of their own in an export: the most referenced.
const EXPORT_SYMBOLS: usize = 300;
const EXPORT_SEARCH: usize = 5_000;

/// Serves the UI until the process is stopped.
pub fn serve(ws: &Workspace, port: u16, open: bool) -> Result<()> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not listen on 127.0.0.1:{port}: {e}"))?;
    let port = server
        .server_addr()
        .to_ip()
        .map_or(port, |addr| addr.port());
    let url = format!("http://127.0.0.1:{port}/");
    eprintln!("Tessera UI on {url} (Ctrl-C to stop)");
    if open {
        open_browser(&url);
    }
    for request in server.incoming_requests() {
        let host = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .map(|h| h.value.as_str().to_string());
        let (status, kind, body) = respond(ws, request.url(), host.as_deref());
        let mut response = tiny_http::Response::from_data(body).with_status_code(status);
        for (field, value) in [
            ("Content-Type", kind),
            ("Cache-Control", "no-store"),
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
        ] {
            if let Ok(header) = tiny_http::Header::from_bytes(field.as_bytes(), value.as_bytes()) {
                response.add_header(header);
            }
        }
        let _ = request.respond(response);
    }
    Ok(())
}

fn open_browser(url: &str) {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Only the loopback names: a page on another host that points its name at
/// 127.0.0.1 (DNS rebinding) gets nothing.
fn local_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return true;
    };
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// One request: `(status, content type, body)`.
pub fn respond(ws: &Workspace, url: &str, host: Option<&str>) -> (u16, &'static str, Vec<u8>) {
    if !local_host(host) {
        return (403, "text/plain", b"forbidden".to_vec());
    }
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let params: HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect();
    match path {
        "/" | "/index.html" => (200, "text/html; charset=utf-8", PAGE.as_bytes().to_vec()),
        _ => {
            let Some(route) = path.strip_prefix("/api/") else {
                return (404, "text/plain", b"not found".to_vec());
            };
            match api(ws, route, &params) {
                Ok(Some(value)) => (200, "application/json", value.to_string().into_bytes()),
                Ok(None) => (
                    404,
                    "application/json",
                    br#"{"error":"unknown endpoint"}"#.to_vec(),
                ),
                Err(error) => (
                    200,
                    "application/json",
                    json!({ "error": format!("{error:#}") })
                        .to_string()
                        .into_bytes(),
                ),
            }
        }
    }
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn api(ws: &Workspace, route: &str, params: &HashMap<String, String>) -> Result<Option<Value>> {
    if !matches!(
        route,
        "status" | "impact" | "map" | "services" | "symbol" | "search" | "path"
    ) {
        return Ok(None);
    }
    // A busy writer still leaves a readable index.
    let _ = fresh::refresh(ws);
    let index = Index::open_workspace(ws)?;
    let param = |name: &str| params.get(name).filter(|v| !v.is_empty()).cloned();
    let value = match route {
        "status" => serde_json::to_value(index.status()?)?,
        "impact" => {
            let changes = param("diff").map_or(Changes::WorkingTree, Changes::Since);
            serde_json::to_value(index.report(&changes)?)?
        }
        "map" => serde_json::to_value(index.map(&MapArgs {
            kind: param("kind"),
            query: param("q"),
        })?)?,
        "services" => serde_json::to_value(index.map(&MapArgs {
            kind: Some("service".to_string()),
            query: None,
        })?)?,
        "symbol" => symbol(
            &index,
            &param("name").ok_or_else(|| anyhow!("pass a symbol name"))?,
        )?,
        "search" => serde_json::to_value(index.search(&SearchArgs {
            pattern: param("q").unwrap_or_default(),
            limit: 50,
            ..SearchArgs::default()
        })?)?,
        _ => serde_json::to_value(index.path(&PathArgs {
            from: param("from").unwrap_or_default(),
            to: param("to").unwrap_or_default(),
            depth: 0,
        })?)?,
    };
    Ok(Some(value))
}

/// Definition, references, callers by impact, and tests, for one symbol.
fn symbol(index: &Index, name: &str) -> Result<Value> {
    let find = index.find(&FindArgs {
        symbol: Some(name.to_string()),
        include_body: true,
        limit: 5,
        ..FindArgs::default()
    })?;
    let refs = index.refs(&RefsArgs {
        target: name.to_string(),
        limit: 100,
        ..RefsArgs::default()
    })?;
    let blast = index
        .blast(&BlastArgs {
            symbol: Some(name.to_string()),
            limit: 20,
            ..BlastArgs::default()
        })
        .ok();
    Ok(json!({ "find": find, "refs": refs, "blast": blast }))
}

/// The file name of a symbol's page in an export; the page computes the
/// same FNV-1a hash.
pub fn symbol_key(name: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// Writes the page and every answer it needs as a static site under `dir`.
pub fn export(ws: &Workspace, dir: &Path) -> Result<()> {
    fresh::ensure(ws)?;
    let index = Index::open_workspace(ws)?;
    let data = dir.join("data");
    fs::create_dir_all(data.join("symbols"))
        .with_context(|| format!("failed to create {}", data.display()))?;
    let page = PAGE.replacen(
        "<script>\n\"use strict\";",
        "<script>window.TESSERA_STATIC = true;</script>\n<script>\n\"use strict\";",
        1,
    );
    fs::write(dir.join("index.html"), page)?;
    let write = |name: &str, value: Value| -> Result<()> {
        fs::write(data.join(format!("{name}.json")), value.to_string())
            .with_context(|| format!("failed to write {name}.json"))
    };
    write("status", serde_json::to_value(index.status()?)?)?;
    let impact = index
        .report(&Changes::WorkingTree)
        .map(|r| serde_json::to_value(r).unwrap_or_default())
        .unwrap_or_else(|e| json!({ "error": format!("{e:#}") }));
    write("impact", impact)?;
    let map = index.map(&MapArgs::default())?;
    write("map", serde_json::to_value(&map)?)?;
    write(
        "services",
        serde_json::to_value(index.map(&MapArgs {
            kind: Some("service".to_string()),
            query: None,
        })?)?,
    )?;
    let busy = busiest_symbols(index.connection(), EXPORT_SEARCH)?;
    let hits: Vec<Value> = busy
        .iter()
        .map(|symbol| json!({ "symbol": symbol }))
        .collect();
    write("search", json!({ "hits": hits }))?;
    let mut pages: BTreeSet<String> = busy
        .iter()
        .take(EXPORT_SYMBOLS)
        .map(|s| s.qualified_name.clone())
        .collect();
    for contract in &map.contracts {
        for site in contract.providers.iter().chain(&contract.consumers) {
            pages.extend(site.symbol.clone());
        }
    }
    for name in pages {
        let value = symbol(&index, &name)?;
        fs::write(
            data.join("symbols")
                .join(format!("{}.json", symbol_key(&name))),
            value.to_string(),
        )?;
    }
    Ok(())
}

/// Symbols by how often they are referenced, most first.
fn busiest_symbols(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<crate::types::SymbolRecord>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.qualified_name, s.kind, s.file_id, f.path, f.language,
                s.start_line, s.end_line, s.signature, s.exported
         FROM symbols s JOIN files f ON f.id = s.file_id
         LEFT JOIN (SELECT to_symbol_id, COUNT(*) AS n FROM edges GROUP BY to_symbol_id) c
           ON c.to_symbol_id = s.id
         ORDER BY COALESCE(c.n, 0) DESC, s.qualified_name
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], db::map_symbol)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_decoding() {
        assert!(local_host(Some("127.0.0.1:4747")));
        assert!(local_host(Some("localhost")));
        assert!(local_host(Some("[::1]:4747")));
        assert!(!local_host(Some("evil.example:4747")));
        assert_eq!(decode("a%2Fb+c%3F"), "a/b c?");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(symbol_key("CartService.GetCart").len(), 8);
    }
}
