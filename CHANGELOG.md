# Changelog

All notable changes to Tessera will be documented here.

## 0.8.0 - Unreleased

v0.8 makes exported graphs easier to inspect and share without leaving the CLI.

### Added
- `tessera export --group-by none|file|directory|language` for Mermaid subgraphs and DOT clusters.
- `tessera export --collapse-tests` and `--exported-only` graph filters.
- `tessera export --html-out graph.html` to write a self-contained Mermaid preview page with a copy button.
- MCP `export` arguments for grouping and graph filters.
- README, quickstart, and MCP docs for the v0.8 graph export workflow.

## 0.7.0 - Unreleased

v0.7 adds the first agent workflow layer on top of the code graph: deterministic
tool planning and a one-call pre-edit bundle.

### Added
- **`plan-query`**: recommends an ordered, token-aware Tessera tool sequence from a natural-language task shape.
- **`edit-prep`**: combines `validate`, `signature`, `siblings`, `context-pack`, and `tests-for` into one pre-edit bundle.
- MCP tools `plan_query` and `edit_prep` with agent-routing descriptions.
- Shell completion entries, docs, and end-to-end tests for the v0.7 workflow commands.

## 0.6.0 - Unreleased

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

## 0.5.0 - Unreleased

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
