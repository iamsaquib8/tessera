use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct InitOptions {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub git_hooks: bool,
    pub mcp_configs: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResult {
    pub root: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub next_steps: Vec<String>,
}

impl Display for InitResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Initialized Tessera defaults under {}", self.root)?;
        if !self.created.is_empty() {
            writeln!(f, "Created:")?;
            for path in &self.created {
                writeln!(f, "  {path}")?;
            }
        }
        if !self.skipped.is_empty() {
            writeln!(f, "Skipped existing files:")?;
            for path in &self.skipped {
                writeln!(f, "  {path}")?;
            }
        }
        writeln!(f, "Next:")?;
        for step in &self.next_steps {
            writeln!(f, "  {step}")?;
        }
        Ok(())
    }
}

pub fn run(options: InitOptions) -> Result<InitResult> {
    let root = options
        .root
        .canonicalize()
        .unwrap_or_else(|_| options.root.clone());
    fs::create_dir_all(root.join(".tessera"))?;

    let mut created = Vec::new();
    let mut skipped = Vec::new();

    write_file(
        &root,
        ".tessera/config.toml",
        &config_toml(&options.db_path),
        options.force,
        &mut created,
        &mut skipped,
    )?;

    if options.mcp_configs {
        write_file(
            &root,
            ".tessera/mcp/codex.toml",
            &codex_mcp(&options.db_path),
            options.force,
            &mut created,
            &mut skipped,
        )?;
        write_file(
            &root,
            ".tessera/mcp/claude.json",
            &claude_mcp(&options.db_path),
            options.force,
            &mut created,
            &mut skipped,
        )?;
        write_file(
            &root,
            ".tessera/mcp/cursor.json",
            &cursor_mcp(&options.db_path),
            options.force,
            &mut created,
            &mut skipped,
        )?;
    }

    if options.git_hooks {
        let hooks = hooks_dir(&root)?;
        for name in ["post-merge", "post-checkout"] {
            let rel = relative_display(&root, &hooks.join(name));
            if write_file(&root, &rel, HOOK, options.force, &mut created, &mut skipped)? {
                make_executable(&root.join(&rel))?;
            }
        }
    }

    Ok(InitResult {
        root: root.display().to_string(),
        created,
        skipped,
        next_steps: vec![
            "tessera status".to_string(),
            "tessera find <symbol>".to_string(),
            "tessera blast <symbol>".to_string(),
        ],
    })
}

const HOOK: &str = "#!/bin/sh\n# Added by `tessera init --git-hooks`: refresh the index after git changes files.\ntessera --quiet index >/dev/null 2>&1 || true\n";

// `.git` is a file in worktrees and submodules, so ask git where hooks live.
fn hooks_dir(root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--path-format=absolute", "--git-path", "hooks"])
        .output();
    match output {
        Ok(output) if output.status.success() => Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim(),
        )),
        _ => bail!(
            "{} is not a git repository; --git-hooks needs one",
            root.display()
        ),
    }
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn write_file(
    root: &Path,
    rel_path: &str,
    content: &str,
    force: bool,
    created: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<bool> {
    let path = root.join(rel_path);
    if path.exists() && !force {
        skipped.push(rel_path.to_string());
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    created.push(rel_path.to_string());
    Ok(true)
}

fn config_toml(_db_path: &Path) -> String {
    "# Tessera project settings. Everything here is optional; see docs/configuration.md.\n\n[index]\n# max_file_bytes = 1_048_576\n\n[include]\n# Only index these path prefixes. Empty means everything.\npaths = []\n\n[exclude]\n# Path prefixes and glob patterns to skip, on top of .gitignore and built-in skips.\npaths = []\nglobs = []\n\n[ignore]\n# Directory or file names to skip anywhere in the tree.\nextra = []\n".to_string()
}

// A JSON string is also a valid TOML basic string, so one escaper serves both.
fn quoted(path: &Path) -> String {
    serde_json::to_string(&path.to_string_lossy()).unwrap_or_else(|_| "\"\"".to_string())
}

fn codex_mcp(db_path: &Path) -> String {
    format!(
        "[mcp_servers.tessera]\ncommand = \"tessera\"\nargs = [\"mcp\", \"--db\", {}]\n",
        quoted(db_path)
    )
}

fn claude_mcp(db_path: &Path) -> String {
    let config = serde_json::json!({
        "mcpServers": {
            "tessera": {
                "command": "tessera",
                "args": ["mcp", "--db", db_path.to_string_lossy()],
            }
        }
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&config).unwrap_or_default()
    )
}

fn cursor_mcp(db_path: &Path) -> String {
    claude_mcp(db_path)
}
