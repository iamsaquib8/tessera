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
            format!("tessera index . --db {}", options.db_path.display()),
            format!("tessera doctor --db {}", options.db_path.display()),
            "tessera impact <symbol>".to_string(),
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

fn config_toml(db_path: &Path) -> String {
    format!(
        "# Tessera project defaults\n[index]\ndb = \"{}\"\nwatch_poll_ms = 500\nwatch_debounce_ms = 250\n\n[ignore]\n# Built-in ignores already cover .git, node_modules, target, dist, build, .next, virtualenvs, __pycache__, and .tessera.\nextra = []\n",
        db_path.display()
    )
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
