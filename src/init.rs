use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
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
        write_file(
            &root,
            ".git/hooks/post-merge",
            "#!/bin/sh\ntessera index .\n",
            options.force,
            &mut created,
            &mut skipped,
        )?;
        write_file(
            &root,
            ".git/hooks/post-checkout",
            "#!/bin/sh\ntessera index .\n",
            options.force,
            &mut created,
            &mut skipped,
        )?;
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

fn write_file(
    root: &Path,
    rel_path: &str,
    content: &str,
    force: bool,
    created: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<()> {
    let path = root.join(rel_path);
    if path.exists() && !force {
        skipped.push(rel_path.to_string());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    created.push(rel_path.to_string());
    Ok(())
}

fn config_toml(_db_path: &Path) -> String {
    "# Tessera project settings. Everything here is optional; see docs/configuration.md.\n\n[index]\n# max_file_bytes = 1_048_576\n\n[include]\n# Only index these path prefixes. Empty means everything.\npaths = []\n\n[exclude]\n# Path prefixes and glob patterns to skip, on top of .gitignore and built-in skips.\npaths = []\nglobs = []\n\n[ignore]\n# Directory or file names to skip anywhere in the tree.\nextra = []\n".to_string()
}

fn codex_mcp(db_path: &Path) -> String {
    format!(
        "[[mcp_servers]]\nname = \"tessera\"\ncommand = \"tessera\"\nargs = [\"mcp\", \"--db\", \"{}\"]\n",
        db_path.display()
    )
}

fn claude_mcp(db_path: &Path) -> String {
    format!(
        "{{\n  \"mcpServers\": {{\n    \"tessera\": {{\n      \"command\": \"tessera\",\n      \"args\": [\"mcp\", \"--db\", \"{}\"]\n    }}\n  }}\n}}\n",
        db_path.display()
    )
}

fn cursor_mcp(db_path: &Path) -> String {
    claude_mcp(db_path)
}
