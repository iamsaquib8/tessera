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

mod live {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::mpsc::{channel, Receiver};
    use std::thread;
    use std::time::{Duration, Instant};

    use serde_json::{json, Value};
    use tempfile::TempDir;

    use super::support::{copy_dir, fixture_dir};

    struct Server {
        child: Child,
        stdin: ChildStdin,
        lines: Receiver<String>,
        next_id: u64,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Server {
        fn start(root: &std::path::Path) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_tessera"))
                .args(["--quiet", "mcp"])
                .current_dir(root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let (tx, lines) = channel();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            Self {
                child,
                stdin,
                lines,
                next_id: 1,
            }
        }

        fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
            let id = self.next_id;
            self.next_id += 1;
            let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
            writeln!(self.stdin, "{message}").unwrap();
            self.stdin.flush().unwrap();
            let line = self
                .lines
                .recv_timeout(timeout)
                .unwrap_or_else(|_| panic!("no reply to {method} within {timeout:?}"));
            serde_json::from_str(&line).unwrap()
        }

        fn tool_text(&mut self, name: &str, arguments: Value) -> (bool, String) {
            let reply = self.request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
                Duration::from_secs(10),
            );
            let result = &reply["result"];
            (
                result["isError"].as_bool().unwrap_or(true),
                result["content"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        }
    }

    fn repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        copy_dir(&fixture_dir("typescript"), temp.path());
        temp
    }

    #[test]
    fn stdio_answers_initialize_quickly_without_an_index() {
        let temp = repo();
        let mut server = Server::start(temp.path());
        let started = Instant::now();
        let reply = server.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }),
            Duration::from_secs(1),
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn stdio_reflects_edits_within_two_seconds() {
        let temp = repo();
        let mut server = Server::start(temp.path());
        let ready = Instant::now();
        loop {
            let (error, _) = server.tool_text("status", json!({}));
            if !error {
                break;
            }
            assert!(
                ready.elapsed() < Duration::from_secs(10),
                "index never became ready"
            );
            thread::sleep(Duration::from_millis(50));
        }
        std::fs::write(
            temp.path().join("src/fresh_module.ts"),
            "export function brandNewFunction() { return 1; }\n",
        )
        .unwrap();
        let edited = Instant::now();
        loop {
            let (_, text) = server.tool_text("find", json!({ "symbol": "brandNewFunction" }));
            if text.contains("src/fresh_module.ts") && !text.contains("closest matches") {
                break;
            }
            assert!(
                edited.elapsed() < Duration::from_secs(2),
                "edit not visible after 2s: {text}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
}
