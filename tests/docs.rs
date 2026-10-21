mod support;

use std::fs;
use std::process::Command;

use support::{copy_dir, fixture_dir};
use tempfile::TempDir;

fn readme_commands() -> Vec<String> {
    let readme = fs::read_to_string("README.md").unwrap();
    readme
        .lines()
        .filter(|line| line.starts_with("| `tessera "))
        .flat_map(|line| {
            let line = line.replace("\\|", "\u{1}");
            let cell = line
                .split('|')
                .nth(1)
                .unwrap_or_default()
                .replace('\u{1}', "|");
            cell.split('`')
                .filter(|part| part.starts_with("tessera ") || part.starts_with("--"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn concrete(command: &str) -> Option<Vec<String>> {
    if command.contains("<rev>") || command.starts_with("--") {
        return None;
    }
    let filled = command
        .replace("<symbol | file>", "src/util/format.ts")
        .replace("<symbol>", "findById")
        .replace("<file>", "src/users/repo.ts")
        .replace("<pattern>", "load*")
        .replace("<from> <to>", "handler formatName")
        .replace("<lang>", "typescript");
    assert!(
        !filled.contains('<'),
        "unhandled placeholder in README command `{command}`"
    );
    Some(
        filled
            .split_whitespace()
            .skip(1)
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn readme_command_table_runs_against_a_fixture() {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir("typescript"), temp.path());
    let commands = readme_commands();
    assert!(
        commands.len() >= 8,
        "README command table not found: {commands:?}"
    );
    for command in &commands {
        let Some(args) = concrete(command) else {
            continue;
        };
        let output = Command::new(env!("CARGO_BIN_EXE_tessera"))
            .arg("--quiet")
            .args(&args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        let expected = if args.first().map(String::as_str) == Some("verify") {
            [0, 1]
        } else {
            [0, 0]
        };
        let code = output.status.code().unwrap_or(-1);
        assert!(
            expected.contains(&code),
            "`{command}` exited {code}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty(), "`{command}` printed nothing");
    }
}

#[test]
fn readme_mcp_setup_works_without_a_global_install() {
    let readme = std::fs::read_to_string("README.md").unwrap();
    let quick_start = readme
        .split("## Quick start")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap();
    assert!(
        quick_start.contains("claude mcp add tessera -- npx -y tessera-codegraph mcp"),
        "the quick start uses npx, so its MCP command must too"
    );
}
