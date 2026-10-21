//! The agents Tessera can be wired into. Each entry comes from the vendor's
//! documentation, summarised in docs/agents.md; what the docs do not confirm
//! is left out and `init` says so.

pub mod diff;
pub mod edit;

use std::path::{Path, PathBuf};

/// How an agent's MCP config file lists servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpFormat {
    /// JSON `{"mcpServers": {"tessera": {"command", "args"}}}`.
    McpServers,
    /// Amp's JSON `{"amp.mcpServers": {...}}`.
    AmpMcpServers,
    /// VS Code's JSON `{"servers": {"tessera": {"type": "stdio", ...}}}`.
    VsCodeServers,
    /// OpenCode and Kilo: JSON `{"mcp": {"tessera": {"type": "local", "command": [...]}}}`.
    OpenCode,
    /// Zed's JSON `{"context_servers": {...}}`.
    ContextServers,
    /// TOML `[mcp_servers.tessera]` (Codex, Grok).
    Toml,
    /// A Continue block file of its own.
    ContinueYaml,
    /// Goose's YAML `extensions:` map.
    GooseYaml,
}

/// One hook event in an agent's hook config.
#[derive(Debug, Clone, Copy)]
pub struct HookEvent {
    /// The agent's event name.
    pub name: &'static str,
    /// Tessera's event (`tessera hook <agent> <event>`).
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub seconds: u64,
}

const fn on(name: &'static str, event: &'static str, seconds: u64) -> HookEvent {
    HookEvent {
        name,
        event,
        matcher: None,
        seconds,
    }
}

const fn on_tools(
    name: &'static str,
    event: &'static str,
    matcher: &'static str,
    seconds: u64,
) -> HookEvent {
    HookEvent {
        name,
        event,
        matcher: Some(matcher),
        seconds,
    }
}

/// How an agent's hook file is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookLayout {
    /// `{"hooks": {"Event": [{"matcher"?, "hooks": [{"type", "command", "timeout"}]}]}}`,
    /// timeouts in seconds (Claude Code, Codex, Qwen).
    Nested,
    /// The same, with timeouts in milliseconds (Gemini, Augment).
    NestedMillis,
    /// `{"version": 1, "hooks": {"event": [{"type", "command", "timeoutSec"}]}}` (Copilot).
    Copilot,
    /// `{"version": 1, "hooks": {"event": [{"command", "timeout"}]}}` (Cursor).
    Cursor,
    /// `{"tessera": {"Event": [...]}}`: a named group; tool events nest a
    /// matcher group, the others list handlers directly (Antigravity).
    Antigravity,
}

#[derive(Debug, Clone, Copy)]
pub struct HookConfig {
    pub path: &'static str,
    /// The agent name `tessera hook` takes.
    pub agent: &'static str,
    pub layout: HookLayout,
    pub events: &'static [HookEvent],
}

#[derive(Debug, Clone, Copy)]
pub struct McpTarget {
    /// Relative to the project root, or to the home directory when global.
    pub path: &'static str,
    pub format: McpFormat,
    /// The client expands `${workspaceFolder}`, so the server can be pointed
    /// at the project even when the client starts it elsewhere.
    pub workspace_variable: bool,
}

/// Instruction files an agent reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instructions {
    AgentsMd,
    /// `CLAUDE.md` wins over `AGENTS.md` when it exists, so it gets an
    /// `@AGENTS.md` import instead of a second copy of the text.
    ClaudeMd,
    Markdown(&'static str),
    CursorRule,
}

#[derive(Debug, Clone, Copy)]
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    /// Files or directories in the project that show it is used here.
    pub project_markers: &'static [&'static str],
    /// Executables on `PATH` that show it is installed.
    pub binaries: &'static [&'static str],
    /// Directories under the home directory that show it is installed.
    pub home_markers: &'static [&'static str],
    pub mcp: Option<McpTarget>,
    pub global_mcp: Option<McpTarget>,
    /// Where Tessera's hooks go, for agents with a hook adapter.
    pub hooks: Option<HookConfig>,
    pub instructions: Option<Instructions>,
    /// A step only the user can take before the config takes effect.
    pub trust: Option<&'static str>,
    /// What Tessera cannot do for this agent yet.
    pub limits: Option<&'static str>,
}

const fn mcp(path: &'static str, format: McpFormat) -> Option<McpTarget> {
    Some(McpTarget {
        path,
        format,
        workspace_variable: false,
    })
}

const fn mcp_in_workspace(path: &'static str, format: McpFormat) -> Option<McpTarget> {
    Some(McpTarget {
        path,
        format,
        workspace_variable: true,
    })
}

const NONE: Agent = Agent {
    id: "",
    name: "",
    project_markers: &[],
    binaries: &[],
    home_markers: &[],
    mcp: None,
    global_mcp: None,
    hooks: None,
    instructions: None,
    trust: None,
    limits: None,
};

pub const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        project_markers: &[".claude", "CLAUDE.md", ".mcp.json"],
        binaries: &["claude"],
        home_markers: &[".claude"],
        mcp: mcp(".mcp.json", McpFormat::McpServers),
        global_mcp: mcp(".claude.json", McpFormat::McpServers),
        hooks: Some(HookConfig {
            path: ".claude/settings.local.json",
            agent: "claude",
            layout: HookLayout::Nested,
            events: &[
                on("SessionStart", "session-start", 10),
                on_tools("PreToolUse", "pre-edit", "Edit|Write|MultiEdit", 10),
                on_tools("PostToolUse", "post-edit", "Edit|Write|MultiEdit", 30),
                on("Stop", "stop", 60),
            ],
        }),
        instructions: Some(Instructions::ClaudeMd),
        trust: Some("Claude Code asks you to approve the project's MCP server the first time; the Tessera plugin is an alternative to these files (use one or the other)."),
        ..NONE
    },
    Agent {
        id: "codex",
        name: "Codex CLI",
        project_markers: &[".codex"],
        binaries: &["codex"],
        home_markers: &[".codex"],
        mcp: mcp(".codex/config.toml", McpFormat::Toml),
        global_mcp: mcp(".codex/config.toml", McpFormat::Toml),
        instructions: Some(Instructions::AgentsMd),
        trust: Some("Codex reads .codex/ only in projects you trust, and runs new hooks only after you review them with /hooks."),
        hooks: Some(HookConfig {
            path: ".codex/hooks.json",
            agent: "codex",
            layout: HookLayout::Nested,
            events: &[
                on("SessionStart", "session-start", 10),
                on_tools("PreToolUse", "pre-edit", "Edit|Write|apply_patch", 10),
                on_tools("PostToolUse", "post-edit", "Edit|Write|apply_patch", 30),
                on("Stop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        project_markers: &[".gemini", "GEMINI.md"],
        binaries: &["gemini"],
        home_markers: &[".gemini"],
        mcp: mcp(".gemini/settings.json", McpFormat::McpServers),
        global_mcp: mcp(".gemini/settings.json", McpFormat::McpServers),
        instructions: Some(Instructions::Markdown("GEMINI.md")),
        limits: Some("no pre-edit note: its pre-tool hook carries no context"),
        hooks: Some(HookConfig {
            path: ".gemini/settings.json",
            agent: "gemini",
            layout: HookLayout::NestedMillis,
            events: &[
                on("SessionStart", "session-start", 10),
                on_tools("AfterTool", "post-edit", "write_file|replace", 30),
                on("AfterAgent", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "antigravity",
        name: "Google Antigravity",
        project_markers: &[".agents"],
        binaries: &["agy"],
        home_markers: &[".gemini/config"],
        mcp: mcp(".agents/mcp_config.json", McpFormat::McpServers),
        global_mcp: mcp(".gemini/config/mcp_config.json", McpFormat::McpServers),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its post-edit hook cannot return feedback, so problems are reported when it stops"),
        hooks: Some(HookConfig {
            path: ".agents/hooks.json",
            agent: "antigravity",
            layout: HookLayout::Antigravity,
            events: &[
                on("SessionStart", "session-start", 10),
                on_tools(
                    "PostToolUse",
                    "post-edit",
                    "write_to_file|replace_file_content|multi_replace_file_content",
                    30,
                ),
                on("Stop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "copilot",
        name: "GitHub Copilot CLI",
        project_markers: &[".github/copilot-instructions.md", ".github/hooks"],
        binaries: &["copilot"],
        home_markers: &[".copilot"],
        mcp: mcp(".mcp.json", McpFormat::McpServers),
        global_mcp: mcp(".copilot/mcp-config.json", McpFormat::McpServers),
        instructions: Some(Instructions::Markdown(".github/copilot-instructions.md")),
        limits: Some("no pre-edit note: its pre-tool hook can only allow or deny"),
        hooks: Some(HookConfig {
            path: ".github/hooks/tessera.json",
            agent: "copilot",
            layout: HookLayout::Copilot,
            events: &[
                on("sessionStart", "session-start", 10),
                on("postToolUse", "post-edit", 30),
                on("agentStop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "vscode",
        name: "VS Code Copilot agent",
        project_markers: &[".vscode"],
        binaries: &["code"],
        mcp: mcp_in_workspace(".vscode/mcp.json", McpFormat::VsCodeServers),
        instructions: Some(Instructions::Markdown(".github/copilot-instructions.md")),
        limits: Some("hooks are in preview and not wired"),
        ..NONE
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        project_markers: &[".cursor", ".cursorrules"],
        binaries: &["cursor", "cursor-agent"],
        home_markers: &[".cursor"],
        mcp: mcp_in_workspace(".cursor/mcp.json", McpFormat::McpServers),
        global_mcp: mcp(".cursor/mcp.json", McpFormat::McpServers),
        instructions: Some(Instructions::CursorRule),
        limits: Some("no pre-edit note; Cursor stops follow-ups after 5 in a row"),
        hooks: Some(HookConfig {
            path: ".cursor/hooks.json",
            agent: "cursor",
            layout: HookLayout::Cursor,
            events: &[
                on("sessionStart", "session-start", 10),
                on("postToolUse", "post-edit", 30),
                on("stop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "grok",
        name: "Grok Build",
        project_markers: &[".grok"],
        binaries: &["grok"],
        home_markers: &[".grok"],
        mcp: mcp(".grok/config.toml", McpFormat::Toml),
        global_mcp: mcp(".grok/config.toml", McpFormat::Toml),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("only its pre-edit hook can block, so the edit-safety loop is not wired; use the git hook"),
        ..NONE
    },
    Agent {
        id: "qwen",
        name: "Qwen Code",
        project_markers: &[".qwen", "QWEN.md"],
        binaries: &["qwen"],
        home_markers: &[".qwen"],
        mcp: mcp(".qwen/settings.json", McpFormat::McpServers),
        global_mcp: mcp(".qwen/settings.json", McpFormat::McpServers),
        instructions: Some(Instructions::Markdown("QWEN.md")),
        limits: Some("no pre-edit note: its pre-tool hook would have to approve the edit"),
        hooks: Some(HookConfig {
            path: ".qwen/settings.json",
            agent: "qwen",
            layout: HookLayout::Nested,
            events: &[
                on("SessionStart", "session-start", 10),
                on("PostToolUse", "post-edit", 30),
                on("Stop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        project_markers: &["opencode.json", ".opencode"],
        binaries: &["opencode"],
        mcp: mcp("opencode.json", McpFormat::OpenCode),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("hooks need a JavaScript plugin; use the git hook"),
        ..NONE
    },
    Agent {
        id: "kilo",
        name: "Kilo Code",
        project_markers: &[".kilo", "kilo.jsonc"],
        binaries: &["kilo"],
        mcp: mcp(".kilo/kilo.jsonc", McpFormat::OpenCode),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("hooks need a plugin; use the git hook"),
        ..NONE
    },
    Agent {
        id: "kiro",
        name: "Kiro",
        project_markers: &[".kiro"],
        binaries: &["kiro", "kiro-cli"],
        home_markers: &[".kiro"],
        mcp: mcp(".kiro/settings/mcp.json", McpFormat::McpServers),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its stop hook is unverified, so hooks are not wired"),
        ..NONE
    },
    Agent {
        id: "junie",
        name: "JetBrains Junie",
        project_markers: &[".junie"],
        binaries: &["junie"],
        mcp: mcp(".junie/mcp/mcp.json", McpFormat::McpServers),
        instructions: Some(Instructions::Markdown(".junie/AGENTS.md")),
        limits: Some("it has no post-edit hook; use the git hook"),
        ..NONE
    },
    Agent {
        id: "amp",
        name: "Amp",
        project_markers: &[".amp"],
        binaries: &["amp"],
        mcp: mcp(".amp/settings.json", McpFormat::AmpMcpServers),
        instructions: Some(Instructions::AgentsMd),
        trust: Some("Amp runs workspace MCP servers only after `amp mcp approve`."),
        limits: Some("hooks need a TypeScript plugin; use the git hook"),
        ..NONE
    },
    Agent {
        id: "warp",
        name: "Warp",
        project_markers: &[".warp"],
        mcp: mcp(".mcp.json", McpFormat::McpServers),
        instructions: Some(Instructions::AgentsMd),
        trust: Some("Warp asks you to approve workspace MCP servers."),
        ..NONE
    },
    Agent {
        id: "continue",
        name: "Continue",
        project_markers: &[".continue"],
        mcp: mcp(".continue/mcpServers/tessera.yaml", McpFormat::ContinueYaml),
        ..NONE
    },
    Agent {
        id: "cline",
        name: "Cline",
        project_markers: &[".clinerules"],
        binaries: &["cline"],
        home_markers: &[".cline"],
        global_mcp: mcp(
            ".cline/data/settings/cline_mcp_settings.json",
            McpFormat::McpServers,
        ),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its MCP config is global only (use --global)"),
        ..NONE
    },
    Agent {
        id: "devin",
        name: "Devin Desktop",
        project_markers: &[".devin"],
        home_markers: &[".config/devin"],
        global_mcp: mcp(".config/devin/mcp_config.json", McpFormat::McpServers),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its MCP config is global only (use --global); its hooks cannot add context"),
        ..NONE
    },
    Agent {
        id: "augment",
        name: "Augment (Auggie)",
        project_markers: &[".augment"],
        binaries: &["auggie"],
        home_markers: &[".augment"],
        global_mcp: mcp(".augment/settings.json", McpFormat::McpServers),
        limits: Some("its MCP config is global only (use --global); no pre-edit note; hooks run in the CLI only"),
        hooks: Some(HookConfig {
            path: ".augment/settings.local.json",
            agent: "augment",
            layout: HookLayout::NestedMillis,
            events: &[
                on("SessionStart", "session-start", 10),
                on("PostToolUse", "post-edit", 30),
                on("Stop", "stop", 60),
            ],
        }),
        ..NONE
    },
    Agent {
        id: "zed",
        name: "Zed",
        project_markers: &[".zed"],
        binaries: &["zed"],
        home_markers: &[".config/zed"],
        global_mcp: mcp(".config/zed/settings.json", McpFormat::ContextServers),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its MCP config is global only (use --global); no hooks"),
        ..NONE
    },
    Agent {
        id: "goose",
        name: "Goose",
        project_markers: &[".goosehints"],
        binaries: &["goose"],
        home_markers: &[".config/goose"],
        global_mcp: mcp(".config/goose/config.yaml", McpFormat::GooseYaml),
        instructions: Some(Instructions::AgentsMd),
        limits: Some("its MCP config is global only (use --global); hooks need a plugin"),
        ..NONE
    },
];

pub fn by_id(id: &str) -> Option<&'static Agent> {
    let id = id.to_ascii_lowercase();
    AGENTS.iter().find(|agent| {
        agent.id == id
            || agent
                .name
                .to_ascii_lowercase()
                .split_whitespace()
                .next()
                .is_some_and(|first| first == id)
    })
}

pub fn ids() -> Vec<&'static str> {
    AGENTS.iter().map(|agent| agent.id).collect()
}

/// Why an agent counts as present: a project marker, an executable on
/// `PATH`, or its directory under `home`.
pub fn detect(agent: &Agent, root: &Path, home: Option<&Path>) -> Option<String> {
    if let Some(marker) = agent
        .project_markers
        .iter()
        .find(|marker| root.join(marker).exists())
    {
        return Some(format!("{marker} in the project"));
    }
    if let Some(binary) = agent
        .binaries
        .iter()
        .find(|binary| on_path(binary).is_some())
    {
        return Some(format!("`{binary}` on PATH"));
    }
    let home = home?;
    agent
        .home_markers
        .iter()
        .find(|marker| home.join(marker).is_dir())
        .map(|marker| format!("~/{marker}"))
}

pub fn on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        [binary.to_string(), format!("{binary}.exe")]
            .into_iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

pub fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_agent_has_a_config() {
        let mut ids = ids();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count);
        for agent in AGENTS {
            assert!(
                agent.mcp.is_some() || agent.global_mcp.is_some(),
                "{}",
                agent.id
            );
        }
    }

    #[test]
    fn agents_are_found_by_id_or_first_word_of_the_name() {
        assert_eq!(by_id("Claude").unwrap().id, "claude");
        assert_eq!(by_id("cursor").unwrap().id, "cursor");
        assert!(by_id("nope").is_none());
    }
}
