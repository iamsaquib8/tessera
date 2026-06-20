# Integrations

Tessera speaks **MCP** (Model Context Protocol). Any agent that supports MCP can call its tools. Index once per repo (or via a post-clone hook), then point your agent at the binary.

```sh
cd path/to/your/repo
tessera index .
```

**Exposed tools** (all of them, in every client below): `find_definition` · `find_references` · `get_outline` · `expand_symbol` · `impact` · `connect` · `export` · `context_pack` · `diff_impact` · `imports` · `imported_by` · `signature` · `siblings` · `search` · `unused` · `validate` · `validate_snippet` · `tests_for` · `stats`. Schemas in [mcp.md](mcp.md).

## Claude Code

Either edit `~/.claude.json` directly, or run from your repo root:

```sh
claude mcp add tessera tessera -- mcp --db .tessera/tessera.db
```

Restart Claude Code; the tools appear as `mcp__tessera__*`. Confirm with `/mcp` in the session.

## Cursor

Add to `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` (per-project):

```json
{
  "mcpServers": {
    "tessera": {
      "command": "tessera",
      "args": ["mcp", "--db", "${workspaceFolder}/.tessera/tessera.db"]
    }
  }
}
```

Reload the window. Tessera shows up under **Settings → MCP**.

## Cline (VS Code)

Open the Cline panel → `…` menu → **MCP Servers** → **Configure MCP Servers**, then add:

```json
{
  "mcpServers": {
    "tessera": {
      "command": "tessera",
      "args": ["mcp", "--db", ".tessera/tessera.db"],
      "disabled": false,
      "autoApprove": ["find_definition", "find_references", "get_outline", "search", "validate"]
    }
  }
}
```

The `autoApprove` list is optional — those are the read-only queries safe to run without a prompt.

## Continue.dev (VS Code / JetBrains)

In `~/.continue/config.json` (or workspace `.continue/config.json`):

```json
{
  "experimental": {
    "modelContextProtocolServers": [
      {
        "transport": {
          "type": "stdio",
          "command": "tessera",
          "args": ["mcp", "--db", ".tessera/tessera.db"]
        }
      }
    ]
  }
}
```

## Codex CLI (OpenAI)

In `~/.codex/config.toml`:

```toml
[[mcp_servers]]
name = "tessera"
command = "tessera"
args = ["mcp", "--db", ".tessera/tessera.db"]
```

## Zed

In `~/.config/zed/settings.json`:

```json
{
  "context_servers": {
    "tessera": {
      "command": {
        "path": "tessera",
        "args": ["mcp", "--db", ".tessera/tessera.db"]
      }
    }
  }
}
```

## Aider

Aider doesn't speak MCP yet, but you can drop Tessera into its shell-out workflow:

```sh
aider --read-only-cmd "tessera get-outline {file}" \
      --pre-prompt "Use \`tessera impact {symbol}\` before editing any function."
```

For richer integration, point Aider at `tessera search` and `tessera impact` from its `/run` hook.

## GPT / ChatGPT (custom GPTs, no MCP)

Build a simple wrapper that exposes the CLI as HTTP — e.g. `tessera mcp` behind a stdio→HTTP shim like [`mcp-proxy`](https://github.com/sparfenyuk/mcp-proxy) — and register it as a custom GPT action. The tool schemas in [mcp.md](mcp.md) map 1:1 to OpenAPI function specs.

## Custom / library

Any process can spawn `tessera mcp --db <path>` and talk JSON-RPC over stdio. Or skip MCP entirely and use the [Rust library](../README.md#use-as-a-rust-library) — `Index::open(...).impact(...)` is exactly the same query path the MCP server uses.

For local clients that need HTTP instead of stdio, run:

```sh
tessera mcp-http --addr 127.0.0.1:8765 --db .tessera/tessera.db
```

The HTTP transport exposes `POST /mcp` for JSON-RPC, `GET /sse` as a simple
readiness event stream, and `GET /health` for health checks.

## Tip: re-index after pulls

Add this to a git hook (`.git/hooks/post-merge`, `post-checkout`):

```sh
tessera index .   # incremental by default — sha-diff skips unchanged files
```

A 950-file Java repo with one changed file re-indexes in ~70 ms.
