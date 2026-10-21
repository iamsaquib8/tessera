# MCP server

```sh
tessera mcp                          # stdio
tessera mcp --http 127.0.0.1:8765    # Streamable HTTP on POST /mcp, health on GET /health
```

Both transports start answering at once. The first index builds in the
background; until it exists, tool calls return `isError: true` with "Index is
building … retry in a few seconds". After that, a file watcher re-indexes
changed files within about 200 ms.

## Protocol

Tessera is a dual-era server:

- **Modern clients (2026-07-28).** Requests carry
  `_meta["io.modelcontextprotocol/protocolVersion"]` and are served
  statelessly. `server/discover` lists the supported versions. An
  unsupported version gets error `-32022` with the `supported` list.
- **Legacy clients (2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05).** They
  use the `initialize` handshake. Tessera echoes the requested version when it
  supports it, and otherwise answers 2025-11-25.

Errors follow JSON-RPC:

| Code | Meaning |
| --- | --- |
| `-32700` | parse error |
| `-32600` | invalid request; batches are not supported |
| `-32601` | unknown method |
| `-32602` | unknown tool or invalid arguments |

A tool that runs but fails returns a normal result with `isError: true`, so
the model can read the message and recover. Notifications never get a reply.

HTTP specifics:

- `Origin` must be absent or `localhost` / `127.0.0.1` / `[::1]`; any other
  origin gets 403.
- Notifications get 202.
- `GET` and `DELETE` on `/mcp` get 405.
- Modern requests must send `MCP-Protocol-Version`, `Mcp-Method`, and, for
  `tools/call`, `Mcp-Name`, all matching the body. A missing or mismatched
  header gets 400 with code `-32020`.

## Tools

Every tool is read-only (`readOnlyHint: true`) and rejects unknown arguments.
Responses are compact text by default; pass `"format": "json"` for
machine-readable output. List tools are paginated: `limit` (default 25) and
`cursor`. When there is more, the text ends with `cursor o:N`.

| Tool | Use it for | Arguments |
| --- | --- | --- |
| `find` | Where a symbol is defined: kind, file:line, signature, optional body. With `path`, outline a file or directory plus its imports. | `symbol` or `path`, `include_body` |
| `search` | Fuzzy or glob name search with filters. `unused: true` lists functions nothing calls. | `pattern`, `kinds`, `languages`, `path`, `exported`, `unused` |
| `refs` | Every reference to a symbol with its confidence, or the files importing a file. | `target`, `tests_only`, `min_confidence` |
| `blast` | Callers ranked by impact, plus test files to run, for a symbol or a git diff. | `symbol` or `diff_from` (+`diff_to`), `depth` |
| `verify` | Does a symbol exist; does every call in a snippet resolve; did a change break a call site, an import, or a contract? | `symbol`, `snippet` + `language`, or `changed` / `staged` / `diff_from` |
| `context` | Body, callees, callers, tests, and siblings in one budgeted bundle. | `symbol`, `budget_tokens` |
| `path` | Shortest call chain from one symbol to another, across services through routes and gRPC calls. | `from`, `to`, `depth` |
| `map` | Contracts between services: routes, gRPC methods, topics, tables, with both sides; or what a service serves and calls. | `kind` (`route`, `rpc`, `topic`, `table`, `service`), `query` |
| `status` | Index root, freshness, counts, and setup hints. | none |

Symbols can be written `name`, `Class.method`, or `A::B`. References and
callers are resolved through scope, imports, and declared types, and each
carries a confidence: unmarked lines are exact, "likely" lines matched a
unique name, "possible" lines are ambiguous or name-level. See
[accuracy](accuracy.md).

## Checking conformance yourself

The repository's `tests/mcp_conformance.rs` drives the binary through full
stdio and HTTP sessions. To poke the server interactively:

```sh
npx @modelcontextprotocol/inspector tessera mcp
npx @modelcontextprotocol/inspector --cli tessera mcp --method tools/list
```
