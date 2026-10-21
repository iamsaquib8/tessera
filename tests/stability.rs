//! The 1.x contract (docs/stability.md): commands and flags, `--json`
//! fields, MCP tool schemas, hook replies, and exit codes. Each pinned file
//! under tests/stability/ may grow; an entry that disappears fails here.
//! After a deliberate addition, run with TESSERA_UPDATE_STABILITY=1 to
//! rewrite the pinned files.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;
use tessera_codegraph::agents::AGENTS;
use tessera_codegraph::hook::{render, silent, Agent, Event, Reply};
use tessera_codegraph::mcp::registry;

type Pins = BTreeMap<String, BTreeSet<String>>;

const UPDATE: &str = "TESSERA_UPDATE_STABILITY";

fn check(name: &str, current: Pins) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/stability")
        .join(format!("{name}.json"));
    if std::env::var_os(UPDATE).is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = serde_json::to_string_pretty(&current).unwrap();
        fs::write(&path, format!("{text}\n")).unwrap();
        return;
    }
    let text = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run with {UPDATE}=1 to create it",
            path.display()
        )
    });
    let pinned: Pins = serde_json::from_str(&text).unwrap();
    let mut gone = Vec::new();
    for (key, items) in &pinned {
        match current.get(key) {
            None => gone.push(key.clone()),
            Some(now) => gone.extend(items.difference(now).map(|item| format!("{key}: {item}"))),
        }
    }
    assert!(
        gone.is_empty(),
        "removed from the 1.x contract in {name}.json (see docs/stability.md):\n{}",
        gone.join("\n")
    );
}

/// Every object key reachable in `value`, as `a.b[].c`.
fn fields(value: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.insert(path.clone());
                fields(item, &path, out);
            }
        }
        Value::Array(items) => {
            let path = format!("{prefix}[]");
            for item in items {
                fields(item, &path, out);
            }
        }
        _ => {}
    }
}

fn git(root: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A fixture as a committed git repository.
fn repo(fixture: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(fixture), temp.path());
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-q", "-m", "base"]);
    temp
}

fn tessera(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("tessera").unwrap();
    // Other `tessera` binaries on PATH would show up in `status`.
    cmd.current_dir(root)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("TESSERA_DB")
        .arg("--quiet");
    cmd
}

fn json(root: &Path, args: &[&str]) -> Value {
    let output = tessera(root).arg("--json").args(args).output().unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "tessera {args:?}: {e}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn help(args: &[&str]) -> String {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .args(args)
        .arg("-h")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn commands_and_flags() {
    let top = help(&[]);
    let commands: Vec<String> = top
        .split("Commands:")
        .nth(1)
        .unwrap()
        .split("Options:")
        .next()
        .unwrap()
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(str::to_string)
        .collect();
    let mut pins = Pins::new();
    for command in &commands {
        let mut flags = BTreeSet::new();
        for line in help(&[command]).lines() {
            let line = line.trim_start();
            if !line.starts_with('-') {
                continue;
            }
            let names: Vec<&str> = line
                .split_whitespace()
                .take_while(|word| word.starts_with('-'))
                .map(|word| word.trim_end_matches(','))
                .collect();
            let Some(flag) = names.iter().find(|name| name.starts_with("--")) else {
                continue;
            };
            flags.extend(names.iter().map(|name| name.to_string()));
            if let Some(values) = line
                .split("[possible values: ")
                .nth(1)
                .and_then(|rest| rest.split(']').next())
            {
                for value in values.split(", ") {
                    flags.insert(format!("{flag}={value}"));
                }
            }
        }
        pins.insert(command.clone(), flags);
    }
    check("cli", pins);
}

#[test]
fn json_fields() {
    let temp = repo("typescript");
    let root = temp.path();
    let mut pins = Pins::new();
    let mut pin = |name: &str, value: Value| {
        let mut out = BTreeSet::new();
        fields(&value, "", &mut out);
        pins.insert(name.to_string(), out);
    };
    pin("index", json(root, &["index", "--full"]));
    pin("status", json(root, &["status"]));
    pin("find", json(root, &["find", "findById", "--body"]));
    pin("search", json(root, &["search", "User"]));
    pin("refs", json(root, &["refs", "UserRepo.findById"]));
    pin("blast", json(root, &["blast", "UserRepo.findById"]));
    pin("context", json(root, &["context", "UserRepo.findById"]));
    pin(
        "path",
        json(root, &["path", "handler", "UserRepo.findById"]),
    );
    pin("export", json(root, &["export"]));
    let file = root.join("src/users/repo.ts");
    let source = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        source.replacen("findById(id: string)", "findOne(id: string)", 1),
    )
    .unwrap();
    pin("blast --diff", json(root, &["blast", "--diff", "HEAD"]));
    let verify = json(root, &["verify", "--changed"]);
    assert!(
        !verify["findings"].as_array().unwrap().is_empty(),
        "the rename should leave a broken call"
    );
    pin("verify", verify);

    let http = repo("contracts-http");
    pin("map", json(http.path(), &["map"]));
    pin("map service", json(http.path(), &["map", "service"]));
    check("json", pins);
}

#[test]
fn mcp_tool_schemas() {
    let mut pins = Pins::new();
    for tool in registry() {
        let schema = (tool.input_schema)();
        let properties = schema["properties"]
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        pins.insert(tool.name.to_string(), properties);
        // A new required argument breaks callers, so the list is pinned whole.
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        pins.insert(
            format!("{} required", tool.name),
            BTreeSet::from([required.join(",")]),
        );
    }
    check("mcp", pins);
}

#[test]
fn hook_replies() {
    let mut pins = Pins::new();
    for config in AGENTS.iter().filter_map(|agent| agent.hooks.as_ref()) {
        for event in config.events {
            let key = format!("{} {}", config.agent, event.event);
            let agent = Agent::parse(config.agent).unwrap();
            let kind = Event::parse(event.event).unwrap();
            let replies = [
                (
                    "context",
                    render(agent, kind, event.event, Reply::Context("T".into())),
                ),
                (
                    "block",
                    render(agent, kind, event.event, Reply::Block("T".into())),
                ),
                ("silent", silent(agent, kind)),
            ];
            pins.insert(
                key,
                replies
                    .into_iter()
                    .map(|(name, reply)| {
                        format!("{name} {}", reply.map_or("-".into(), |v| v.to_string()))
                    })
                    .collect(),
            );
        }
    }
    check("hooks", pins);
}

#[test]
fn exit_codes() {
    let temp = repo("typescript");
    let root = temp.path();
    let code = |args: &[&str]| tessera(root).args(args).output().unwrap().status.code();
    assert_eq!(code(&["status"]), Some(0));
    assert_eq!(code(&["find", "--no-such-flag"]), Some(2));
    let file = root.join("src/users/repo.ts");
    let source = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        source.replacen("findById(id: string)", "findOne(id: string)", 1),
    )
    .unwrap();
    assert_eq!(code(&["verify", "--changed"]), Some(1));
    let blocked: PathBuf = root.join("not-a-dir");
    fs::write(&blocked, "").unwrap();
    let db = blocked.join("tessera.db");
    assert_eq!(code(&["--db", db.to_str().unwrap(), "find", "x"]), Some(3));
}
