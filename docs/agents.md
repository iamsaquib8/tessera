# Agent support matrix

Input for `tessera init` adapters and the hook entry point. Verified against
vendor docs on 2026-10-21. Anything marked **unverified** must not be
implemented until a vendor doc confirms it. Re-verify before each release:
agent hook contracts change often.

Tessera never calls an LLM, so model choice does not matter. Users of Grok,
DeepSeek, Qwen, or local models get Tessera through whichever agent they run.

What Tessera wires from this matrix today, per agent, is in
[hooks.md](hooks.md) (hooks) and [integrations.md](integrations.md) (MCP);
`tessera init` reads the same registry.

## Tiers

| Tier | Meaning | Agents |
| --- | --- | --- |
| **A** | Hook after an edit can feed back or block, and an end-of-turn hook can force the agent to continue | Claude Code, Codex CLI, Gemini CLI, Copilot CLI, Cursor IDE, Qwen Code, Factory Droid, Augment (Auggie), Amp (TS plugins), VS Code Copilot (preview) |
| **A−** | Half the loop | Stop gate only: Antigravity, Junie, Goose. Post-edit feedback only: Kiro, OpenCode, Kilo Code, Cline CLI. Cursor CLI coverage is partial |
| **B+** | Can only block before a tool call | Grok Build, Crush, Devin Desktop (formerly Windsurf) |
| **B** | MCP + instructions only | Zed, Warp, Continue, Trae (until its hook schema is confirmed), Cline in VS Code/JetBrains |
| **C** | No MCP | Aider (use its `--lint-cmd` / `--test-cmd` loop to run `tessera verify`) |

Every tier also gets the git backstop (`tessera init --git-hook`).

## Matrix

| Agent | MCP config | Hooks (edit / stop events, block, context) | Instructions | Distribution | Source |
| --- | --- | --- | --- | --- | --- |
| Claude Code | stdio/http. Project `.mcp.json`, user `~/.claude.json`, key `mcpServers`; `claude mcp add -s project` | `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`; stdin JSON. Pre blocks via exit 2 or `hookSpecificOutput.permissionDecision: "deny"`; Post `decision: "block"` + `reason`; Stop `decision: "block"`. `additionalContext` on Pre/Post/Stop (10k chars) | `CLAUDE.md`, `.claude/rules`; reads `AGENTS.md` only when no `CLAUDE.md` exists | Plugin: `.claude-plugin/plugin.json`, `hooks/hooks.json`, `.mcp.json`; catalog `.claude-plugin/marketplace.json` | code.claude.com/docs/en/{hooks,mcp,memory,plugins} |
| Codex CLI | stdio + streamable HTTP. `~/.codex/config.toml`, `.codex/config.toml` (trusted projects), `[mcp_servers.<name>]`; `codex mcp add` | `PreToolUse`, `PostToolUse`, `Stop`; `hooks.json` or `[hooks]`. Exit 2, `permissionDecision: deny`, `decision: block`; Stop block becomes a continuation prompt; context on Pre/Post. `apply_patch` matches `Edit`/`Write`. Hooks need trust via `/hooks` | `AGENTS.override.md` > `AGENTS.md` (32 KiB cap) | Plugins (skills, MCP, hooks); Agent Plugins | learn.chatgpt.com/docs/hooks, /docs/extend/mcp |
| Gemini CLI (enterprise / API-key only since 2026-06-18) | `command` / `url` (SSE) / `httpUrl`. `~/.gemini/settings.json`, `.gemini/settings.json`, `mcpServers`; `gemini mcp add` | `BeforeTool`, `AfterTool`, `AfterAgent`. Exit 2 or `decision: "deny"` + reason; AfterTool `additionalContext`; AfterAgent deny forces retry. Edit tools `write_file`, `replace` | `GEMINI.md` (`AGENTS.md` via `context.fileName`) | Extensions (`gemini-extension.json`) | geminicli.com/docs/hooks/reference |
| Google Antigravity (app, IDE, `agy` CLI) | stdio `command`; remote via `serverUrl` only. `.agents/mcp_config.json`, `~/.gemini/config/mcp_config.json`, `mcpServers` | `PreToolUse`, `PostToolUse`, `Stop` in `.agents/hooks.json`; camelCase input (`toolCall.name/args`). Pre `decision: "deny"`; **PostToolUse output is `{}` only** (no post-edit feedback); Stop `decision: "continue"` with reason injected. Exit codes **unverified**. Edit tools `write_to_file`, `replace_file_content`, `multi_replace_file_content` | `AGENTS.md`, `GEMINI.md`, `.agents/rules/*.md` | Plugins (`plugin.json` + `mcp_config.json` + `hooks.json`); `agy plugin install` | antigravity.google/docs/{hooks,mcp,rules,plugins} |
| GitHub Copilot CLI | stdio/http. `~/.copilot/mcp-config.json`; reads workspace `.mcp.json`; `copilot mcp add` | `preToolUse`, `postToolUse`, `agentStop` in `.github/hooks/*.json`; camelCase input. `permissionDecision: "deny"`; Post `additionalContext` (10 KB); agentStop `decision: "block"` (max 8 in a row) | `copilot-instructions.md`, `.github/instructions/**`, `AGENTS.md` | `copilot plugin install`; reads `.claude-plugin/` | docs.github.com/en/copilot/reference/hooks-configuration |
| VS Code Copilot agent | `.vscode/mcp.json`, key **`servers`** | Preview: `PreToolUse`, `PostToolUse`, `Stop` in `.github/hooks/*.json`; Claude-format hooks only with `chat.useClaudeHooks` (matchers ignored). Exit-code semantics **unverified** | `copilot-instructions.md`, `AGENTS.md`, `CLAUDE.md` (settings) | Plugins | code.visualstudio.com/docs/copilot/customization/hooks |
| Cursor (IDE, `agent` CLI) | `.cursor/mcp.json`, `~/.cursor/mcp.json`, `mcpServers` | `preToolUse`, `postToolUse`, `afterFileEdit`, `stop` in `.cursor/hooks.json`. Exit 2 or `permission: "deny"`; postToolUse `additional_context`; stop `followup_message` (limit 5); afterFileEdit has no output. CLI coverage partial (**unverified**) | `.cursor/rules/*.mdc`, nested `AGENTS.md` | `.cursor-plugin/plugin.json`, Cursor Marketplace | cursor.com/docs/agent/hooks |
| Grok Build (xAI, beta) | `~/.grok/config.toml`, `.grok/config.toml`, `[mcp_servers.<name>]`; also reads `.mcp.json`, `.cursor/mcp.json`; `grok mcp add` | 14 events in `.grok/hooks/*.json`; also reads `.claude/settings.json` hooks. **Only PreToolUse can block** (fails open). Post feedback and Stop gate **unverified**. Needs `/hooks-trust` | `AGENTS.md` | Plugins + marketplace | docs.x.ai/build/features/{hooks,mcp-servers} |
| DeepSeek | No first-party agent. Documented path: Claude Code with `ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic` — all Claude Code hooks and MCP apply | — | — | — | api-docs.deepseek.com/quick_start/agent_integrations/claude_code |
| Qwen Code | `~/.qwen/settings.json`, `.qwen/settings.json`, `mcpServers`; `qwen mcp add` | Claude-shaped events; exit 2 sends stderr to the model; Pre/Post `additionalContext`; Stop `decision: block` (cap 8) | `QWEN.md` | `qwen-extension.json`; installs Claude marketplace plugins | github.com/QwenLM/qwen-code docs |
| OpenCode | `opencode.json`, key `mcp`, `{type: "local", command: [...]}` | JS/TS plugins only: `tool.execute.before` (throw blocks), `tool.execute.after`, `file.edited`, `session.idle`; Stop re-prompt **unverified** | `AGENTS.md` (falls back to `CLAUDE.md`) | npm `plugin` array, `.opencode/plugins/` | opencode.ai/docs/{mcp-servers,plugins} |
| Kilo Code | `kilo.jsonc` / `.kilo/kilo.jsonc`, key `mcp` (OpenCode shape) | Same as OpenCode; `tool.execute.after` can rewrite output | `AGENTS.md` | `kilo plugin`, `.kilo/plugin/` | kilo.ai/docs/automate/extending/plugins |
| Crush | `crushrc` (JSON deprecated); `mcp add` | `PreToolUse` only; exit 2 blocks, exit 49 halts; `context` field | `AGENTS.md`, `CRUSH.md` | — | github.com/charmbracelet/crush |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json`, `mcpServers`, set `type: "streamableHttp"` explicitly (default is SSE) | TS plugins `beforeTool` / `afterTool` (`appendContext`), CLI/SDK only | `.clinerules/`, `AGENTS.md` | `cline plugin install` | docs.cline.bot |
| Devin Desktop (formerly Windsurf) | `~/.config/devin/mcp_config.json`, `mcpServers` (`serverUrl`) | `pre_write_code`, `post_write_code`, `post_cascade_response` in `.devin/hooks.json`; exit 2 blocks pre-hooks only; no context, no stop gate | `.devin/rules/*.md`, `AGENTS.md` | — | docs.devin.ai/desktop/cascade/hooks |
| Zed | `context_servers` | none | first match of `.rules` … `AGENTS.md` … | Extensions | zed.dev/docs/ai/mcp |
| Aider | no MCP | none (lint/test commands) | none automatic | — | aider.chat/docs/usage/lint-test.html |
| Goose | `~/.config/goose/config.yaml` `extensions:` (`stdio` / `streamable_http`) | plugin hooks; block only on PreToolUse and Stop; no context | `AGENTS.md`, `.goosehints` | Plugins | goose-docs.ai/docs/guides |
| Amp | `amp.mcpServers` in `.amp/settings.json` (needs `amp mcp approve`); `amp mcp add` | TS plugins: `tool.call`, `tool.result`, `agent.end` (`action: 'continue'`, max 5) | `AGENTS.md` | Plugins | ampcode.com/docs/customize/plugins |
| Kiro (replaces Amazon Q Developer CLI) | `.kiro/settings/mcp.json`, `mcpServers`; `/mcp add` | `.kiro/hooks/*.json`: Pre/Post Tool Use, Agent Stop, File Save; exit 0 stdout → context, non-zero blocks Pre. Stop continuation **unverified** | `.kiro/steering/*.md`, `AGENTS.md` | Powers | kiro.dev/docs/hooks |
| Factory Droid | **unverified** | Claude-shaped, `.factory/hooks.json` | **unverified** | `.factory-plugin/plugin.json` | docs.factory.com/reference/hooks-reference |
| Warp | `.warp/.mcp.json`; also reads `.mcp.json` | none documented | `AGENTS.md` | — | docs.warp.dev/agents/capabilities/mcp |
| JetBrains Junie | `.junie/mcp/mcp.json`, `mcpServers` | CLI: `PreToolUse`, `Stop` (no PostToolUse); project hooks ignored by default | `.junie/AGENTS.md`, `AGENTS.md` | Extensions | junie.jetbrains.com/docs/junie-cli-hooks.html |
| Augment (Auggie) | `~/.augment/settings.json`, `mcpServers`; `auggie mcp add` | `PreToolUse`, `PostToolUse` (gets `file_changes`), `Stop`; exit 2 / `permissionDecision: deny`; project hooks CLI-only | **unverified** | **unverified** | docs.augmentcode.com/cli/hooks |
| Trae | `.trae/mcp.json` (third-party sources) | hooks shipped v3.5.66; schema **unverified** | `AGENTS.md`, `CLAUDE.md` | **unverified** | docs.trae.ai/ide/changelog |
| Continue | `.continue/mcpServers/*.yaml` | none | `.continue/rules/` | Hub | docs.continue.dev |
| Roo Code | — | — | — | **Shut down 2026-05-15** | — |

## Implementation notes

- **Payload detection.** Several clients run each other's hook files (Cursor
  loads Claude hooks, Grok reads `.claude/settings.json`, VS Code reads Claude
  format). `tessera hook` must detect which shape arrived (`tool_input`,
  `toolArgs`, `toolCall.args`) and check the tool name itself, not rely on
  matchers.
- **Agent Plugins 1.0** (VS Code, Cursor, Copilot, Codex, Kiro) bundles skills
  and MCP but not hooks; hooks are wired per client.
- **`CLAUDE.md` vs `AGENTS.md`.** Claude Code ignores `AGENTS.md` when a
  `CLAUDE.md` exists; `init --with-instructions` adds `@AGENTS.md` to
  `CLAUDE.md` instead of duplicating text.
- **Trust gates.** Codex, Grok, and Qwen need the project trusted before hooks
  run; Junie ignores project hooks by default; Amp and Warp need workspace MCP
  approval. `init` prints the exact trust step per agent.
- **Format traps.** VS Code uses `servers`; OpenCode and Kilo use `mcp` with a
  `command` array; Cline defaults to SSE without `type`; Goose dropped SSE;
  Antigravity uses `serverUrl`.

## Not yet verified

Antigravity exit codes; Grok Build post-edit feedback and stop gate; VS Code
exit-code semantics; Cursor CLI hook coverage; Kiro stop continuation; Trae
hook schema and MCP paths; Factory MCP and instructions; Augment instructions
and distribution; Continue and Warp hooks; OpenCode stop re-prompt; Codex
plugin manifest path. Also unchecked: Mistral Vibe, Kimi CLI, Qoder CLI,
CodeBuddy.
