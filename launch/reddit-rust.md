# Reddit — r/rust

Audience: Rust devs. Lead with the implementation/architecture, not the AI pitch. Honest, technical, "I made a thing". Likely flair: "Project" / "I made ...".

## Title
`Tessera: a Tree-sitter + SQLite code graph in Rust, with personalized PageRank impact and mmap snapshots`

## Body

I've been building **Tessera**, a local semantic code graph for AI coding agents,
and the internals might interest this sub regardless of the AI angle.

Pipeline:
- **Tree-sitter** for 11 languages (TS/TSX/JS, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP). Per-language extraction of symbols, references, and imports via a single `Visitor` walking the parse tree.
- **SQLite** (rusqlite, bundled, WAL) as the store: `symbols`, `refs`, `edges`, `imports`, plus an **FTS5 trigram** virtual table for fuzzy symbol search and a **Bloom filter** blob for O(1) existence checks.
- **Incremental indexing**: SHA-256 per file, only changed files re-parse, cascading deletes in one transaction. ~38 ms to re-index a near-million-LOC repo.
- **Memory-mapped snapshot**: post-index, a `bincode` archive of the symbol/edge tables that the MCP server `mmap`s so hot-path queries skip SQLite.
- **Personalized PageRank** (hand-rolled power iteration over a reverse call graph) for "impact" — ranks callers by how much they actually matter relative to the edit site, with an auditable breakdown.

A grammar-pinning gotcha that ate an afternoon and might save you one: several
`tree-sitter-*` grammar crates declare `tree-sitter = ">= 0.20"` with no upper
bound, so a fresh resolve happily pulls `tree-sitter 0.26` alongside the 0.20
line the other grammars use → `Language` type mismatch. Pinning every grammar to
`>=0.20, <0.21` keeps the resolver in one ABI window. Adding C/C++/C#/Ruby/PHP
this release meant finding each grammar's 0.20-compatible version.

New this release: `connect` (shortest call path via BFS over the graph) and
`export` (call graph → Graphviz DOT / Mermaid). petgraph is in the deps; the
PageRank is custom because I wanted the personalized teleport vector.

CLI + library (`tessera_codegraph::Index`) + MCP server. Apache-2.0,
`cargo install tessera-codegraph`.

https://github.com/iamsaquib8/tessera

Feedback on the graph model and the incremental path especially welcome — and if
anyone's done full type-resolution over Tree-sitter without a language server, I'd
love to compare notes (name-based edge resolution is my current weak spot).
