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
    // `ui` serves until stopped; tests/ui.rs covers it.
    if command.contains("<rev>") || command.starts_with("--") || command == "tessera ui" {
        return None;
    }
    let filled = command
        .replace("<symbol | file>", "src/util/format.ts")
        .replace("<symbol>", "findById")
        .replace("<file>", "src/users/repo.ts")
        .replace("<pattern>", "load*")
        .replace("<from> <to>", "handler formatName")
        .replace("<lang>", "typescript")
        .replace("<route|rpc|topic|table|service>", "route")
        .replace(" [query]", "");
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
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    }
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
        quick_start.contains("npx tessera-codegraph init"),
        "the quick start uses npx, so its setup step must too"
    );
}

#[test]
fn readme_agent_grid_matches_the_registry() {
    let readme = fs::read_to_string("README.md").unwrap();
    for agent in tessera_codegraph::agents::AGENTS {
        let row = readme
            .lines()
            .find(|line| line.starts_with(&format!("| {} |", agent.name)))
            .unwrap_or_else(|| panic!("README grid lacks {}", agent.name));
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        assert_eq!(
            cells[3].starts_with('✓'),
            agent.hooks.is_some(),
            "{}: hooks column",
            agent.name
        );
        assert_eq!(
            cells[2].contains("--global"),
            agent.mcp.is_none() && agent.global_mcp.is_some(),
            "{}: MCP column",
            agent.name
        );
    }
}

#[test]
fn translated_readmes_are_drafts_with_working_links() {
    for lang in ["zh-CN", "ja", "ko", "es", "pt-BR"] {
        let name = format!("README.{lang}.md");
        let text = fs::read_to_string(&name).unwrap();
        assert!(
            text.contains("Draft translation awaiting review"),
            "{name} must say it is a draft"
        );
        assert!(
            fs::read_to_string("README.md").unwrap().contains(&name),
            "README.md must link {name}"
        );
        for target in text
            .split("](")
            .skip(1)
            .chain(text.split("href=\"").skip(1))
            .filter_map(|rest| rest.split([')', '"', '#']).next())
            .filter(|target| !target.is_empty() && !target.starts_with("http"))
        {
            assert!(
                std::path::Path::new(target).exists(),
                "{name} links to missing {target}"
            );
        }
    }
}
