# Agent integrations

Tessera never calls a language model, so it works with any model:
Claude, GPT, Gemini, Grok, DeepSeek, Qwen, or a local one. What matters is
the **agent** you run. Every agent that speaks MCP gets the nine tools
below; agents with hook systems also get [checks before and after every
edit](hooks.md).

The fastest setup is `tessera init` in the repository: it detects your
agents, shows the diff of each config change, and asks before writing
(`--yes` to skip the question, `--dry-run` to only look, `--agent <name>` to
pick, `--global` for user-wide config). The sections below are the same
configuration by hand.

The sourced, per-agent matrix of config paths, hook events and trust steps
lives in [agents.md](agents.md).

## The command every agent runs

```sh
tessera mcp            # stdio; run from the repository, or pass --root
tessera mcp --http 127.0.0.1:8765   # Streamable HTTP for clients that need a URL
```

The server finds the repository root itself, answers at once, indexes in the
background, and re-indexes files as they change.

Without a global install, use `npx -y tessera-codegraph mcp` wherever a
config below says `tessera mcp`. In JSON configs that means
`"command": "npx", "args": ["-y", "tessera-codegraph", "mcp"]`.

## Claude Code

```sh
claude mcp add tessera -- tessera mcp
# no global install:
claude mcp add tessera -- npx -y tessera-codegraph mcp
```

Or commit a project `.mcp.json`:

```json
{ "mcpServers": { "tessera": { "command": "tessera", "args": ["mcp"] } } }
```

DeepSeek users can run Claude Code against DeepSeek's Anthropic-compatible
API (`ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic`); everything here
applies unchanged.

## Cursor

`.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (all projects):

```json
{ "mcpServers": { "tessera": { "command": "tessera", "args": ["mcp"] } } }
```

## OpenAI Codex CLI

```sh
codex mcp add tessera -- tessera mcp
```

or in `~/.codex/config.toml`:

```toml
[mcp_servers.tessera]
command = "tessera"
args = ["mcp"]
```

## Gemini CLI and Google Antigravity

Gemini CLI, `.gemini/settings.json`:

```json
{ "mcpServers": { "tessera": { "command": "tessera", "args": ["mcp"] } } }
```

Antigravity (`agy`), `.agents/mcp_config.json`:

```json
{ "mcpServers": { "tessera": { "command": "tessera", "args": ["mcp"] } } }
```

## GitHub Copilot

VS Code agent mode, `.vscode/mcp.json` (note the key is `servers`):

```json
{ "servers": { "tessera": { "type": "stdio", "command": "tessera", "args": ["mcp"] } } }
```

Copilot CLI: `copilot mcp add tessera -- tessera mcp`.

## Grok Build, Qwen Code, Kiro, Junie, Augment, Warp

These read an `mcpServers` map in their own settings file. The command is
always `tessera` with args `["mcp"]`. File locations are in
[agents.md](agents.md):

| Agent | File |
| --- | --- |
| Grok Build | `.grok/config.toml` (`[mcp_servers.tessera]`), or it reads `.mcp.json` |
| Qwen Code | `.qwen/settings.json` |
| Kiro | `.kiro/settings/mcp.json` |
| JetBrains Junie | `.junie/mcp/mcp.json` |
| Augment (Auggie) | `~/.augment/settings.json` |
| Warp | `.warp/.mcp.json`, or it reads `.mcp.json` |

## OpenCode and Kilo Code

`opencode.json` (Kilo: `kilo.jsonc`) uses an `mcp` map with a command array:

```json
{ "mcp": { "tessera": { "type": "local", "command": ["tessera", "mcp"] } } }
```

## Cline, Continue, Zed, Goose, Amp

| Agent | Config |
| --- | --- |
| Cline | `cline mcp add`, or `cline_mcp_settings.json` with `"type": "stdio"` |
| Continue | `.continue/mcpServers/tessera.yaml` |
| Zed | `context_servers` → `{ "tessera": { "command": "tessera", "args": ["mcp"] } }` |
| Goose | `extensions:` in `~/.config/goose/config.yaml`, type `stdio`, `cmd: tessera`, `args: [mcp]` |
| Amp | `amp mcp add tessera -- tessera mcp` |

## Aider and anything without MCP

Run the CLI directly. Aider appends the edited file's name to its lint
command, so you can point the lint loop at Tessera per language and failures
reach the model:

```sh
aider --lint-cmd "python: tessera --quiet verify --language python --snippet" \
      --lint-cmd "typescript: tessera --quiet verify --language typescript --snippet"
```

## Every agent: a git backstop

Whatever the agent, a broken edit should not reach a commit:

```sh
tessera init --git-hook
```

installs a pre-commit hook that runs `tessera verify --staged` and refuses a
commit that breaks a call site, an argument count, or an import.
`git commit --no-verify` skips it once. To add it to an existing hook
yourself, append:

```sh
tessera verify --staged --format agent --quiet; [ $? -ne 1 ] || exit 1
```
