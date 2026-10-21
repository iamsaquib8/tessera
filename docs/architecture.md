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
- `refs`: references whose edges resolve to the target's definitions, plus
  unresolved references by the same name, each with a confidence; for a file
  path, the files whose imports resolve to it.
- `blast`: breadth-first walk over resolved callers, ranked by personalised
  PageRank with the teleport set to the edited symbol; each caller carries
  the weakest confidence on its path. With `--diff`, the symbols in
  git-changed hunks (working tree by default) are the seeds.
- `context`: the body, callees, callers, tests, and sibling symbols, trimmed
  to a token budget.
- `verify`: symbol existence, and per-call checks on a parsed snippet that
  skip standard globals and calls on untyped receivers.

Ties are broken by path, line, and name, and the PageRank adjacency list is
sorted, so the same index always produces byte-identical output.

## Resolution

Extraction records, per file, more than names: import bindings (ESM, CommonJS,
Java single, wildcard and static imports), type hints (declared, `new`,
parameter, field, return types, interface properties), each reference's
receiver and argument count, and each symbol's container and arity.

After indexing, a resolve pass turns each reference into edges:

1. A definition visible in scope (`local`).
2. An import binding, followed through the module resolver (relative paths,
   tsconfig `paths`, workspace packages, Java packages) and re-export chains
   (`import`).
3. `this`/`super` through the class and its supertypes (`this`).
4. A receiver's declared type, including field chains such as
   `this.deps.repo` (`hint`), or a class used as a receiver (`static`).
5. Otherwise a repository-wide name match: one candidate is medium, two to
   five are low.

A name that an import, hint, or package places outside the repository gets no
edge. Overloads are told apart by argument count. An incremental index
re-resolves the changed files and their dependents (importers, files whose
edges lost their target, files with name matches on a name the change
defines); above 2,000 dependents it resolves everything and `status` says so.
See [accuracy](accuracy.md) for the measured results.
