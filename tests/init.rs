mod support;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

/// A project and an empty home directory, so detection sees only what a test
/// puts there.
struct Setup {
    _temp: TempDir,
    root: std::path::PathBuf,
    home: std::path::PathBuf,
}

fn setup() -> Setup {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    copy_dir(&fixture_dir("typescript"), &root);
    git(&root, &["init", "-q"]);
    Setup {
        _temp: temp,
        root,
        home,
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// `tessera` on PATH, and the system tools git needs.
fn path_with_tessera() -> String {
    let bin = assert_cmd::cargo::cargo_bin("tessera");
    format!("{}:/usr/bin:/bin", bin.parent().unwrap().display())
}

fn init(s: &Setup, args: &[&str]) -> (String, bool) {
    init_with_path(s, args, &path_with_tessera())
}

fn init_with_path(s: &Setup, args: &[&str], path: &str) -> (String, bool) {
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .current_dir(&s.root)
        .env("HOME", &s.home)
        .env("USERPROFILE", &s.home)
        .env("PATH", path)
        .env_remove("TESSERA_DB")
        .arg("init")
        .args(args)
        .output()
        .unwrap();
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.success(),
    )
}

fn touch(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn dry_run_shows_a_diff_for_detected_agents_and_writes_nothing() {
    let s = setup();
    touch(&s.root, "CLAUDE.md", "# Notes\n");
    fs::create_dir_all(s.root.join(".cursor")).unwrap();
    let (out, ok) = init(&s, &["--dry-run"]);
    assert!(ok, "{out}");
    assert!(out.contains("+++ .mcp.json"), "{out}");
    assert!(out.contains("+++ .cursor/mcp.json"), "{out}");
    assert!(out.contains("Claude Code"), "{out}");
    assert!(!s.root.join(".mcp.json").exists());
    assert!(!s.root.join(".cursor/mcp.json").exists());
}

#[test]
fn without_a_terminal_nothing_is_written_until_yes() {
    let s = setup();
    fs::create_dir_all(s.root.join(".claude")).unwrap();
    let (out, ok) = init(&s, &[]);
    assert!(ok, "{out}");
    assert!(out.contains("--yes"), "{out}");
    assert!(!s.root.join(".mcp.json").exists());
}

#[test]
fn yes_writes_every_format_family() {
    let s = setup();
    fs::create_dir_all(s.root.join(".claude")).unwrap();
    fs::create_dir_all(s.root.join(".vscode")).unwrap();
    fs::create_dir_all(s.root.join(".continue")).unwrap();
    touch(
        &s.root,
        ".codex/config.toml",
        "model = \"o4\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
    );
    touch(
        &s.root,
        "opencode.json",
        "{\n  \"theme\": \"dark\",\n  \"agent\": {}\n}\n",
    );
    let (out, ok) = init(&s, &["--yes"]);
    assert!(ok, "{out}");

    let claude = json(&s.root.join(".mcp.json"));
    assert_eq!(claude["mcpServers"]["tessera"]["command"], "tessera");
    assert_eq!(claude["mcpServers"]["tessera"]["args"][0], "mcp");

    let vscode = json(&s.root.join(".vscode/mcp.json"));
    assert_eq!(vscode["servers"]["tessera"]["type"], "stdio");

    let codex = fs::read_to_string(s.root.join(".codex/config.toml")).unwrap();
    assert!(codex.starts_with("model = \"o4\""), "{codex}");
    assert!(codex.contains("[mcp_servers.other]"), "{codex}");
    assert!(codex.contains("[mcp_servers.tessera]"), "{codex}");

    let opencode = fs::read_to_string(s.root.join("opencode.json")).unwrap();
    assert!(
        opencode.find("\"theme\"").unwrap() < opencode.find("\"mcp\"").unwrap(),
        "{opencode}"
    );
    let opencode: Value = serde_json::from_str(&opencode).unwrap();
    assert_eq!(opencode["mcp"]["tessera"]["type"], "local");
    assert_eq!(opencode["mcp"]["tessera"]["command"][1], "mcp");

    let cont = fs::read_to_string(s.root.join(".continue/mcpServers/tessera.yaml")).unwrap();
    assert!(cont.contains("schema: v1"), "{cont}");
    assert!(cont.contains("command: tessera"), "{cont}");

    let hooks = json(&s.root.join(".claude/settings.local.json"));
    let post = hooks["hooks"]["PostToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(post.ends_with("hook claude post-edit"), "{post}");
    assert!(hooks["hooks"]["Stop"].is_array());
}

#[test]
fn merging_keeps_existing_entries_and_a_rerun_changes_nothing() {
    let s = setup();
    touch(
        &s.root,
        ".mcp.json",
        "{\n  \"mcpServers\": {\n    \"other\": {\n      \"command\": \"other\"\n    }\n  }\n}\n",
    );
    touch(
        &s.root,
        ".claude/settings.local.json",
        "{\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"say done\"\n          }\n        ]\n      }\n    ]\n  }\n}\n",
    );
    let (out, ok) = init(&s, &["--yes"]);
    assert!(ok, "{out}");
    let mcp = json(&s.root.join(".mcp.json"));
    assert_eq!(mcp["mcpServers"]["other"]["command"], "other");
    assert_eq!(mcp["mcpServers"]["tessera"]["command"], "tessera");
    let hooks = json(&s.root.join(".claude/settings.local.json"));
    assert_eq!(hooks["hooks"]["Stop"].as_array().unwrap().len(), 2);

    let before: Vec<Vec<u8>> = [".mcp.json", ".claude/settings.local.json"]
        .iter()
        .map(|p| fs::read(s.root.join(p)).unwrap())
        .collect();
    let (out, ok) = init(&s, &["--yes"]);
    assert!(ok, "{out}");
    assert!(out.contains("Nothing to change"), "{out}");
    let after: Vec<Vec<u8>> = [".mcp.json", ".claude/settings.local.json"]
        .iter()
        .map(|p| fs::read(s.root.join(p)).unwrap())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn instruction_files_change_only_with_the_flag() {
    let s = setup();
    touch(&s.root, "AGENTS.md", "# Project\n\nUse pnpm.\n");
    touch(&s.root, "CLAUDE.md", "# Claude notes\n");
    init(&s, &["--yes"]);
    assert_eq!(
        fs::read_to_string(s.root.join("AGENTS.md")).unwrap(),
        "# Project\n\nUse pnpm.\n"
    );
    assert_eq!(
        fs::read_to_string(s.root.join("CLAUDE.md")).unwrap(),
        "# Claude notes\n"
    );

    let (out, ok) = init(&s, &["--yes", "--with-instructions"]);
    assert!(ok, "{out}");
    let agents = fs::read_to_string(s.root.join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Project\n\nUse pnpm.\n"), "{agents}");
    assert!(agents.contains("tessera blast"), "{agents}");
    let claude = fs::read_to_string(s.root.join("CLAUDE.md")).unwrap();
    assert!(claude.contains("@AGENTS.md"), "{claude}");

    let (out, _) = init(&s, &["--yes", "--with-instructions"]);
    assert!(out.contains("Nothing to change"), "{out}");
    assert_eq!(
        fs::read_to_string(s.root.join("AGENTS.md")).unwrap(),
        agents
    );
}

#[test]
fn global_config_changes_only_with_the_flag() {
    let s = setup();
    fs::create_dir_all(s.home.join(".codex")).unwrap();
    let (out, ok) = init(&s, &["--yes"]);
    assert!(ok, "{out}");
    assert!(s.root.join(".codex/config.toml").exists(), "{out}");
    assert!(!s.home.join(".codex/config.toml").exists());

    let (out, ok) = init(&s, &["--yes", "--global"]);
    assert!(ok, "{out}");
    let global = fs::read_to_string(s.home.join(".codex/config.toml")).unwrap();
    assert!(global.contains("[mcp_servers.tessera]"), "{global}");
}

#[test]
fn agent_flag_picks_agents_even_when_not_detected() {
    let s = setup();
    let (out, ok) = init(&s, &["--yes", "--agent", "cursor"]);
    assert!(ok, "{out}");
    let cursor = json(&s.root.join(".cursor/mcp.json"));
    assert_eq!(cursor["mcpServers"]["tessera"]["command"], "tessera");
    assert!(!s.root.join(".mcp.json").exists());

    let (out, ok) = init(&s, &["--yes", "--agent", "nope"]);
    assert!(!ok, "{out}");
    assert!(out.contains("cursor"), "lists known agents: {out}");
}

#[test]
fn files_it_cannot_parse_are_left_alone_with_a_snippet() {
    let s = setup();
    let original = "{\n  // my servers\n  \"mcpServers\": {}\n}\n";
    touch(&s.root, ".cursor/mcp.json", original);
    let (out, ok) = init(&s, &["--yes", "--agent", "cursor"]);
    assert!(ok, "{out}");
    assert_eq!(
        fs::read_to_string(s.root.join(".cursor/mcp.json")).unwrap(),
        original
    );
    assert!(out.contains("by hand"), "{out}");
    assert!(out.contains("\"tessera\""), "{out}");
}

#[test]
fn no_agents_found_still_explains_what_to_do() {
    let s = setup();
    let (out, ok) = init(&s, &["--yes"]);
    assert!(ok, "{out}");
    assert!(out.contains("--agent"), "{out}");
    assert!(s.root.join(".tessera/config.toml").exists());
}

#[cfg(unix)]
#[test]
fn git_hook_refuses_a_commit_that_breaks_an_import() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup();
    git(&s.root, &["add", "."]);
    git(&s.root, &["commit", "-q", "-m", "base"]);
    let (out, ok) = init(&s, &["--yes", "--git-hook"]);
    assert!(ok, "{out}");
    let hook = s.root.join(".git/hooks/pre-commit");
    let mode = fs::metadata(&hook).unwrap().permissions().mode();
    assert!(mode & 0o111 != 0, "hook mode {mode:o}");

    let format = fs::read_to_string(s.root.join("src/util/format.ts")).unwrap();
    fs::write(
        s.root.join("src/util/format.ts"),
        format
            .replace(
                "export function formatName(",
                "export function formatDisplayName(",
            )
            .replace("return formatName(", "return formatDisplayName("),
    )
    .unwrap();
    git(&s.root, &["add", "."]);
    let bin = assert_cmd::cargo::cargo_bin("tessera");
    let path = format!("{}:/usr/bin:/bin", bin.parent().unwrap().to_string_lossy());
    let commit = |extra: &[&str]| {
        StdCommand::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "-m",
                "rename",
            ])
            .args(extra)
            .current_dir(&s.root)
            .env("PATH", &path)
            .output()
            .unwrap()
    };
    let refused = commit(&[]);
    assert!(!refused.status.success());
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(said.contains("src/users/repo.ts"), "{said}");
    assert!(commit(&["--no-verify"]).status.success());
}

#[cfg(unix)]
#[test]
fn an_existing_pre_commit_hook_is_never_overwritten() {
    let s = setup();
    touch(&s.root, ".git/hooks/pre-commit", "#!/bin/sh\nnpm test\n");
    let (out, ok) = init(&s, &["--yes", "--git-hook"]);
    assert!(ok, "{out}");
    assert_eq!(
        fs::read_to_string(s.root.join(".git/hooks/pre-commit")).unwrap(),
        "#!/bin/sh\nnpm test\n"
    );
    assert!(out.contains("tessera verify --staged"), "{out}");
}

/// Every `command` string anywhere in a JSON value.
fn commands(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match child {
                    Value::String(text) if key == "command" => out.push(text.clone()),
                    _ => commands(child, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| commands(item, out)),
        _ => {}
    }
}

#[test]
fn every_hook_adapter_is_wired_in_its_documented_shape() {
    use tessera_codegraph::hook::{Agent, Event};
    let s = setup();
    let ids = [
        "claude",
        "codex",
        "qwen",
        "gemini",
        "copilot",
        "cursor",
        "augment",
        "antigravity",
    ];
    let mut args = vec!["--yes"];
    for id in &ids {
        args.push("--agent");
        args.push(id);
    }
    let (out, ok) = init(&s, &args);
    assert!(ok, "{out}");
    let files = [
        ("claude", ".claude/settings.local.json", 4),
        ("codex", ".codex/hooks.json", 4),
        ("qwen", ".qwen/settings.json", 3),
        ("gemini", ".gemini/settings.json", 3),
        ("copilot", ".github/hooks/tessera.json", 3),
        ("cursor", ".cursor/hooks.json", 3),
        ("augment", ".augment/settings.local.json", 3),
        ("antigravity", ".agents/hooks.json", 3),
    ];
    for (agent, path, count) in files {
        let value = json(&s.root.join(path));
        let mut found = Vec::new();
        commands(&value, &mut found);
        let hooks: Vec<&String> = found.iter().filter(|c| c.contains(" hook ")).collect();
        assert_eq!(hooks.len(), count, "{path}: {found:?}");
        for command in hooks {
            let words: Vec<&str> = command.split_whitespace().collect();
            assert_eq!(words[..3], ["tessera", "hook", agent], "{command}");
            assert!(Agent::parse(words[2]).is_some(), "{command}");
            assert!(Event::parse(words[3]).is_some(), "{command}");
        }
    }
    let copilot = json(&s.root.join(".github/hooks/tessera.json"));
    assert_eq!(copilot["version"], 1);
    assert_eq!(copilot["hooks"]["agentStop"][0]["timeoutSec"], 60);
    let cursor = json(&s.root.join(".cursor/hooks.json"));
    assert_eq!(cursor["version"], 1);
    assert!(cursor["hooks"]["stop"][0]["command"].is_string());
    let gemini = json(&s.root.join(".gemini/settings.json"));
    assert_eq!(
        gemini["hooks"]["AfterAgent"][0]["hooks"][0]["timeout"],
        60_000
    );
    assert!(
        gemini["mcpServers"]["tessera"].is_object(),
        "MCP and hooks share the file"
    );
    let antigravity = json(&s.root.join(".agents/hooks.json"));
    assert_eq!(antigravity["tessera"]["Stop"][0]["type"], "command");
    assert!(antigravity["tessera"]["PostToolUse"][0]["matcher"].is_string());

    let (out, _) = init(&s, &args);
    assert!(out.contains("Nothing to change"), "{out}");
}

#[test]
fn without_tessera_on_path_configs_run_the_pinned_npm_package() {
    let s = setup();
    let (out, ok) = init_with_path(&s, &["--yes", "--agent", "claude"], "/usr/bin:/bin");
    assert!(ok, "{out}");
    let package = format!("tessera-codegraph@{}", env!("CARGO_PKG_VERSION"));
    let mcp = json(&s.root.join(".mcp.json"));
    assert_eq!(mcp["mcpServers"]["tessera"]["command"], "npx");
    assert_eq!(mcp["mcpServers"]["tessera"]["args"][0], "-y");
    assert_eq!(mcp["mcpServers"]["tessera"]["args"][1], package.as_str());
    assert_eq!(mcp["mcpServers"]["tessera"]["args"][2], "mcp");
    let hooks = json(&s.root.join(".claude/settings.local.json"));
    let stop = hooks["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert_eq!(stop, format!("npx -y {package} hook claude stop"));
    assert!(out.contains("npx"), "{out}");
}
