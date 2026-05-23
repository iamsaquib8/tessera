# Tessera

**A local semantic code graph for AI coding agents — deterministic navigation without burning your context window.**

[![Crates.io](https://img.shields.io/crates/v/tessera-codegraph.svg)](https://crates.io/crates/tessera-codegraph)
[![Downloads](https://img.shields.io/crates/d/tessera-codegraph.svg)](https://crates.io/crates/tessera-codegraph)
[![CI](https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml/badge.svg)](https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](rust-toolchain.toml)

If Tessera saves your agents from grep spirals, **[give it a star](https://github.com/iamsaquib8/tessera)** — it helps others find it.

---

Agents should not need a whole context window to answer:

- Where is `findById` defined?
- Who calls it?
- What breaks if I change it?

Tessera indexes your repo into **SQLite** (Tree-sitter parsers, no cloud) and answers through a **CLI** and **MCP stdio server** — structured graph results with token estimates in every response.

## Why Tessera?

| Approach | Cost | Determinism |
| --- | --- | --- |
| Ripgrep + file reads | High token burn, easy to miss re-exports | Heuristic |
| Full LSP in the agent loop | Heavy setup, language-server quirks | Strong, but noisy |
| **Tessera graph queries** | Low — outlines and impact without bodies | **Explicit JSON + `_meta` token hints** |

Built for **Cursor, Claude Code, Codex**, and any MCP client that can spawn a stdio server.

## Install

```sh
cargo install tessera-codegraph
```

Binary name: `tessera`.

```sh
tessera index .
tessera find-definition findById
tessera impact findById
```

From source:

```sh
git clone https://github.com/iamsaquib8/tessera
cd tessera
cargo install --path .
```

One-liner (no Rust toolchain required on the machine if you already have `cargo`):

```sh
curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/scripts/install.sh | sh
```

## 30-second demo

```sh
tessera index examples/sample --db /tmp/tessera-demo.db
tessera impact findById --db /tmp/tessera-demo.db
```

Example impact output (truncated):

```json
{
  "symbol": "findById",
  "callers": [
    { "symbol": "getUser", "file": "examples/sample/users.ts", "criticality": 0.72 }
  ],
  "_meta": { "estimated_tokens": 180, "suggested_query": "expand_symbol findById" }
}
```

## Tools

| Command / MCP tool | What you get |
| --- | --- |
| `find_definition(symbol)` | File, line, kind, signature for definitions |
| `find_references(symbol)` | Call sites with one-line context |
| `get_outline(path)` | Semantic skeleton — no function bodies |
| `expand_symbol(symbol)` | Body + immediate dependencies |
| `impact(symbol)` | Transitive callers ranked by simple criticality |

Graph path defaults to `.tessera/tessera.db`. Override with `--db`.

## MCP (Cursor / Claude / etc.)

Index first, then run the server:

```sh
tessera index .
tessera mcp --db .tessera/tessera.db
```

**Cursor** — add to MCP settings (`~/.cursor/mcp.json` or project config):

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

Run `tessera index .` once per repo (or in a post-clone hook). Full schemas: [docs/mcp.md](docs/mcp.md).

## Language support

| Language | Extensions |
| --- | --- |
| TypeScript | `.ts`, `.tsx`, `.mts`, `.cts` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` |
| Python | `.py`, `.pyw` |

Skips `.git`, `node_modules`, `target`, `dist`, `.next`, `.venv`, `__pycache__`, and other common noise.

## Architecture

Rust CLI + MCP · Tree-sitter extraction · SQLite (WAL) · symbol / reference / call-edge tables.

Details: [docs/architecture.md](docs/architecture.md) · Quickstart: [docs/quickstart.md](docs/quickstart.md)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Status

**v0.1 — pre-alpha.** Solid for real TS/JS/Python repos; not full LSP parity. See [ROADMAP.md](ROADMAP.md).

## Contributing

PRs welcome — parsers, graph accuracy, and query quality have the highest leverage. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0 — see [LICENSE](LICENSE).
