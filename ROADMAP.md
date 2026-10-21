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
| v0.10 | Foundation | done, ships in 1.0.0 |
| v0.11 | Precision + public accuracy benchmark | done, ships in 1.0.0 |
| v0.12 | Edit-safety loop (hooks in every agent that supports them) | done, ships in 1.0.0 |
| v0.13 | Cross-repo contract map | done, ships in 1.0.0 |
| v1.0 | Change-impact UI, GitHub Action, stable contracts | done, release pending |
| v1.1 | Contract map, completed: mount prefixes, GraphQL, config keys, `path` across repositories | next |
| v1.2 | Team & CI: test selection, index cache, code scanning | planned |
| v1.x | Architecture rules, precise mode, editors, runtime truth, data lineage, plugins, agent memory | planned |

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

**Result:** passed for TypeScript/JavaScript (precision 0.998; recall 0.990
direct calls, 0.912 typed receivers, 0.986 type references) and for Java
(precision 0.995; recall 0.872, 0.935, 1.000). Per-repository numbers,
including those below the bar, are in [docs/accuracy.md](docs/accuracy.md).

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
  config. 21 agents are in the registry; see [docs/agents.md](docs/agents.md).
  A git pre-commit backstop covers every agent.
- A refactor eval: how many call sites agents miss, with and without Tessera.

**Status:** built. Hooks ship for Claude Code (plugin or `init`), Codex CLI,
Qwen Code, Gemini CLI, Copilot CLI, Cursor, Augment and Antigravity
([docs/hooks.md](docs/hooks.md)). A blind find-and-replace over the corpus
was used to hunt verify false positives; after the fixes it leaves 15
blocking findings, all real (tessera-bench `results/eval-replace-check.jsonl`).
The agent arms of the refactor eval are ready to run; the gate they measure
(missed call sites, false-positive blocks) is not met until they do.

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

**Status:** built ([docs/contracts.md](docs/contracts.md)). gRPC in Go, C#,
Node, Python, Java and C++; HTTP in Express, Fastify, NestJS, Next.js,
Spring and their clients; Spring Kafka, kafkajs and NestJS microservices;
SQL migrations, JPA, TypeORM, Sequelize and Prisma; workspaces; V5. Hand-
labelled gRPC links: no wrong link on microservices-demo or the held-out
opentelemetry-demo, whose first measurement found 10 of 12 servers and 16
of 26 clients (gaps since closed). Still to come: Express mount prefixes,
GraphQL, config keys, `path` across repositories, and a third untouched
corpus.

## v1.0: Visible and stable

- `tessera ui`, a local change-impact view: the diff, the affected symbols,
  the affected services. It can export a static site.
- A GitHub Action that comments blast radius and verify results on PRs.
- A docs site, translated READMEs, and CLI, MCP, and JSON contracts frozen for
  1.x.

**Status:** built. `tessera ui` serves change impact, the contract map, a
symbol page and search from `127.0.0.1`, and `--export` writes a static
site. Checked on microservices-demo and on a 45,000-file repository (map
in about a second, 31 services). `verify --format markdown` is the
pull-request report, and the Action posts it as one comment, falling back to
the job summary on forks; it runs on this repository's own pull requests.
[docs/stability.md](docs/stability.md) lists what 1.x keeps, and
`tests/stability.rs` pins it. Translated READMEs are drafts until reviewed.

## v1.1: The contract map, completed

What v0.13 left open, each with a fixture and a benchmark row:

- Express routers mounted under a prefix (`app.use("/api", router)`).
- GraphQL schemas, resolvers and client operations.
- Config keys: who sets an environment variable or feature flag, and who
  reads it.
- `path` across repositories in a workspace.
- Go (`net/http`, chi, gin, echo), Python (FastAPI, Flask, Django) and Ruby
  (Rails) routes and clients.
- HTTP and topic ground truth in tessera-bench, and a third contract corpus
  nobody has tuned against.

**Gate:** published route and topic precision on two corpora, no wrong link
on the untouched one.

## v1.2: Team and CI

- `tessera affected <rev>`: the tests a change reaches, as a list a CI job
  can run, for every language.
- An index cache keyed by commit, shared between CI runs and teammates.
- SARIF uploaded to code scanning by the Action, so findings show inline.
- Exit-code and annotation modes for GitLab, Buildkite and Jenkins.

**Gate:** on three public monorepos, `affected` selects every test that
fails after a seeded break, and skips at least half of the suite on a
typical change.

## Later in 1.x

| Theme | What |
| --- | --- |
| Architecture rules | Layer, forbidden-call, and cycle rules enforced by `verify`, hooks, and CI |
| Precise mode | SCIP import for compiler-grade edges, SCIP export, and an optional `tessera lsp` |
| Editors | VS Code and JetBrains extensions over the local binary |
| Runtime truth | OpenTelemetry traces overlaid on the static graph |
| Data lineage | Who reads and writes which tables and columns; PII rules in `verify` |
| Extension platform | WASM extractor and framework-pack plugins; bring your own grammar |
| Agent memory | Notes tied to symbols that expire when the code changes |
| Refactor plans | `plan rename`, `plan move`, `plan signature`: every site an agent must edit for a change, in order, then `verify` checks the result |
| Ownership | CODEOWNERS and history on impact: who owns the callers and services a change reaches |

**Every release also brings:**
- Deep-tier languages (Python, Ruby, Go, C#, Kotlin).
- New framework, infrastructure, and queue packs.
- New agent adapters.
- Benchmark rows for every addition.

## Contributing to the roadmap

Language requests, agent integrations, and framework packs each have an issue
template. A pack or language is a small, self-contained contribution; see
[CONTRIBUTING.md](CONTRIBUTING.md).
