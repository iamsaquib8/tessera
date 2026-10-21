# Directories, registries and awesome lists

Slow but durable discovery. Most entries are a one-line pull request or a
web form. Submit after v1.0.0 is tagged, every install path is smoke-tested,
and the README, social preview and tessera-bench repository are public
(RUNBOOK, Phase 0).

## Descriptions

Pick the one that fits the list, and adjust it to the list's format. Every
claim in them is in the README.

**One line (MCP lists):**

> **[Tessera](https://github.com/iamsaquib8/tessera)**: Deterministic edit safety for AI coding agents. Callers and tests to run before an edit; missing names, argument counts, broken imports and removed routes or gRPC methods after it. Local tree-sitter + SQLite index, no LLM, 12 languages.

**Agent and hook lists (Claude Code, Codex, Cursor):**

> **[Tessera](https://github.com/iamsaquib8/tessera)**: Hooks that run `verify` after every edit and hold the agent at "done" until the callers it broke are fixed. Claude Code plugin; `tessera init` sets up 21 agents with a diff preview. Local, no LLM, Apache-2.0.

**Rust and developer-tool lists:**

> **[Tessera](https://github.com/iamsaquib8/tessera)**: A tree-sitter and SQLite code index that checks changes: blast radius, broken call sites and imports, and a contract map of HTTP routes, gRPC, Kafka topics and tables across services. CLI, MCP server, local web UI, GitHub Action.

**GitHub Actions lists:**

> **[Tessera](https://github.com/iamsaquib8/tessera/tree/main/action)**: Comments on each pull request with what the change breaks (missing names, argument counts, broken imports, removed routes or gRPC methods), the callers of the changed code, and the tests to run.

## MCP registries

The server is `tessera mcp` (stdio) or `tessera mcp --http 127.0.0.1:8765`
(Streamable HTTP). Nine read-only
tools: `find`, `search`, `refs`, `blast`, `verify`, `context`, `path`,
`map`, `status`. Install line for listings:
`npx -y tessera-codegraph mcp`.

| Target | How | Link |
| --- | --- | --- |
| Official MCP Registry | Publish a `server.json` with the registry's publisher CLI (npm package `tessera-codegraph`). Check the current process first; it has changed often | https://registry.modelcontextprotocol.io |
| `modelcontextprotocol/servers` | Check whether the README still takes community entries; it has been pointing people to the registry instead | https://github.com/modelcontextprotocol/servers |
| `punkpeye/awesome-mcp-servers` | Pull request under the coding-agents or developer-tools category | https://github.com/punkpeye/awesome-mcp-servers |
| `wong2/awesome-mcp-servers` | Pull request | https://github.com/wong2/awesome-mcp-servers |
| `appcypher/awesome-mcp-servers` | Pull request under development tools | https://github.com/appcypher/awesome-mcp-servers |
| Glama | Indexes public MCP repositories; claim the listing | https://glama.ai/mcp/servers |
| Smithery | Publish the server | https://smithery.ai |
| PulseMCP | Submit form | https://www.pulsemcp.com |
| mcp.so | Submit form | https://mcp.so |
| mcpservers.org | Submit form | https://mcpservers.org |

## Agent ecosystems

| Target | How | Notes |
| --- | --- | --- |
| Claude Code plugin lists (for example `hesreallyhim/awesome-claude-code`) | Pull request under hooks or plugins | Install is `/plugin marketplace add iamsaquib8/tessera`, then `/plugin install tessera@tessera` |
| Cursor, Codex, Gemini CLI, Qwen Code community lists | Pull request or forum post with the hook setup | Point to `tessera init` and docs/hooks.md rather than pasting config |
| Agents with MCP only (OpenCode, Goose, Continue, Cline, Zed and others) | Their docs and communities often collect MCP servers | Use the snippet for that agent from docs/integrations.md |

## GitHub Marketplace

The Action lives in `action/`, and Marketplace only lists an `action.yml` at
a repository root. `uses: iamsaquib8/tessera/action@v1` works without a
listing. To list it, publish `iamsaquib8/tessera-action` with the same
`action.yml` at its root (RUNBOOK, Phase 0, optional).

## Rust and general developer tools

| Target | How | Notes |
| --- | --- | --- |
| `rust-unofficial/awesome-rust` | Pull request under Development tools | Has an inclusion bar (activity and some adoption); submit once there is a little of both |
| lib.rs | Automatic from crates.io | Check that the category and keywords (`mcp`, `codegraph`, `tree-sitter`, `agents`, `cli`) look right after publish |
| AI coding tool lists | Pull request | These lists churn; search for the active ones at submission time |

## Tips

- One pull request per list, each with the description that fits it.
  Maintainers close obvious copy-paste submissions.
- Space submissions over the launch week instead of sending them all in an
  hour.
- Keep a checklist of what was submitted where, and follow up on stale pull
  requests after a couple of weeks.
- When a list asks for a category, prefer the one for code analysis or
  developer tools over a generic "AI" bucket.
