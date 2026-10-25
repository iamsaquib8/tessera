# Changelog

All notable changes to Tessera are documented here.

## 1.1.0 - Unreleased

The contract map, completed: more frameworks, two new contract kinds, and
paths that cross repositories.

### Added

- **Express mounts:** `app.use("/api", users)` serves the routes on `users`
  under `/api`, whether the router is defined in the same file or required
  or imported from another.
- **Go, Python and Ruby web frameworks:** net/http (with Go 1.22 method
  patterns), chi, gin and echo; FastAPI, Flask and Django; Rails
  `routes.rb` with `resources`. Clients: Go `net/http`, Python `requests`
  and `httpx`, Ruby `Net::HTTP`, Faraday and HTTParty.
- **GraphQL** (`tessera map graphql`): root fields in schemas, Apollo and
  NestJS resolvers, and the operations that select them.
- **Config keys** (`tessera map config`): `.env` files, Dockerfiles,
  Compose and Spring properties set keys; code in every language reads
  them. Keys bind like Spring's relaxed binding; values are never stored.
- **`path` across repositories:** in a workspace, a path can run from a
  client in one repository into the handler of another; each hop through a
  contract names it (`via route GET /api/users/{}`).

## 1.0.1 - 2026-10-21

### Fixed

- The container image builds again: the Dockerfile now copies the
  framework packs the binary embeds. 1.0.0 shipped everywhere but GHCR.

## 1.0.0 - 2026-10-21

The first full release since 0.4.0. It ships everything in 0.10 to 0.13 below,
plus a way to see a change's impact, a pull-request report, and a contract
for what 1.x keeps stable.

### Added

- **`tessera ui`**: a local page on `127.0.0.1` with the change impact of
  the working tree (problems to fix, changed symbols, callers by impact,
  contracts with services on the other side, tests to run), the contract
  map of services and the routes, gRPC methods and topics between them
  (grouped by directory above 60 services) with every contract listed, a
  symbol page and search. Nothing loads from the network.
  `--export DIR` writes the same page as a static site.
- **`verify --format markdown`**: the pull-request report, one comment with
  blocking problems, advisory findings, contracts touched, callers of the
  changed code and tests to run, kept under GitHub's size limit.
- **GitHub Action** (`uses: iamsaquib8/tessera/action@v1`): installs the
  release binary, caches the index, and posts or updates one comment per
  pull request; forks fall back to the job summary. `fail-on: blocking`
  fails the job on high-confidence problems ([action/README.md](action/README.md)).
- **Stability** ([docs/stability.md](docs/stability.md)): commands and flags,
  exit codes, `--json` fields, MCP tool schemas, hook replies and check ids
  are stable for 1.x, pinned by `tests/stability.rs`.
- Translated READMEs (Simplified Chinese, Japanese, Korean, Spanish,
  Brazilian Portuguese), marked as drafts until a native speaker reviews
  them, and a pitch kit in `launch/pitch/`.

### Fixed (from the pre-release review)

- `verify` and the hooks no longer report breaks in code nobody touched:
  CommonJS `module.exports = …`, `export * as ns`, enum members, imports of
  JSON/CSS/SVG and generated files, plain-JavaScript argument counts, and
  calls on JavaScript globals. On Ghost, a comment-only edit used to raise 74
  blocking findings; it now raises none.
- Parameters and local variables shadow an import of the same name (schema
  v9 records them).
- A body-only edit to a file imported by thousands re-resolves that file
  alone; a signature change on such a file finishes the rest in the
  background instead of stalling the hook (25.6 s → 0.3 s on a 45k-file
  repository).
- Hooks block only on problems the agent introduced, never interrupt a
  rename fixed one call site at a time, answer once per event in the shape
  of the client that sent it, and bound Stop's time.
- `init` never writes TOML or YAML that no longer parses, re-plans a file
  that changed after the preview, and writes atomically.
- V5 no longer blocks on requests to external APIs, parameter-only matches,
  or routes moved under a mounted router.

### Changed

- V5 names a removed gRPC method after the `.proto` service that declares
  it when the server file registers more than one service
  (`ShippingService/GetQuote`, not `*/GetQuote`).
- The README lists every agent `init` sets up, with what each one gets; a
  test keeps the list in step with the registry.

## 0.13.0 - shipped in 1.0.0

The contract-map release. Tessera sees how services talk to each other.

### Added

- **`tessera map <route|rpc|topic|table|service> [query]`** and the MCP
  `map` tool: each HTTP route, gRPC method, topic, and table with where it
  is declared, who provides it, and who uses it, each site with its
  handler, service, and confidence ([docs/contracts.md](docs/contracts.md)).
- **Facts layer** (schema v8): contract facts extracted per file at index
  time and joined by key at query time. `.proto`, `.sql`, OpenAPI and
  Prisma files are indexed as data files.
- **Framework packs** as tree-sitter queries in `packs/`: Express, Fastify,
  NestJS, Next.js file routes, fetch/axios/ky/SWR, Spring MVC, Feign,
  RestTemplate, WebClient, MockMvc, Spring Kafka, kafkajs, NestJS
  microservices, JPA, TypeORM, Sequelize.
- **gRPC** servers and clients in Go, C#, Node, Python, Java and C++,
  matched against `.proto` methods.
- **`path` across services**: contracts are call edges, so a path can run
  from a client into another service's handler.
- **Workspaces**: `tessera.workspace.toml` lists repositories mapped
  together.
- **V5** in `verify` and the hooks: a client still uses a route, gRPC
  method, or topic the change removed.
- **tessera-bench** contract links: hand-labelled gRPC truth for
  microservices-demo and a held-out opentelemetry-demo.

### Changed

- MCP lists nine tools.

## 0.12.0 - shipped in 1.0.0

The edit-safety release. Tessera checks every edit an agent makes and holds
it at "done" until the breakage is fixed.

### Added

- **`verify --changed` / `--staged` / `--diff <rev>`** check a change: calls
  to names that do not exist where they point (V1), wrong argument counts
  (V2), references the change left dangling (V3), broken imports (V4), and
  the tests that reach the changed code (V6). Only high-confidence findings
  in TypeScript, JavaScript and Java block (exit 1). `--format agent`,
  `json`, and `sarif`.
- **`tessera hook <agent> <event>`**, one entry point for agent hooks: a
  note on cross-file callers before an edit, problems reported after it
  (blocking only when they survive an edit that fixed nothing), and a gate
  at the end of the turn (at most three blocks). Adapters for Claude Code,
  Codex CLI, Qwen Code, Gemini CLI, GitHub Copilot CLI, Cursor, Augment and
  Antigravity, each in its vendor's documented shape
  ([docs/hooks.md](docs/hooks.md)).
- **Claude Code plugin** in this repository:
  `/plugin marketplace add iamsaquib8/tessera`, then
  `/plugin install tessera@tessera`.
- **`tessera init`** detects the agents a project uses (21 in the registry),
  shows the diff of every config change, and asks. `--yes`, `--dry-run`,
  `--agent`, `--global`, `--with-instructions`, `--git-hook`. Merging keeps
  existing entries, key order and indentation; a rerun changes nothing.
- **`init --git-hook`** installs a pre-commit hook that refuses a commit
  breaking a call site or an import.
- Calls on a freshly constructed object (`new Repo().find()`) resolve on the
  class.
- **Refactor eval** in tessera-bench: refactoring tasks scored against
  ground-truth call sites, with and without Tessera.

### Changed

- `tessera init` replaces `--mcp-configs`, `--git-hooks` and `--force`.

### Fixed

- Missing-member findings only for classes (and Java interfaces), never for
  TypeScript interfaces, type aliases or variables; properties holding
  functions count as members.
- Comments in parameter and argument lists no longer count as parameters
  or arguments; an arrow assigned to a typed variable takes its arity from
  the type; tagged templates have no argument count.
- `export default <identifier>` no longer reports its own file as a missing
  module; default exports of other expressions and destructured exports
  are recognised.

## 0.11.0 - shipped in 1.0.0

The precision release. References in TypeScript, JavaScript, and Java are
resolved, not matched by name, and every one carries a confidence that is
measured against compiler ground truth.

### Added

- **Resolution.** A resolve pass after indexing turns references into edges
  through scope, imports (ESM, CommonJS, re-exports and barrels, tsconfig
  `paths`, workspace packages, Java packages and static imports), `this` and
  `super`, declared types (variables, parameters, fields, interface
  properties, field chains such as `this.deps.repo`), and class names. Java
  overloads are told apart by argument count; TypeScript overload
  signatures share one symbol whose arity spans them.
- **Confidence** on every reference: `high`, `medium` (unique name), `low`
  (two to five candidates, kept as one ambiguous group), or `name`.
  References that resolve outside the repository get no edge.
- `refs --min-confidence high|medium|low` (MCP `min_confidence`); each hit
  names the definition it resolved to.
- `new`, `extends`/`implements`, decorators and annotations, type
  references, and JSX member tags are recorded. Anonymous callbacks passed
  to calls (`it("…", () => …)`, `app.get("/x", (req, res) => …)`), class
  fields holding arrow functions, function-typed interface properties, and
  module-level constants are symbols, so their calls have a caller.
- `status` shows how many references resolved at each confidence.
- **tessera-bench** (separate repository): SCIP ground truth on pinned
  public repositories, precision per confidence tier, recall per category.
  First results: high-confidence precision 0.998 for TypeScript/JavaScript
  and 0.995 for Java ([docs/accuracy.md](docs/accuracy.md)).

### Changed

- `refs`, `blast`, `context`, and `search --unused` use resolved edges.
  Output names "name-level matching" only when a result contains it, and
  marks likely and possible matches.
- `search --unused` checks types in TypeScript, JavaScript, and Java through
  type references, and skips interface and abstract members.
- References are located at the called name, so a call on a continuation
  line points at that line.
- Indexing parses files on up to eight threads and fills the symbol search
  index per file; a 45,000-file repository indexes with resolution in 54 s.
- The index schema is v6; existing indexes rebuild automatically.

## 0.10.0 - shipped in 1.0.0

The foundation release. Tessera is repositioned as deterministic edit safety
for AI coding agents, and the basics are rebuilt so they can be trusted on
real repositories. Versions 0.5 to 0.9 never shipped as GitHub releases, npm,
or Homebrew packages (0.8.0 reached crates.io only); their changes are
included here.

### Breaking

- The CLI is now fourteen commands built on eight operations. The old command
  names are removed:

  | Before | Now |
  | --- | --- |
  | `find-definition`, `signature`, `expand-symbol`, `get-outline`, `imports` | `find` (`--body`, `--path`) |
  | `find-references`, `tests-for`, `imported-by` | `refs` (`--tests`; pass a file path for importers) |
  | `impact`, `diff-impact`, `siblings` | `blast` (`--diff`) |
  | `validate`, `validate-snippet` | `verify` (`--snippet`) |
  | `context-pack`, `edit-prep` | `context` |
  | `connect` | `path` |
  | `unused` | `search --unused` |
  | `stats`, `doctor` | `status` |
  | `mcp-http` | `mcp --http` |
  | `plan-query`, `snapshot`, `bench`, `shell` | removed |

- MCP tools are now `find`, `search`, `refs`, `blast`, `verify`, `context`,
  `path`, `status`. Results are compact text by default (`format: "json"` for
  JSON).
- `--db` defaults to `<root>/.tessera/tessera.db`, where the root is found by
  walking up to `.tessera/` or `.git`. Global flags are `--root`, `--db`,
  `--json`, and `--quiet`.
- Exit codes: `0` success, `1` verify found problems, `2` usage error, `3`
  index error.
- The library API is `Index::{find, search, refs, blast, path, context,
  verify, status, export}`.
- MSRV is Rust 1.90.

### Added

- **No setup step.** Queries index on first use and re-index changed files
  first. The MCP server indexes in the background and watches files.
- **Discovery.** `.gitignore`, `.ignore`, and `.tesseraignore` are honoured;
  minified and oversized files are skipped; `index --exclude <glob>` excludes
  are remembered; `[exclude].globs` and `[index].max_file_bytes` are new
  config keys.
- **Output.** Pagination with `total`, `truncated`, and `next_cursor`
  everywhere. Fuzzy matches are labelled. Byte-identical output across runs.
- **`refs <file>`** lists the files that import it.
- **`status`** reports freshness, skipped files, and any older `tessera`
  shadowing this one on `PATH`.
- **MCP 2026-07-28** (stateless, `server/discover`) alongside initialize-based
  clients; spec error codes; HTTP header validation; Origin check.
- **Distribution.**
  - Draft-then-publish releases with SHA256SUMS and build provenance.
  - Static musl Linux builds.
  - npm platform packages with no postinstall step.
  - A Homebrew formula rendered on each release.
  - A checksum-verifying `install.sh`.
  - cargo-binstall metadata.
  - Smoke tests on every channel.
- Golden extraction fixtures for all 11 languages, plus MCP conformance,
  robustness, and perf-budget suites.

### Fixed

- Indexing no longer crashes on repositories above ~32k files
  (`too many SQL variables`).
- Long non-ASCII lines no longer panic, and deeply nested files no longer
  overflow the stack.
- `.ts` files are parsed with the TypeScript grammar; angle-bracket casts no
  longer swallow the rest of the file.
- Invalid UTF-8 files are indexed lossily with a warning instead of skipped,
  and BOMs are stripped.
- Java packages named `build`, `out`, `target`, or `vendor` are indexed.
- Test-file detection no longer treats `test_cases_controller.rb` as a test,
  and does recognise `_spec.rb`, `FooIT.java`, and top-level `tests/`.
- MCP no longer replies to notifications, answers `ping`, and returns
  `-32601` and `-32602` instead of `-32000` for everything.
- Silent caps on references (250), impact callers (100), and imports (500)
  are gone.
- Concurrent writers no longer fail with "database is locked": writers take
  `<db>.lock`, and a query that finds another process indexing answers from
  the existing index.
- `--db` pointed at a SQLite file that is not a Tessera index is refused
  instead of having its tables dropped.
- The watcher and explicitly named paths follow the same ignore, hidden-name,
  and symlink rules as the full walk.
- The index rebuilds when the Tessera version, `config.toml`, or the excludes
  change; unreadable files and files with syntax errors are listed in
  `status`; `--exclude` survives `watch` and git hooks (`--reset-excludes`
  clears it).
- `blast --diff <rev>` compares against the working tree, works when the
  root is a subdirectory of the git repository, and rejects revisions that
  look like options.
- Symbol bodies are read lossily, without a BOM, from the current root (a
  moved repository still works), and never from outside it.
- `verify` no longer reports false problems on lines with non-ASCII text.
- Tessera refuses to auto-index the home directory or `/` without `--root`.
- `init` writes executable git hooks (also in worktrees), a valid Codex
  `[mcp_servers.tessera]` table, and escaped JSON; `.tessera/.gitignore`
  keeps `config.toml` committable.
- npm and cargo-binstall install the static musl build on every Linux, so
  older glibc distributions work. `install.sh` replaces the binary by rename.
  The Docker image keeps its index inside the container (`TESSERA_DB`).
- Release candidates are published as pre-releases (npm `next`, no Homebrew,
  no crates.io), every release job builds the tag, and the Docker image and
  install smoke tests run as part of the release.

### Removed

- The bloom filter, the unused memory-mapped snapshot, the placeholder Cozo
  engine, the built-in `bench` (replaced by `tessera-bench` in v0.11), and the
  made-up "alternative query" token and fidelity numbers.

## 0.9.0 - never published

v0.9 hardens the HTTP/SSE MCP path so Tessera can run as a shared local or CI
service, not only as a stdio process spawned by an agent.

### Added
- Metadata-rich `GET /health` for `tessera mcp-http`, including service
  version, DB path, indexed root, schema version, snapshot path, snapshot
  presence, and advertised endpoint paths.
- `GET /sse` readiness events now include the service name and Tessera version.
- End-to-end HTTP transport coverage that boots the compiled `tessera` binary,
  verifies `/health` and `/sse`, and runs an MCP `find_definition` query through
  `POST /mcp`.

## 0.8.0 - crates.io only

v0.8 makes exported graphs easier to inspect and share without leaving the CLI.

### Added
- `tessera export --group-by none|file|directory|language` for Mermaid subgraphs and DOT clusters.
- `tessera export --collapse-tests` and `--exported-only` graph filters.
- `tessera export --html-out graph.html` to write a self-contained Mermaid preview page with a copy button.
- MCP `export` arguments for grouping and graph filters.
- README, quickstart, and MCP docs for the v0.8 graph export workflow.

## 0.7.0 - never published

v0.7 adds the first agent workflow layer on top of the code graph: deterministic
tool planning and a one-call pre-edit bundle.

### Added
- **`plan-query`**: recommends an ordered, token-aware Tessera tool sequence from a natural-language task shape.
- **`edit-prep`**: combines `validate`, `signature`, `siblings`, `context-pack`, and `tests-for` into one pre-edit bundle.
- MCP tools `plan_query` and `edit_prep` with agent-routing descriptions.
- Shell completion entries, docs, and end-to-end tests for the v0.7 workflow commands.

## 0.6.0 - never published

v0.6 focuses on real-repo correctness: predictable indexing boundaries,
non-fatal parser/read failures, stronger ignored-path defaults, and regression
fixtures that snapshot the graph shape.

### Added
- `.tessera/config.toml` include/exclude support through `[include].paths`, `[exclude].paths`, and `[ignore].extra`.
- Non-fatal index warnings for unreadable files, traversal errors, and parser failures.
- Broader built-in ignored directories for generated, vendored, build, package-manager, virtualenv, and cache output.
- Deterministic fixture tests covering symbols, references, imports, edges, and exported flags.
- Configuration documentation at `docs/configuration.md`.

### Changed
- `tessera index` and `tessera watch` print warning summaries instead of aborting the whole indexing run for a bad source file.
- `tessera init` now writes include/exclude config stubs into `.tessera/config.toml`.

## 0.5.0 - never published

v0.5 hardens Tessera for daily use: keeping indexes fresh, diagnosing local
setup, reducing empty-result confusion, and documenting the first-run path.

### Added
- **`watch`**: polling daemon mode that runs the incremental indexer after source changes, with `--once`, `--poll-ms`, `--debounce-ms`, `--full`, and `--no-snapshot`.
- **`unused`**: zero-inbound-reference detection with kind, language, exported, path, limit, and JSON filters.
- **`doctor`**: local setup diagnostics for root path, DB existence, schema, index counts, snapshot freshness, parser smoke tests, ignored paths, and MCP command hints.
- **`init`**: project-local `.tessera/config.toml`, optional MCP snippets, optional git hooks, and next-step prompts.
- **`completions`**: dependency-free shell completion output for bash, zsh, fish, and PowerShell.
- **`mcp-http`**: local HTTP transport with `POST /mcp`, `GET /sse`, and `GET /health`.
- **`--explain` / `--why`** on `impact`, `validate`, `search`, and `unused`.
- Stable v0.5 JSON response schema snapshot at `docs/json-schemas/tessera-cli-v0.5.schema.json`.
- New docs: first five minutes, when not to use Tessera, and troubleshooting.

### Changed
- Read-only query paths now fail with an actionable missing-DB error instead of creating an empty DB.
- Empty human-readable results now include next-command hints.
- Architecture, quickstart, integration, and README docs now reflect the v0.5 command set.

## 0.4.0 - 2026-06-14

The biggest release since launch: five new languages, two new graph queries, a
zero-install agent skill, and install paths for everyone who isn't a Rust dev.

### Added
- **Five new languages**, each with real AST-level extraction (definitions, references, imports) — bringing the total to **11**: **C**, **C++**, **C#**, **Ruby**, **PHP**. C++ also covers `.h` headers (the grammar is a C superset). All exercised end-to-end (index → impact) in the test suite.
- **`connect <from> <to>`** (CLI + MCP + library) — the shortest call path between two symbols. Deterministic BFS over the call graph; returns the ordered chain or reports no path.
- **`export [--from SYM] [--format dot|mermaid]`** (CLI + MCP + library) — render the call graph as Graphviz DOT or Mermaid. Whole graph, or the precise forward subgraph rooted at a symbol. Paste the Mermaid into a PR or a Markdown doc.
- **Drop-in `/tessera` Agent Skill** (`skills/tessera/`) — teaches Claude Code (and other skill-aware agents) to use Tessera for navigation, installing the binary on first use. No Rust toolchain required.
- **Install everywhere**: `npm i -g tessera-codegraph` / `npx`, `brew install`, `curl | sh`, Docker (`ghcr.io/iamsaquib8/tessera`), plus prebuilt binaries for macOS (arm64/x64), Linux (x64/arm64), and Windows attached to each release.
- **Branding + visual hero**: logo, terminal demo, and a social-preview card (`assets/`).

### Changed
- Callee resolution for `connect`/`export` resolves each call to a single best symbol, **preferring a same-file definition** and matching case-sensitively — so a common name like `loadUser` no longer fans out across every file/language that happens to share it.
- CI now runs the test suite on **Linux, macOS, and Windows**; the release pipeline builds cross-platform binaries and publishes the npm wrapper and Docker image.
- `validate-snippet` / the `validate_snippet` MCP tool accept the five new languages.

## 0.3.1 - 2026-05-24

### Fixed
- CI release workflow on stable Rust 1.95 failed v0.3.0 because two new clippy lints (`manual_checked_ops`, `unnecessary_min_or_max`) were promoted to default-warn. Switched to `checked_div` in `src/bench.rs` and dropped a redundant `.max(0)` on a `usize` in `src/query.rs`. Local toolchain bumped to 1.95.0 so CI drift surfaces locally first.

## 0.3.0 - 2026-05-24

Six new token-saver tools, all in one release. Each replaces a multi-tool-call workflow with a single bounded response — the biggest single drop in agent token usage since v0.2.

### Added
- **`context_pack(symbol, budget?)`** — bundle a symbol's body + immediate-dep signatures + top caller signatures + relevant tests into one token-budgeted response. Replaces the 3-5 round trips an agent makes to "understand" a symbol before editing it.
- **`diff_impact(from, to?, depth?)`** — shell out to `git diff -U0`, map changed hunks to symbols, run PageRank impact, aggregate. Single tool call answers "what does this PR break?".
- **`imports(path)`** — list imports/uses/requires declared in a file or directory.
- **`imported_by(source)`** — inverse: list files that import a given module / source path.
- **`signature(symbol)`** — ultra-cheap signature lookup. For class/struct/interface/trait/enum/record/impl, also returns child member signatures (no bodies).
- **`siblings(symbol)`** — symbols that share callers with the target, ranked by overlap count. Novel signal: find the cluster of related abstractions to refactor together.

### Changed
- Schema bumped to **version 3**: new `imports` table populated during indexing. JS/TS/TSX extract both ES6 `import_statement` *and* CommonJS `require('./foo')` / dynamic `import('./foo')`. Python (`import_statement`/`import_from_statement`), Go (`import_spec`), Rust (`use_declaration`), and Java (`import_declaration`) also extract imports.
- Existing DBs are migrated automatically on next open — the new `imports` table is added in `migrate()`. Running `tessera index .` (incremental) backfills imports for changed files; `--full` backfills the whole repo.

## 0.2.2 - 2026-05-24

### Added
- New `search` tool (CLI + MCP + library): fuzzy + `*`-glob search across indexed symbols, filterable by `--kind`, `--language`, `--exported`, and `--path` prefix. Closes the symbol-name "spiral find" loop without needing embeddings.
  ```sh
  tessera search '*Repository*' --kind class,interface --language java
  tessera search parseFrom
  tessera search 'init*' --kind method --exported
  ```

## 0.2.1 - 2026-05-24

### Fixed
- `cargo install tessera-codegraph` (without `--locked`) failed with a `tree_sitter::Language` type mismatch. `tree-sitter-rust 0.20.3` declares its `tree-sitter` dep as `>= 0.20` (no upper bound), so fresh resolution pulled `tree-sitter 0.26` in parallel with the 0.20 line our other grammars use. Tessera now pins every `tree-sitter*` crate to `>=0.20, <0.21` so the resolver can't escape the 0.20.x window.

## 0.2.0 - 2026-05-24

Substantial upgrade. v0.2 closes Month-1 gaps from the execution plan and ships the Month-2 deliverables, then goes further with a Datalog backend, memory-mapped snapshot, Java support, and JSX/React component awareness.

### Added
- Java extractor (Tree-sitter): classes, interfaces, records, enums, methods, constructors. Method invocations and `new` expressions register as references.
- TSX/React: `.tsx` parsed with the TSX grammar (a superset of TS), so `<Component />` and `<Foo.Bar />` JSX elements index as references of kind `jsx`. Lowercase intrinsic elements (`<div>`) are skipped.
- Go and Rust extractors (Tree-sitter), bringing supported languages to six total (TS/TSX, JS, Java, Python, Go, Rust).
- Personalised PageRank scoring for `impact`, with a per-caller `breakdown` (`pagerank`, `fanout_in`, `fanout_out`, `exported`, `test_coverage`, `depth_decay`). Criticality is now normalised to 0–100.
- Hallucination validator: `tessera validate <symbol>` and `tessera validate-snippet --language ...`. Bloom filter front-door + trigram FTS + Jaro-Winkler ranking for near-miss suggestions.
- Incremental re-index: `tessera index` now defaults to a sha-diff path that reuses unchanged files and only re-parses what changed. `--full` keeps the old behaviour.
- Memory-mapped graph snapshot (`.tessera/snapshot.bin`) built automatically at index time; rebuilt explicitly with `tessera snapshot`.
- New tools (CLI + MCP): `validate`, `validate_snippet`, `stats`, `tests_for`.
- `tessera bench` harness — generates the perf chart published in the README, against a real or synthetic repo.
- `src/lib.rs` and the `tessera_codegraph::Index` façade — Tessera is now a library you can embed.
- Optional Cozo (Datalog) graph engine behind the `cozo` Cargo feature and `--graph-engine cozo` CLI flag.
- Trigram FTS5 index on symbol names for fast fuzzy lookup.

### Changed
- `ImpactCaller.criticality` is now `f32` (0–100) and ships with a `breakdown` object. **Breaking** for v0.1.0 consumers; v0.1.0 was pre-alpha.
- `tessera index` runs inside a single SQLite transaction for substantially faster batch inserts and consistent visibility.
- MCP server opens one DB connection at start-up instead of one per request.
- `find_definition` falls back to fuzzy candidates when exact lookup misses.

### Notes
- The Cozo backend is experimental — it requires `--features cozo` at build time. Default builds are unaffected.

## 0.1.0 - 2026-05-23

Initial pre-alpha release.

- Added `tessera index` for local SQLite graph creation.
- Added Tree-sitter extraction for TypeScript, JavaScript, and Python.
- Added CLI query commands for definitions, references, outlines, expansion, and impact.
- Added MCP stdio server exposing the five core tools.
- Added token estimate metadata to query responses.
- Added CI, tests, docs, and crates.io package metadata.
