use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use tessera_codegraph::hook::{Agent, Event};

const RUNNER: &str = "${CLAUDE_PLUGIN_ROOT}/scripts/tessera-run";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn marketplace_lists_the_plugin_in_this_repository() {
    let marketplace = json(&repo().join(".claude-plugin/marketplace.json"));
    assert_eq!(marketplace["name"], "tessera");
    assert!(marketplace["owner"]["name"].is_string());
    assert!(marketplace["description"].is_string());
    let entry = &marketplace["plugins"][0];
    assert_eq!(entry["name"], "tessera");
    let source = entry["source"].as_str().unwrap();
    assert!(source.starts_with("./"), "{source}");
    assert!(repo()
        .join(source)
        .join(".claude-plugin/plugin.json")
        .is_file());
}

#[test]
fn manifest_matches_the_crate() {
    let manifest = json(&repo().join("plugin/.claude-plugin/plugin.json"));
    assert_eq!(manifest["name"], "tessera");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["license"], env!("CARGO_PKG_LICENSE"));
    for field in ["description", "homepage", "repository", "license"] {
        assert!(manifest[field].is_string(), "{field}");
    }
    assert!(manifest["author"]["name"].is_string());
    let runner = fs::read_to_string(repo().join("plugin/scripts/tessera-run")).unwrap();
    assert!(
        runner.contains(&format!("version={}\n", env!("CARGO_PKG_VERSION"))),
        "the runner pins the crate version"
    );
}

#[test]
fn every_hook_runs_a_known_agent_and_event_through_the_runner() {
    let hooks = json(&repo().join("plugin/hooks/hooks.json"));
    let events = hooks["hooks"].as_object().expect("a `hooks` wrapper");
    for name in ["SessionStart", "PreToolUse", "PostToolUse", "Stop"] {
        let groups = events[name].as_array().unwrap_or_else(|| panic!("{name}"));
        for group in groups {
            for hook in group["hooks"].as_array().unwrap() {
                assert_eq!(hook["type"], "command");
                let command = hook["command"].as_str().unwrap();
                let rest = command
                    .strip_prefix(&format!("\"{RUNNER}\" hook "))
                    .unwrap_or_else(|| panic!("{command}"));
                let mut words = rest.split_whitespace();
                assert_eq!(Agent::parse(words.next().unwrap()), Some(Agent::Claude));
                assert!(Event::parse(words.next().unwrap()).is_some(), "{command}");
                assert!(hook["timeout"].as_u64().is_some(), "{command}");
            }
        }
    }
    assert_eq!(events["PreToolUse"][0]["matcher"], "Edit|Write|MultiEdit");
}

#[test]
fn mcp_server_runs_through_the_runner() {
    let mcp = json(&repo().join("plugin/.mcp.json"));
    assert_eq!(mcp["mcpServers"]["tessera"]["command"], RUNNER);
    assert_eq!(mcp["mcpServers"]["tessera"]["args"][0], "mcp");
}

#[test]
fn skill_and_commands_ship_with_the_plugin() {
    assert_eq!(
        fs::read_to_string(repo().join("plugin/skills/tessera/SKILL.md")).unwrap(),
        fs::read_to_string(repo().join("skills/tessera/SKILL.md")).unwrap(),
        "plugin/skills/tessera/SKILL.md must be a copy of skills/tessera/SKILL.md"
    );
    for command in ["blast", "verify"] {
        let text =
            fs::read_to_string(repo().join(format!("plugin/commands/{command}.md"))).unwrap();
        assert!(text.starts_with("---\ndescription: "), "{command}");
        assert!(
            text.contains(&format!("tessera-run\" {command}")),
            "{command}"
        );
    }
}

#[cfg(unix)]
#[test]
fn runner_prefers_a_matching_tessera_and_never_fails_a_hook() {
    use std::os::unix::fs::PermissionsExt;
    let runner = repo().join("plugin/scripts/tessera-run");
    let mode = fs::metadata(&runner).unwrap().permissions().mode();
    assert!(mode & 0o111 != 0, "runner mode {mode:o}");

    let bin = assert_cmd::cargo::cargo_bin("tessera");
    let path = bin.parent().unwrap().display().to_string();
    let version = Command::new(&runner)
        .arg("--version")
        .env("PATH", &path)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("tessera {}", env!("CARGO_PKG_VERSION"))
    );

    // An empty PATH: no tessera and no npx.
    let empty = tempfile::TempDir::new().unwrap();
    let nothing = empty.path().display().to_string();
    for path in [path.as_str(), nothing.as_str()] {
        let hook = Command::new(&runner)
            .args(["hook", "claude", "stop"])
            .current_dir(empty.path())
            .env("PATH", path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(hook.status.success(), "PATH={path}");
    }
}
