# Roadmap

**Tessera's job:** your agent says done, Tessera checks. It gives a
deterministic blast radius before every edit and verification after it,
across files, repos, routes, topics, and tables. It runs locally, uses no
LLM, and is Apache-2.0.

Every milestone ships with proof: a test suite, a published number, or a
demo. Nothing is claimed until it can be reproduced. The full design is in
[docs/design/2026-10-21-overhaul.md](docs/design/2026-10-21-overhaul.md).

| | Milestone | Status |
| --- | --- | --- |
| v0.10 | Foundation | done, release pending |
| v0.11 | Precision + public accuracy benchmark | done, release pending |
| v0.12 | Edit-safety loop (hooks in every agent that supports them) | planned |
| v0.13 | Cross-repo contract map | planned |
| v1.0 | Change-impact UI, GitHub Action, stable contracts | planned |
| v1.x | Team & CI, architecture rules, precise mode, editors, runtime truth, data lineage, plugins, agent memory | planned |

## v0.10: Foundation

Make the basics trustworthy before claiming anything else.

- tree-sitter 0.26 with one extractor module per language. The walk is
  iterative, signatures are cut on char boundaries, and TS and TSX use their
  own grammars.
- Discovery honours `.gitignore` and skips minified and oversized files.
  The index works on repositories of any size (45k+ files).
- No setup step: the first query indexes, and later queries re-index only
  what changed. The MCP server watches files.
- Output is deterministic, paginated with totals and cursors, labels fuzzy
  matches, and carries measured token counts.
- Eight operations replace 21 overlapping commands: `find`, `search`,
  `refs`, `blast`, `verify`, `context`, `path`, `status`.
- A dual-era MCP server (2026-07-28 plus initialize-based clients) over stdio
  and Streamable HTTP.
- Every install channel verified after each release: npm (platform packages),
  Homebrew, `curl | sh` with checksums, cargo-binstall, and Docker.

**Gate:** every install channel is green on three operating systems, no
panics on adversarial input, byte-identical output across runs, and MCP
conformance tests pass.

## v0.11: Precision + `tessera-bench`

- Import- and scope-aware resolution for TypeScript, JavaScript, and Java:
  import bindings, a module resolver, file identity in symbol keys, Java FQNs
  and overloads by arity, and limited receiver-type inference.
- Record `new`, `extends`/`implements`, decorators and annotations, type
  references, and calls inside callbacks. Every edge carries a confidence and
  the resolver that produced it.
- `tessera-bench`: an open benchmark that scores reference and caller
  accuracy against SCIP ground truth on public repositories, for Tessera and
  for other code-graph tools, with every adapter and raw output published.

**Gate:** high-confidence precision ≥ 0.95 and recall ≥ 0.85 for TS/JS. The
milestone grows until that holds; the bar is not lowered.

**Result:** passed on the TypeScript/JavaScript corpus: precision 0.990,
recall 0.990 (direct calls), 0.912 (typed receivers), 0.986 (type
references). Per-repository numbers, including the one below the bar, are in
[docs/accuracy.md](docs/accuracy.md).

## v0.12: Edit-safety loop

- `blast --diff` covers uncommitted work. `verify` checks for invented APIs,
  arity mismatches, dangling references, and broken imports, with a
  confidence on each finding and SARIF output.
- Hooks for every agent with a hook system, starting with a Claude Code
  plugin:
  - Before an edit: note the callers of the symbol being changed.
  - After an edit: re-index and block on a high-confidence break.
  - At stop: refuse "done" while breaks remain.
- `tessera init` detects installed agents and shows a diff before writing any
  config. 24 agents are covered; see [docs/agents.md](docs/agents.md). A
  git pre-commit backstop covers every agent.
- A refactor eval: how many call sites agents miss, with and without Tessera.

## v0.13: Contract map

- Workspaces of many repositories, with auto-detected monorepos.
- Facts for routes, HTTP clients, topics, producers and consumers, tables,
  migrations, RPC, GraphQL, and config keys. Each link records its evidence
  and confidence.
- Framework packs:
  - Express, Nest, Next.js, and Fastify.
  - fetch, axios, React Query, and SWR clients.
  - Spring MVC, Feign, and Spring Kafka.
  - kafkajs.
  - Prisma, TypeORM, Sequelize, JPA, Flyway, OpenAPI, and `.proto`.
- `map` queries, `path` across services, and `verify` flags a contract change
  (a removed route, an orphaned consumer).

## v1.0: Visible and stable

- `tessera ui`, a local change-impact view: the diff, the affected symbols,
  the affected services. It can export a static site.
- A GitHub Action that comments blast radius and verify results on PRs.
- A docs site, translated READMEs, and CLI, MCP, and JSON contracts frozen for
  1.x.

## After 1.0

| Theme | What |
| --- | --- |
| Team & CI | `tessera affected` for test selection across languages, an index cache keyed by commit, SARIF in code scanning |
| Architecture rules | Layer, forbidden-call, and cycle rules enforced by `verify`, hooks, and CI |
| Precise mode | SCIP import for compiler-grade edges, SCIP export, and an optional `tessera lsp` |
| Editors | VS Code and JetBrains extensions over the local binary |
| Runtime truth | OpenTelemetry traces overlaid on the static graph |
| Data lineage | Who reads and writes which tables and columns; PII rules in `verify` |
| Extension platform | WASM extractor and framework-pack plugins; bring your own grammar |
| Agent memory | Notes tied to symbols that expire when the code changes |

**Every release also brings:**
- Deep-tier languages (Python, Ruby, Go, C#, Kotlin).
- New framework, infrastructure, and queue packs.
- New agent adapters.
- Benchmark rows for every addition.

## Contributing to the roadmap

Language requests, agent integrations, and framework packs each have an issue
template. A pack or language is a small, self-contained contribution; see
[CONTRIBUTING.md](CONTRIBUTING.md).
