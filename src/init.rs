//! `tessera init`: wires Tessera into the agents a project uses. It plans
//! every file change first, so the caller can show the diff and ask before
//! anything is written.

use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::agents::edit::{self, Edit, Server};
use crate::agents::{self, diff, Agent, Instructions, McpTarget};

#[derive(Debug, Clone)]
pub struct InitOptions {
    pub root: PathBuf,
    pub db_path: PathBuf,
    /// Agent ids to set up whether or not they are detected; empty means
    /// every detected agent.
    pub agents: Vec<String>,
    /// Also change user-wide config under the home directory.
    pub global: bool,
    /// Also add usage instructions to the files each agent reads.
    pub with_instructions: bool,
    /// Install a pre-commit hook that runs `verify --staged`.
    pub git_hook: bool,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoundAgent {
    pub id: String,
    pub name: String,
    /// Why it was picked: a project marker, an executable, or `--agent`.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    /// Relative to the root, or `~/…` for user-wide files.
    pub path: String,
    #[serde(skip)]
    pub absolute: PathBuf,
    pub created: bool,
    pub agents: Vec<String>,
    #[serde(skip)]
    pub before: Option<String>,
    #[serde(skip)]
    pub after: String,
    #[serde(skip)]
    pub executable: bool,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualStep {
    pub agent: String,
    pub path: String,
    pub reason: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub agent: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitPlan {
    pub root: String,
    pub agents: Vec<FoundAgent>,
    pub changes: Vec<Change>,
    pub manual: Vec<ManualStep>,
    pub notes: Vec<Note>,
    /// `tessera` is not on PATH, and every config runs it by that name.
    pub not_on_path: bool,
    /// What the plan was made from, so `apply` can plan a file again.
    #[serde(skip)]
    options: InitOptions,
}

struct Pending {
    absolute: PathBuf,
    original: Option<String>,
    current: Option<String>,
    agents: Vec<String>,
    executable: bool,
}

struct Planner<'a> {
    options: &'a InitOptions,
    on_path: bool,
    root: PathBuf,
    files: BTreeMap<String, Pending>,
    manual: Vec<ManualStep>,
}

impl Planner<'_> {
    fn edit(
        &mut self,
        display: String,
        absolute: PathBuf,
        agent: &str,
        apply: impl FnOnce(Option<&str>) -> Edit,
    ) {
        if !self.files.contains_key(&display) {
            let original = match fs::read_to_string(&absolute) {
                Ok(text) => Some(text),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    self.manual.push(ManualStep {
                        agent: agent.to_string(),
                        path: display,
                        reason: format!("it could not be read ({error})"),
                        snippet: String::new(),
                    });
                    return;
                }
            };
            self.files.insert(
                display.clone(),
                Pending {
                    absolute,
                    current: original.clone(),
                    original,
                    agents: Vec::new(),
                    executable: false,
                },
            );
        }
        let pending = self.files.get_mut(&display).expect("inserted above");
        match apply(pending.current.as_deref()) {
            Edit::Write(text) => {
                pending.current = Some(text);
                if !pending.agents.iter().any(|a| a == agent) {
                    pending.agents.push(agent.to_string());
                }
            }
            Edit::Unchanged => {}
            Edit::Manual { reason, snippet } => self.manual.push(ManualStep {
                agent: agent.to_string(),
                path: display,
                reason,
                snippet,
            }),
        }
    }

    fn project(&mut self, rel: &str, agent: &str, apply: impl FnOnce(Option<&str>) -> Edit) {
        let absolute = self.root.join(rel);
        self.edit(rel.to_string(), absolute, agent, apply);
    }

    /// How configs start Tessera: by name when it is on PATH, otherwise the
    /// npm package at this version through npx, so a quick start with npx
    /// works without a global install.
    fn launcher(&self) -> Vec<String> {
        if self.on_path {
            vec!["tessera".to_string()]
        } else {
            vec![
                "npx".to_string(),
                "-y".to_string(),
                format!("tessera-codegraph@{}", env!("CARGO_PKG_VERSION")),
            ]
        }
    }

    fn server(&self, target: &McpTarget, global: bool) -> Server {
        let mut launcher = self.launcher();
        let command = launcher.remove(0);
        let mut args = launcher;
        args.push("mcp".to_string());
        let default_db = self.root.join(".tessera/tessera.db");
        if !global && self.options.db_path != default_db {
            args.push("--db".to_string());
            args.push(self.options.db_path.to_string_lossy().into_owned());
        } else if !global && target.workspace_variable {
            args.push("--root".to_string());
            args.push("${workspaceFolder}".to_string());
        }
        Server { command, args }
    }

    fn agent(&mut self, agent: &Agent) {
        if let Some(target) = agent.mcp {
            let server = self.server(&target, false);
            self.project(target.path, agent.id, |text| {
                edit::mcp(target.format, text, &server)
            });
        }
        if self.options.global {
            if let (Some(target), Some(home)) = (agent.global_mcp, self.options.home.clone()) {
                let server = self.server(&target, true);
                self.edit(
                    format!("~/{}", target.path),
                    home.join(target.path),
                    agent.id,
                    |text| edit::mcp(target.format, text, &server),
                );
            }
        }
        if let Some(hooks) = agent.hooks {
            let command = self.launcher().join(" ");
            self.project(hooks.path, agent.id, |text| {
                edit::hooks(text, &hooks, &command)
            });
        }
        if self.options.with_instructions {
            match agent.instructions {
                Some(Instructions::AgentsMd) => {
                    self.project("AGENTS.md", agent.id, edit::markdown_block)
                }
                Some(Instructions::ClaudeMd) => {
                    self.project("AGENTS.md", agent.id, edit::markdown_block);
                    self.project("CLAUDE.md", agent.id, edit::claude_import);
                }
                Some(Instructions::Markdown(path)) => {
                    self.project(path, agent.id, edit::markdown_block)
                }
                Some(Instructions::CursorRule) => {
                    self.project(".cursor/rules/tessera.mdc", agent.id, edit::cursor_rule)
                }
                None => {}
            }
        }
    }
}

const CONFIG_TOML: &str = "# Tessera project settings. Everything here is optional; see docs/configuration.md.\n\n[index]\n# max_file_bytes = 1_048_576\n\n[include]\n# Only index these path prefixes. Empty means everything.\npaths = []\n\n[exclude]\n# Path prefixes and glob patterns to skip, on top of .gitignore and built-in skips.\npaths = []\nglobs = []\n\n[ignore]\n# Directory or file names to skip anywhere in the tree.\nextra = []\n";

/// Every change `init` would make, and what it cannot do. Writes nothing.
pub fn plan(options: &InitOptions) -> Result<InitPlan> {
    let root = options
        .root
        .canonicalize()
        .unwrap_or_else(|_| options.root.clone());
    let mut found = Vec::new();
    if options.agents.is_empty() {
        for agent in agents::AGENTS {
            if let Some(reason) = agents::detect(agent, &root, options.home.as_deref()) {
                found.push((agent, reason));
            }
        }
    } else {
        for id in &options.agents {
            let Some(agent) = agents::by_id(id) else {
                bail!(
                    "unknown agent `{id}`; known agents: {}",
                    agents::ids().join(", ")
                );
            };
            if !found
                .iter()
                .any(|(a, _): &(&Agent, String)| a.id == agent.id)
            {
                found.push((agent, "--agent".to_string()));
            }
        }
    }

    let on_path = agents::on_path("tessera").is_some();
    let mut planner = Planner {
        options,
        on_path,
        root: root.clone(),
        files: BTreeMap::new(),
        manual: Vec::new(),
    };
    planner.project(".tessera/config.toml", "tessera", |text| match text {
        Some(_) => Edit::Unchanged,
        None => Edit::Write(CONFIG_TOML.to_string()),
    });
    let mut notes = Vec::new();
    for (agent, _) in &found {
        planner.agent(agent);
        for text in [agent.trust, agent.limits].into_iter().flatten() {
            notes.push(Note {
                agent: agent.name.to_string(),
                text: text.to_string(),
            });
        }
    }
    if options.git_hook {
        let hooks = hooks_dir(&root)?;
        let display = hooks
            .join("pre-commit")
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| hooks.join("pre-commit").to_string_lossy().into_owned());
        planner.edit(
            display.clone(),
            hooks.join("pre-commit"),
            "git",
            edit::pre_commit,
        );
        if let Some(pending) = planner.files.get_mut(&display) {
            pending.executable = true;
        }
    }

    let changes = planner
        .files
        .into_iter()
        .filter(|(_, p)| p.current != p.original)
        .map(|(path, p)| {
            let after = p.current.unwrap_or_default();
            Change {
                diff: diff::unified(&path, p.original.as_deref(), &after),
                created: p.original.is_none(),
                path,
                absolute: p.absolute,
                agents: p.agents,
                before: p.original,
                after,
                executable: p.executable,
            }
        })
        .collect();
    Ok(InitPlan {
        root: root.display().to_string(),
        agents: found
            .into_iter()
            .map(|(agent, reason)| FoundAgent {
                id: agent.id.to_string(),
                name: agent.name.to_string(),
                reason,
            })
            .collect(),
        changes,
        manual: planner.manual,
        notes,
        not_on_path: !on_path,
        options: options.clone(),
    })
}

/// What [`apply`] did, by display path.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Applied {
    pub written: Vec<String>,
    /// Files that changed after the plan was made: planned again from their
    /// new content, and written.
    pub replanned: Vec<String>,
    /// Files left alone, and why.
    pub skipped: Vec<(String, String)>,
}

/// Writes the planned changes. A file that changed since the plan was made
/// (Claude Code rewrites `~/.claude.json` all the time) is planned again
/// from what it holds now, so nothing written in between is lost. Each file
/// is replaced whole, through a temporary file in its directory, keeping
/// its permissions; a symlinked config is written where the link points.
pub fn apply(plan: &InitPlan) -> Result<Applied> {
    let mut applied = Applied::default();
    let mut again: Option<InitPlan> = None;
    for change in &plan.changes {
        let current = read_existing(&change.absolute)?;
        let change = if current == change.before {
            change.clone()
        } else {
            if again.is_none() {
                again = Some(self::plan(&plan.options)?);
            }
            let replanned = again
                .as_ref()
                .and_then(|p| p.changes.iter().find(|c| c.path == change.path));
            match replanned {
                Some(fresh) if fresh.before == current => {
                    applied.replanned.push(change.path.clone());
                    fresh.clone()
                }
                Some(_) => {
                    applied.skipped.push((
                        change.path.clone(),
                        "it kept changing while init ran; run init again".to_string(),
                    ));
                    continue;
                }
                None => {
                    applied.skipped.push((
                        change.path.clone(),
                        "it changed after the plan, and now needs nothing".to_string(),
                    ));
                    continue;
                }
            }
        };
        write_whole(&change.absolute, &change.after, change.executable)?;
        applied.written.push(change.path.clone());
    }
    Ok(applied)
}

/// The file's text, or `None` when it does not exist.
fn read_existing(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

/// Replaces `path` with `content` by a rename, so a reader never sees half
/// a file.
fn write_whole(path: &Path, content: &str, executable: bool) -> Result<()> {
    use std::io::Write;

    let is_link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let path = if is_link {
        fs::canonicalize(path).with_context(|| format!("failed to follow {}", path.display()))?
    } else {
        path.to_path_buf()
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.tessera-{}.tmp", std::process::id()));
    let permissions = fs::metadata(&path).ok().map(|m| m.permissions());
    let written = (|| -> std::io::Result<()> {
        let _ = fs::remove_file(&tmp);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        if let Some(permissions) = permissions {
            fs::set_permissions(&tmp, permissions)?;
        }
        fs::rename(&tmp, &path)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(error).with_context(|| format!("failed to write {}", path.display()));
    }
    if executable {
        make_executable(&path)?;
    }
    Ok(())
}

impl Display for InitPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.agents.is_empty() {
            writeln!(
                f,
                "No agents found in this project or on PATH. Pass --agent <name> to set one up anyway; known agents: {}.",
                agents::ids().join(", ")
            )?;
        } else {
            writeln!(f, "Agents:")?;
            for agent in &self.agents {
                writeln!(f, "  {} ({})", agent.name, agent.reason)?;
            }
        }
        for change in &self.changes {
            writeln!(f)?;
            f.write_str(&change.diff)?;
        }
        if !self.manual.is_empty() {
            writeln!(f, "\nAdd by hand:")?;
            for step in &self.manual {
                writeln!(f, "  {} ({}): {}", step.path, step.agent, step.reason)?;
                for line in step.snippet.lines() {
                    writeln!(f, "    {line}")?;
                }
            }
        }
        if !self.notes.is_empty() {
            writeln!(f, "\nNotes:")?;
            for note in &self.notes {
                writeln!(f, "  {}: {}", note.agent, note.text)?;
            }
        }
        if self.not_on_path {
            writeln!(
                f,
                "\n`tessera` is not on your PATH, so these configs run it through npx, which adds a moment to each hook. For faster hooks, install it (npm i -g tessera-codegraph, Homebrew, or cargo) and put `tessera` in place of the npx command."
            )?;
        }
        Ok(())
    }
}

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
            "{} is not a git repository; --git-hook needs one",
            root.display()
        ),
    }
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
