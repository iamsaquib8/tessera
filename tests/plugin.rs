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
    let skill = fs::read_to_string(repo().join("plugin/skills/tessera/SKILL.md")).unwrap();
    assert!(
        skill.starts_with("---\nname: tessera\n"),
        "the plugin ships the skill: {}",
        &skill[..skill.len().min(80)]
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

/// An executable shell script `name` in `dir`.
#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A `tessera` that reports `version` and echoes its arguments.
#[cfg(unix)]
fn fake_tessera(dir: &Path, version: &str) {
    fs::create_dir_all(dir).unwrap();
    script(
        dir,
        "tessera",
        &format!(
            "if [ \"$1\" = --version ]; then echo 'tessera {version}'; else echo \"ran $0 $*\"; fi"
        ),
    );
}

#[cfg(unix)]
fn run_runner(path: &str, home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(repo().join("plugin/scripts/tessera-run"))
        .args(args)
        .env("PATH", path)
        .env("HOME", home)
        .current_dir(home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn runner_runs_an_older_tessera_on_path_instead_of_npx() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = temp.path().join("bin");
    fake_tessera(&bin, "1.0.1");
    // npx and npm exist, as on a machine with Node; neither may be used.
    script(&bin, "npx", "echo npx was used; exit 1");
    script(&bin, "npm", "echo 99.0.0");
    let path = format!("{}:/usr/bin:/bin", bin.display());

    let out = run_runner(&path, temp.path(), &["blast", "x"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}{stderr}");
    assert!(stdout.contains("blast x"), "{stdout}");
    assert!(!stdout.contains("npx"), "{stdout}");
    assert!(
        stderr.contains("1.0.1") && stderr.contains("older"),
        "a warning on stderr: {stderr}"
    );

    let hook = run_runner(&path, temp.path(), &["hook", "claude", "stop"]);
    assert!(hook.status.success());
    assert!(String::from_utf8_lossy(&hook.stdout).contains("hook claude stop"));
}

#[cfg(unix)]
#[test]
fn runner_is_quiet_when_tessera_is_as_new_as_the_plugin() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = temp.path().join("bin");
    for version in [env!("CARGO_PKG_VERSION"), "9.9.9"] {
        fake_tessera(&bin, version);
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let out = run_runner(&path, temp.path(), &["status"]);
        assert!(out.status.success());
        assert!(out.stderr.is_empty(), "{version}: {:?}", out.stderr);
        assert!(String::from_utf8_lossy(&out.stdout).contains("status"));
    }
}

#[cfg(unix)]
#[test]
fn runner_falls_back_to_the_installer_directories() {
    for dir in [".local/bin", ".cargo/bin"] {
        let temp = tempfile::TempDir::new().unwrap();
        fake_tessera(&temp.path().join(dir), env!("CARGO_PKG_VERSION"));
        let empty = temp.path().join("empty");
        fs::create_dir(&empty).unwrap();
        let out = run_runner(&empty.display().to_string(), temp.path(), &["status"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{dir}: {stdout}");
        assert!(stdout.contains(dir), "{dir}: {stdout}");
    }
}

#[cfg(unix)]
#[test]
fn runner_uses_npx_last_and_only_for_a_version_that_exists() {
    let temp = tempfile::TempDir::new().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    script(&bin, "npx", "echo \"npx $*\"");
    let path = bin.display().to_string();

    // npm knows no such version: say how to install, run nothing, exit 0.
    script(&bin, "npm", "exit 1");
    let out = run_runner(&path, temp.path(), &["hook", "claude", "stop"]);
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("npx"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("install.sh"));

    // npm has it.
    script(&bin, "npm", "echo \"$2\"");
    let out = run_runner(&path, temp.path(), &["status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!(
            "npx --yes tessera-codegraph@{} status",
            env!("CARGO_PKG_VERSION")
        )),
        "{stdout}"
    );
}

#[test]
fn skill_and_installer_point_at_install_channels_that_exist() {
    let skill = fs::read_to_string(repo().join("plugin/skills/tessera/SKILL.md")).unwrap();
    let setup = skill.split("## Setup").nth(1).unwrap();
    let setup = setup.split("\n## ").next().unwrap();
    let script = setup
        .find("install.sh")
        .expect("the install script is listed");
    let cargo = setup.find("cargo").expect("cargo is listed");
    assert!(script < cargo, "the install script comes first:\n{setup}");
    for word in ["npx", "npm", "brew"] {
        assert!(
            !setup.contains(word),
            "{word} is not an install path:\n{setup}"
        );
    }
    let installer = fs::read_to_string(repo().join("install.sh")).unwrap();
    assert!(
        !installer.contains("npm "),
        "install.sh must not send anyone to npm"
    );
}
