# Tessera

> **A local semantic code graph for AI coding agents.**
> Stop burning tokens on `grep` + file reads. Answer "where is this?", "who calls it?", and "did the model hallucinate this symbol?" deterministically — over a Tree-sitter graph indexed in seconds, queried in milliseconds.

[![Crates.io](https://img.shields.io/crates/v/tessera-codegraph.svg)](https://crates.io/crates/tessera-codegraph)
[![Downloads](https://img.shields.io/crates/d/tessera-codegraph.svg)](https://crates.io/crates/tessera-codegraph)
[![CI](https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml/badge.svg)](https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](rust-toolchain.toml)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%9D%A4-ff69b4.svg)](https://github.com/sponsors/iamsaquib8)

⭐ **Star this repo** if it saves your agent from a grep spiral — it helps others find it.
💚 **[Sponsor on GitHub](https://github.com/sponsors/iamsaquib8)** if Tessera saves you tokens at work.

---

## Measured on real production repos

`tessera bench --path .` runs against any repo and prints the chart below. The harness ships in the binary — every number here is reproducible.

### 951-file Java service (browserstack/observability-pipeline)

```
Tessera v0.3.1 bench
─────────────────────
  951 files · 16,368 symbols · 129,959 references

Index time
  full         ████████████████████████████████    2,981 ms
  incremental  █                                      40 ms   ·  75× faster

"who calls parseFrom?"
  raw grep + read   ████████████████████████████████   394,140 tokens
  tessera           █                                    6,530 tokens   ·  60× cheaper

Per-query latency  ·  median of 3 runs
  find_definition      1 ms     ~ 1,781 tokens
  find_references      8 ms     ~16,144 tokens
  impact             371 ms     ~ 6,530 tokens
  validate             1 ms     ~    48 tokens
```

### 1,063-file Node.js service (browserstack/testhub, CommonJS)

```
Tessera v0.3.1 bench
─────────────────────
  1,063 files · 3,067 symbols · 142,337 references

Index time
  full         ████████████████████████████████    1,557 ms
  incremental  █                                      38 ms   ·  41× faster

"who calls BaseWorker?"
  raw grep + read   ████████████████████████████████    36,311 tokens
  tessera           █                                       41 tokens   ·  886× cheaper

"where is BaseWorker defined?"
  raw grep + read   ████████████████████████████████     1,790 tokens
  tessera           ██                                      90 tokens   ·  20× cheaper

Per-query latency  ·  median of 3 runs
  find_definition      0 ms     ~    90 tokens
  find_references      8 ms     ~    33 tokens
  impact               1 ms     ~    41 tokens
  validate             0 ms     ~    49 tokens
```

### Headlines

- **60–900× fewer tokens** to answer "who calls this?" — the work your agent spends most of its context window on.
- **38–40 ms incremental re-index** on near-million-LOC repos — fast enough to run on every file save.
- **Sub-20 ms** for definition / reference / validation queries.
- **CommonJS-aware**: `require('./foo')` is indexed alongside ES6 `import`, so `imports` / `imported_by` work on legacy Node code too.

## Install

```sh
cargo install tessera-codegraph
```

## Five commands to start

```sh
tessera index .                          # index your repo into .tessera/tessera.db
tessera context-pack findById            # body + deps + callers + tests in one budgeted bundle
tessera diff-impact origin/main          # "what does this branch break?" — changed symbols + PageRank-impacted callers
tessera search '*Repository*' --kind class --language java  # kill the grep spiral
tessera validate findByIdd               # "did the model hallucinate this?" — yes; meant findById (0.98)
```

That's it. The graph is local, the queries are deterministic, every response carries `_meta` token estimates so agents can plan their context budget.

## How it compares

| | Tessera | `aider`'s repomap | Sourcegraph | Cursor's index |
| --- | --- | --- | --- | --- |
| Local-only, no cloud | ✅ | ✅ | ❌ (enterprise) | ❌ |
| MCP server | ✅ | ❌ | ❌ | ❌ |
| Personalised PageRank impact | ✅ | ✅ (non-personalised) | ❌ | ❌ |
| Hallucination validator | ✅ | ❌ | ❌ | ❌ |
| Incremental re-index in ms | ✅ | partial | ❌ | proprietary |
| TypeScript / TSX with JSX refs | ✅ | partial | ✅ | ✅ |
| Java / Go / Rust / Python | ✅ | mixed | ✅ | ✅ |
| Token estimates in every response | ✅ | ❌ | ❌ | ❌ |
| Open source (Apache-2.0) | ✅ | ✅ | core | ❌ |

## What makes it different

- **Personalised PageRank impact.** Not just "who calls X" — *who calls X **that matters***. The random surfer teleports back to your edit site, so transitively reachable hubs float to the top with auditable breakdowns (`pagerank`, `fanout_in`, `fanout_out`, `exported`, `test_coverage`, `depth_decay`).

- **Hallucination validator.** Bloom-filter-fronted symbol existence check + a snippet validator that parses LLM output with the same Tree-sitter pipeline that built the graph. Every call is verified; near-misses come back with Jaro-Winkler confidence scores.

  ```sh
  echo 'findByIdd(1)' | tessera validate-snippet --language typescript
  ```
  ```
  ✗ findByIdd at line 1 col 1
        -> maybe findById (0.98)
        -> maybe find_by_id (0.85)
  ```

- **Incremental everywhere.** Re-running `tessera index .` only re-parses files whose SHA changed. 951-file Java repo: full index 4.6 s, incremental rerun **64 ms**.

- **Memory-mapped graph snapshot.** MCP server `mmap`s a `bincode` archive of the symbol + edge tables at startup. Hot-path queries don't touch SQLite.

- **React-aware.** `.tsx` parsed with the TSX grammar. `<UserCard />` registers a reference to `UserCard` of kind `jsx`, so React component graphs work the same as call graphs.

- **Token-priced operations.** Every response carries `_meta` with token estimates plus cheaper alternative queries. Agents can route to the right fidelity-to-token tradeoff.

## A 30-second demo

```sh
tessera index examples/sample
tessera impact findById --json | jq '.callers[0] | {symbol: .symbol.qualified_name, criticality, breakdown}'
```
```json
{
  "symbol": "renderUser",
  "criticality": 100.0,
  "breakdown": {
    "pagerank": 0.4674,
    "fanout_in": 0,
    "fanout_out": 1,
    "exported": true,
    "test_coverage": 0,
    "depth_decay": 1.0
  }
}
```

## Wire it up to your coding agent

Tessera speaks **MCP**. Index your repo, point your agent at the binary.

**Claude Code:**

```sh
claude mcp add tessera tessera -- mcp --db .tessera/tessera.db
```

**Cursor** — add to `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` (per-project):

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

Configs for **Cline, Continue.dev, Codex CLI, Zed, Aider, and custom GPTs** live in [docs/integrations.md](docs/integrations.md). Tool schemas in [docs/mcp.md](docs/mcp.md).

**Exposed tools:** `find_definition` · `find_references` · `get_outline` · `expand_symbol` · `impact` · `context_pack` · `diff_impact` · `imports` · `imported_by` · `signature` · `siblings` · `search` · `validate` · `validate_snippet` · `tests_for` · `stats`.

**Tip:** add `tessera index .` to a git `post-merge` hook so the graph stays fresh on every pull (incremental re-index is 38–66 ms on real repos).

## Use as a Rust library

```toml
[dependencies]
tessera-codegraph = "0.2"
```

```rust
use tessera_codegraph::{Index, IndexOptions, Language};

let report = Index::build("./repo", "./repo/.tessera/tessera.db", IndexOptions::default())?;
let idx = Index::open("./repo/.tessera/tessera.db")?;

for caller in idx.impact("findById", 4)?.callers.iter().take(5) {
    println!("{:5.1}  {}", caller.criticality, caller.symbol.qualified_name);
}

let check = idx.validate_snippet("findByIdd(1)", Language::TypeScript)?;
println!("{} unresolved calls", check.unresolved_calls);
```

## Languages

| Language | Extensions | Notes |
| --- | --- | --- |
| TypeScript | `.ts`, `.mts`, `.cts` | Parsed with the TSX grammar (a superset of TS) |
| TSX (React) | `.tsx` | `<Component />` and `<Foo.Bar />` register as references of kind `jsx` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` | JSX-aware |
| **Java** | `.java` | Classes, interfaces, records, enums, methods, constructors, method invocations, `new` expressions |
| Python | `.py`, `.pyw` | Functions, classes |
| Go | `.go` | Functions, methods (receiver-qualified), structs, interfaces |
| Rust | `.rs` | Functions, methods, structs, enums, traits, modules, macro invocations |

Skips `.git`, `node_modules`, `target`, `dist`, `.next`, `.venv`, `__pycache__`, and other common noise.

## Reproduce the bench

```sh
tessera bench --path /path/to/your/repo
tessera bench --scale 200                # synthetic 200-file TS repo, no arguments
tessera bench --out docs/benchmarks.md   # write the chart to disk
```

The synthetic repo (`tessera bench` with no `--path`) models a "popular utility" topology: a `sharedHelper` called from every module file, mirroring how high-impact refactors really cascade through a codebase. See [docs/benchmarks.md](docs/benchmarks.md) for methodology.

## Architecture

Rust core · Tree-sitter (7 grammars) · SQLite (WAL, FTS5 trigram, Bloom) · memory-mapped snapshot · personalised PageRank impact · MCP stdio.

```text
source files
  ─► tree-sitter parsers (ts/tsx/js, java, py, go, rust)
  ─► symbols, references, edges, exports
  ─► SQLite (WAL, FTS5 trigram, bloom blob)
  ─► memory-mapped snapshot (.tessera/snapshot.bin)
  ─► CLI · MCP server · library API
```

Details: [docs/architecture.md](docs/architecture.md) · Quickstart: [docs/quickstart.md](docs/quickstart.md) · Benchmarks: [docs/benchmarks.md](docs/benchmarks.md)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Status

**v0.2 — pre-alpha.** Seven languages, JSX-aware React component references, incremental indexing, PageRank-ranked impact, hallucination validator, library + MCP + CLI. See [ROADMAP.md](ROADMAP.md).

## Contributing

PRs welcome — parsers, graph accuracy, and query quality have the highest leverage. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Sponsor

Tessera is Apache-2.0 and built in public. If it saves you or your team tokens, **[sponsor on GitHub](https://github.com/sponsors/iamsaquib8)** to keep new languages, queries, and benchmarks shipping. Sponsors get early access to v0.4 features (runtime trace fusion, ADR memory, semantic git) and a say in the roadmap.

## License

Apache-2.0 — see [LICENSE](LICENSE).
