# Reddit: r/rust

Audience: Rust developers. Lead with the implementation, not the AI pitch.
Plain "I made a thing" tone; flair it as a project.

## Title

`Tessera 1.0: a tree-sitter 0.26 + SQLite code index in Rust that checks AI agents' edits`

## Body

Tessera is a CLI, MCP server and library that indexes a repository and
checks changes against it: calls to names that do not exist, wrong argument
counts, broken imports, call sites a rename left behind, and routes or gRPC
methods removed while another service still calls them. AI coding agents
run it through their hook systems after every edit. 1.0 just shipped, and
the internals may interest this sub regardless of the agent angle.

**Extraction.** tree-sitter 0.26, one module per language behind a
`LanguageSupport` trait, 12 languages. The walk uses a tree cursor instead
of recursion, so deeply nested input cannot overflow the stack. Each module
emits symbols (with qualified names from the enclosing scope and an arity
range), references (with receiver and argument count), imports, import
bindings, and type hints (declared, `new`, parameter, field and return
types). Files are parsed on up to eight threads.

**Storage.** SQLite through rusqlite (bundled), WAL. The tables are
`files`, `symbols`, `refs`, `edges`, `imports`, `import_bindings`,
`type_hints`, `facts`, `skipped_files` and `meta`, plus an FTS5 trigram
table for fuzzy symbol search. An edge carries a confidence, the resolver
that produced it, an ambiguity group, and flags for a name its target
module lacks or an argument count no candidate accepts. The schema version lives in
`PRAGMA user_version` (v8); a mismatch rebuilds the index, since it is a
cache, not a format anyone else should read. Stale rows are deleted through
a temp table, which is how it got past `too many SQL variables` on
repositories above about 32k files.

**Resolution.** A pass after indexing turns each reference into edges, in
order: a definition in scope; an import binding followed through a module
resolver (relative paths, tsconfig `paths`, workspace packages, Java
packages) and re-export chains; `this`/`super` through the class and its
supertypes; a receiver's declared type, including field chains like
`this.deps.repo`; and finally a repository-wide name match (one candidate is
medium confidence, two to five are low). A name that an import, a type
hint or a package places outside the repository gets no edge, so `rows.get()` on a `Map` never shows
up under your own `get`. This is deep for TypeScript, JavaScript and Java;
the other languages stay at scope and unique-name resolution. An
incremental run re-resolves changed files and their dependents, and above
2,000 dependents it resolves everything.

**Freshness.** Every CLI query first compares each discovered file's size
and mtime with the stored values: one stat walk, no reads, under 100 ms on
10,000 files. Changed files are confirmed by SHA-256 before re-parsing.
The MCP server does the same check in the background and then follows a
`notify` watcher. Concurrent writers take a lock file, and a query that
finds another process indexing answers from the existing index.

**Framework packs as `.scm` queries.** Contracts (HTTP routes and clients,
Kafka producers and consumers, ORM entities) come from tree-sitter queries
in `packs/*.scm`. Each pack is a regex gate (a cheap scan that skips files
that cannot match), a language list, and a query. Captures follow one
convention: `@route`, `@client`, `@producer`, `@consumer` and `@entity`
mark the fact; `@method`, `@path`, `@prefix`, `@topic` and `@table` carry
its parts. Adding a framework needs no Rust beyond one table entry. gRPC
(Go, C#, Node, Python, Java, C++) and the `.proto`, `.sql`, OpenAPI and
Prisma readers are Rust. Facts are stored per file and joined by key at
query time.

**Determinism.** Ties are broken by path, line and name, and adjacency
lists are sorted, so the same index produces byte-identical output, and
`tests/robustness.rs` checks it.

**Hooks fail open.** `tessera hook <agent> <event>` is one entry point for
eight agents. Several agents run each other's hook files, so it detects the
payload shape that arrived (`tool_input`, `toolArgs`, `toolCall.args`) and
checks the tool name itself rather than trusting matchers. It always exits
0: if the index is missing or busy, or anything fails, it prints nothing or
the agent's no-op reply. A bug in Tessera never stops an agent.

**Stability.** Commands, flags, `--json` fields, MCP tool schemas, hook
replies and exit codes (0 ok, 1 blocking finding, 2 usage, 3 index or I/O)
are pinned for 1.x by `tests/stability.rs` against JSON fixtures, so a
release that drops a field fails its own tests.

Numbers: 99.7% of high-confidence references match compiler ground truth
(scip-typescript, scip-java) on public repositories; lookups under 50 ms at
p95 on 10,000 files; a 45,000-file repository indexes, with resolution, in
under a minute.

MSRV 1.90, Apache-2.0. `cargo install tessera-codegraph` or
`cargo binstall tessera-codegraph`.

https://github.com/iamsaquib8/tessera

Two things I would like opinions on:

1. Incremental resolution. Re-resolving dependents (importers, files whose
   edges lost their target, files with name matches on a name the change
   defines), with a cap at 2,000, works, but it is hand-rolled. Has anyone
   moved a tree-sitter-based indexer onto something like salsa, and was it
   worth it?
2. The regex gate in front of each query is what keeps the packs cheap on
   big repositories. Is there a better way to skip files before running a
   tree-sitter query that you have measured?
