# Architecture

```text
 files on disk
   │  walk.rs        gitignore-aware discovery, minified/size skips, sorted output
   ▼
 extract/        tree-sitter 0.26, one module per language behind LanguageSupport,
   │             iterative walk → symbols, references, imports
   ▼
 db.rs           SQLite (WAL, FTS5 trigram), schema in PRAGMA user_version;
   │             a version mismatch rebuilds the cache
   ▼
 fresh.rs        stat-walk freshness check; index_files re-indexes only changed paths
   ▼
 query/          eight operations: find search refs blast path context verify status
   │             every list paginated, sorted by a stable key, measured in tokens
   ├─▶ main.rs   CLI (clap): global --root --db --json --quiet; exit codes 0/1/2/3
   ├─▶ mcp/      dual-era MCP: stdio + Streamable HTTP, background index, file watcher
   └─▶ lib.rs    Index, the Rust library API
```

## Indexing

1. `walk::discover` lists source files using the `ignore` crate. It follows
   `.gitignore`, `.ignore` and `.tesseraignore`, never follows symlinks, and
   returns paths with forward slashes, sorted.
2. Each file is read (invalid UTF-8 is decoded lossily with a warning, and a
   BOM is stripped). Its SHA-256 is compared with the stored one, so unchanged
   files are skipped.
3. `extract::parse_file` runs the language's tree-sitter grammar and walks the
   tree with a cursor. This is not recursive, so deeply nested input cannot
   overflow the stack. Each language module turns nodes into symbols (with
   qualified names from the enclosing scope), references (calls, `new`, JSX),
   and imports.
4. `db` writes everything in one transaction with cached prepared statements.
   Caller edges use the symbol ids returned for the same file. Stale rows are
   deleted through a temp table, so repositories with any number of files work.

## Freshness

`fresh::check` compares each discovered file's size and mtime with the stored
values, including files that were skipped. This takes one stat walk with no
file reads: under 100 ms on 10,000 files. `fresh::ensure` re-indexes only when
something changed. Every CLI query calls it first. The MCP server runs it in
the background and then keeps the index current with a `notify` watcher that
calls `index_files` for the paths that changed.

## Queries

- `find`: definitions by exact name or qualified name, with a fuzzy fallback
  (labelled as fuzzy). With a path, it gives the file outline plus imports.
- `refs`: references by name, or, for a file path, the files whose imports
  resolve to it.
- `blast`: breadth-first walk over callers, ranked by personalised PageRank
  with the teleport set to the edited symbol. With `--diff`, the symbols in
  git-changed hunks are the seeds.
- `context`: the body, callees, callers, tests, and sibling symbols, trimmed
  to a token budget.
- `verify`: symbol existence, and per-call checks on a parsed snippet that
  skip standard globals and calls on untyped receivers.

Ties are broken by path, line, and name, and the PageRank adjacency list is
sorted, so the same index always produces byte-identical output.

## What is not here yet

References resolve **by name**: `repo.save()` and `cache.save()` both point at
every `save`. Import- and scope-aware resolution with confidence per edge is
the v0.11 milestone. Until then, `refs` and `blast` say "name-level matching"
in their output. See [when not to use Tessera](when-not-to-use.md).
