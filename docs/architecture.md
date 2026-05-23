# Architecture

Tessera turns source code into a small local graph:

```text
source files
  -> tree-sitter parsers
  -> symbols, references, and edges
  -> SQLite
  -> CLI and MCP queries
```

## Storage

SQLite is the default store. WAL mode is enabled so the graph can support concurrent reads while agents query it.

Core tables:

- `files`: relative path, language, content hash, line count
- `symbols`: name, qualified name, kind, line range, signature
- `refs`: call/reference sites and one-line context
- `edges`: caller-to-callee relationships
- `meta`: repository root and other index metadata

## Parsing

Tree-sitter provides fast syntax trees without requiring a language server. The v0.1 parser extracts:

- top-level and nested functions
- classes
- methods
- function-like variable declarations
- call expressions

## Querying

Queries are deterministic SQL and graph traversals. They intentionally return compact structures instead of full files whenever possible.

`impact` starts with reverse call edges and walks callers transitively. Its current scoring is intentionally simple:

```text
criticality = fanout * 3 + export_bonus + test_bonus + depth_bonus
```

This gives agents a useful first ordering without pretending to be a complete risk model.
