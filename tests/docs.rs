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
        .replace("<route|rpc|graphql|topic|table|config|service>", "route")
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

/// Install channels that exist today. A page may not advertise one that does
/// not; set a flag here in the pull request that publishes the channel.
const HOMEBREW_PUBLISHED: bool = false;
const PUBLIC_IMAGE_PUBLISHED: bool = false;

/// The pages a visitor reads: the README, the docs, the site and the plugin.
fn user_facing_pages() -> Vec<std::path::PathBuf> {
    let mut found = vec![std::path::PathBuf::from("README.md")];
    for dir in ["docs", "website/src", "plugin"] {
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                // Design notes are not install guidance, and the site's content
                // folder is copied from docs/ at build time.
                if path.ends_with("docs/design") || path.ends_with("website/src/content") {
                    continue;
                }
                stack.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            } else if path.extension().is_some_and(|e| e == "md" || e == "mdx")
                && !path.ends_with("docs/repo-settings.md")
            {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn pages_do_not_advertise_channels_that_are_not_published() {
    let pages = user_facing_pages();
    assert!(pages.len() > 20, "found {} pages", pages.len());
    for page in pages {
        let text = fs::read_to_string(&page).unwrap();
        if !HOMEBREW_PUBLISHED {
            assert!(
                !text.contains("brew install"),
                "{} tells people to `brew install`, but there is no tap yet",
                page.display()
            );
        }
        if !PUBLIC_IMAGE_PUBLISHED {
            assert!(
                !text.contains("ghcr.io/iamsaquib8/tessera"),
                "{} names the container image, which is not public yet",
                page.display()
            );
        }
    }
}

#[test]
fn the_unpublished_channels_are_said_to_be_unavailable() {
    for page in ["README.md", "docs/quickstart.md"] {
        let text = fs::read_to_string(page).unwrap();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            text.contains("Homebrew tap and a public container image are not available yet"),
            "{page}"
        );
    }
}

#[test]
fn security_md_says_how_to_verify_a_release_and_where_the_network_is_used() {
    let text = fs::read_to_string("SECURITY.md").unwrap();
    assert!(text.contains("gh attestation verify tessera-"));
    assert!(text.contains("--repo iamsaquib8/tessera"));
    assert!(text.contains("sha256sum -c --ignore-missing SHA256SUMS"));
    assert!(text.contains("It opens no outbound connection."));
    for installer in ["install.sh", "npx", "cargo install"] {
        assert!(text.contains(installer), "{installer}");
    }
    assert!(text.contains("within 3 days"), "a response target in days");
}
