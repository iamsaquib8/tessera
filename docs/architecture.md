# Architecture

Tessera turns source code into a small local graph plus a memory-mapped projection:

```text
source files
  -> tree-sitter parsers (TS/JS/Python/Go/Rust)
  -> symbols, references, edges, exports
  -> SQLite (WAL, trigram FTS, bloom filter blob)
  -> memory-mapped snapshot (.tessera/snapshot.bin)
  -> CLI, MCP server, library API
```

## Storage

SQLite is the default store. WAL mode is enabled so the graph can support concurrent reads while agents query it. A bincode-serialised snapshot lives next to the DB and is mmap'd by the MCP server for hot-path queries.

Core tables:

- `files`: relative path, language, content hash, line count
- `symbols`: name, qualified name, kind, line range, signature, exported flag
- `refs`: call/reference sites and one-line context
- `edges`: caller-to-callee relationships with weights
- `symbols_fts`: FTS5 virtual table with the `trigram` tokenizer for fuzzy lookup
- `meta` / `meta_blob`: index metadata + serialized Bloom filter

## Parsing

Tree-sitter provides fast syntax trees without requiring a language server. The v0.2 parser extracts:

- Top-level and nested functions, methods, classes (TS/JS, Python)
- Function-like variable declarations (TS/JS) — including React function components
- JSX elements (`<Component />`, `<Foo.Bar />`) as references of kind `jsx`. The TSX grammar is used for both `.ts` and `.tsx` files; lowercase intrinsic elements (`<div>`) are skipped.
- Functions, methods, structs, interfaces (Go), with Go method receivers qualified by their receiver type
- Functions, methods, structs, enums, traits, modules, and `impl` blocks (Rust)
- Classes, interfaces, records, enums, methods, constructors, method invocations, and `new` expressions (Java)
- Call expressions and Rust macro invocations

Exported detection is language-aware:

- JS/TS/TSX: ancestor has an `export_statement`
- Python: all top-level names treated as exported
- Go: identifier starts with an uppercase ASCII letter
- Rust: declaration node has a `visibility_modifier` child starting with `pub`
- Java: declaration carries a `public` modifier child (package-private = not exported)

## Incremental indexing

`tessera index` defaults to an incremental path:

1. Walk the repo, hashing each file with SHA-256.
2. For each file, look up the existing row in `files`. If the sha matches, skip.
3. If it differs, `DELETE FROM files WHERE id = ?` (cascades to `symbols`, `refs`, `edges`, FTS). Re-parse and re-insert.
4. After the walk, drop any `files` row whose id was never visited.

All inserts happen inside one transaction. `--full` keeps the previous "blow away and rebuild" behaviour.

## Snapshot

After indexing, Tessera writes `.tessera/snapshot.bin`: a bincode archive containing every symbol, the call adjacency list, and a name-to-id reverse index. The MCP server `mmap`s this file at startup so `find_definition`, `find_references`, and `impact` can avoid SQLite on the hot path. SQLite remains the source of truth — the snapshot is a derived view that can be rebuilt at any time via `tessera snapshot`.

## Querying

Queries are deterministic SQL and graph traversals. They intentionally return compact structures instead of full files whenever possible.

`impact` runs a personalised PageRank on the reverse call graph:

```text
walk graph     : callee -> caller (reverse of call direction)
teleport vector: 1.0 at the symbol being changed, 0 elsewhere
update         : rank_new[v] = (1 - α) * teleport[v]
                              + α * Σ_{u -> v} rank_old[u] / out_deg(u)
α              : 0.85
iterations     : 25
```

Each caller's normalised PageRank lands in `criticality` (0–100). The `breakdown` object exposes the components an agent or human might want to audit (`fanout_in`, `fanout_out`, `exported`, `test_coverage`, `depth_decay`, raw `pagerank`).

## Hallucination validator

`validate` walks two layers:

1. **Bloom filter** in `meta_blob.bloom_symbols`. Built at index time over every `name` and `qualified_name`. A miss returns early — the symbol is definitely not in the graph.
2. **Trigram FTS5** for cheap fuzzy candidate lookup, re-ranked by Jaro-Winkler distance against the user's query.

`validate_snippet` parses an agent-supplied code blob with the same Tree-sitter pipeline that powers indexing, extracts every call/macro target, and runs the same two-layer check per call.

## Graph engine

The default backend is SQLite + petgraph-style adjacency lists. A feature-gated `cozo` backend mirrors call edges into an embedded Datalog database so impact becomes a recursive Datalog rule. Both engines implement the `GraphEngine` trait in `src/engine.rs`.
