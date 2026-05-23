# Changelog

All notable changes to Tessera will be documented here.

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
