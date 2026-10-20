mod support;

use serde_json::{json, Value};
use support::{index_fixture, synth_repo};
use tempfile::TempDir;
use tessera_codegraph::mcp::{handle, Outcome, ServerState};
use tessera_codegraph::{Index, IndexOptions, Workspace};

const MODERN: &str = "2026-07-28";

fn state_for(lang: &str) -> (TempDir, ServerState) {
    let (temp, db) = index_fixture(lang);
    let index = Index::open(&db).unwrap();
    let ws = Workspace {
        root: index.root().to_path_buf(),
        db_path: db,
    };
    (temp, ServerState::new(ws))
}

fn reply(state: &ServerState, message: Value) -> Value {
    match handle(state, &message.to_string()) {
        Outcome::Reply(value) => value,
        Outcome::Silent => panic!("expected a reply to {message}"),
    }
}

fn modern(id: u64, method: &str, params: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": MODERN,
        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "0" },
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn call(state: &ServerState, name: &str, arguments: Value) -> Value {
    reply(
        state,
        modern(
            9,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        ),
    )
}

#[test]
fn notifications_get_no_reply() {
    let (_temp, state) = state_for("typescript");
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(matches!(handle(&state, &note.to_string()), Outcome::Silent));
}

#[test]
fn ping_returns_empty_result() {
    let (_temp, state) = state_for("typescript");
    let legacy = reply(
        &state,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
    );
    assert_eq!(legacy["result"], json!({}));
}

#[test]
fn error_codes_follow_json_rpc() {
    let (_temp, state) = state_for("typescript");
    let parse = match handle(&state, "{not json") {
        Outcome::Reply(v) => v,
        Outcome::Silent => panic!(),
    };
    assert_eq!(parse["error"]["code"], -32700);
    assert!(parse["id"].is_null());
    let batch = match handle(&state, "[]") {
        Outcome::Reply(v) => v,
        Outcome::Silent => panic!(),
    };
    assert_eq!(batch["error"]["code"], -32600);
    let unknown = reply(&state, modern(1, "nope/nope", json!({})));
    assert_eq!(unknown["error"]["code"], -32601);
    let tool = reply(
        &state,
        modern(
            2,
            "tools/call",
            json!({ "name": "no_such_tool", "arguments": {} }),
        ),
    );
    assert_eq!(tool["error"]["code"], -32602);
    let missing = call(&state, "refs", json!({}));
    assert_eq!(missing["error"]["code"], -32602);
}

#[test]
fn legacy_initialize_negotiates_a_legacy_version() {
    let (_temp, state) = state_for("typescript");
    let init = |version: &str| {
        reply(
            &state,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "c", "version": "0" } }
            }),
        )
    };
    assert_eq!(
        init("2024-11-05")["result"]["protocolVersion"],
        "2024-11-05"
    );
    assert_eq!(
        init("1999-01-01")["result"]["protocolVersion"],
        "2025-11-25"
    );
    assert_eq!(
        init("2024-11-05")["result"]["serverInfo"]["name"],
        "tessera"
    );
}

#[test]
fn modern_discover_and_version_errors() {
    let (_temp, state) = state_for("typescript");
    let discover = reply(&state, modern(1, "server/discover", json!({})));
    assert_eq!(discover["result"]["resultType"], "complete");
    let versions = discover["result"]["supportedVersions"].as_array().unwrap();
    assert_eq!(versions[0], MODERN);
    assert!(versions.contains(&json!("2024-11-05")));
    let mut bad = modern(2, "tools/list", json!({}));
    bad["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    let error = reply(&state, bad);
    assert_eq!(error["error"]["code"], -32022);
    assert_eq!(error["error"]["data"]["requested"], "1900-01-01");
    assert_eq!(error["error"]["data"]["supported"][0], MODERN);
}

#[test]
fn tools_list_has_eight_read_only_tools() {
    let (_temp, state) = state_for("typescript");
    let listed = reply(&state, modern(1, "tools/list", json!({})));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["find", "search", "refs", "blast", "verify", "context", "path", "status"]
    );
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{tool}");
        assert!(!tool["title"].as_str().unwrap().is_empty());
    }
}

#[test]
fn tool_results_are_text_with_execution_errors_flagged() {
    let (_temp, state) = state_for("typescript");
    let found = call(&state, "find", json!({ "symbol": "findById" }));
    assert_eq!(found["result"]["resultType"], "complete");
    assert_eq!(found["result"]["isError"], false);
    let text = found["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("UserRepo.findById"), "{text}");
    let as_json = call(
        &state,
        "find",
        json!({ "symbol": "findById", "format": "json" }),
    );
    let json_text = as_json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(serde_json::from_str::<Value>(json_text).is_ok());
    let failed = call(&state, "blast", json!({}));
    assert_eq!(failed["result"]["isError"], true, "{failed}");
}

#[test]
fn verify_accepts_plain_language_names() {
    let (_temp, state) = state_for("javascript");
    let result = call(
        &state,
        "verify",
        json!({ "snippet": "slugifyy('x')", "language": "javascript" }),
    );
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("slugifyy"), "{text}");
}

#[test]
fn large_results_stay_within_budget_and_paginate() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    synth_repo(&root, 300);
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();
    let state = ServerState::new(Workspace {
        root: root.canonicalize().unwrap(),
        db_path: db,
    });
    let result = call(&state, "refs", json!({ "target": "sharedHelper" }));
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.len() <= 8_000, "{} chars", text.len());
    assert!(text.contains("cursor o:"), "{text}");
}
