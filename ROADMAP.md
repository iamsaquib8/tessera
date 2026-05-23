# Roadmap

Tessera is built in layers. The first releases focus on local deterministic navigation and a few standout algorithms; later releases add runtime-aware context and team-shared graphs.

## v0.1 (released)

- Local SQLite graph
- TypeScript, JavaScript, and Python extraction
- CLI query surface
- MCP stdio server
- Definitions, references, outlines, expansion, impact

## v0.2 (released)

- Java extraction (classes, interfaces, records, enums, methods, constructors, `new` and method-invocation references)
- TSX / React component support — JSX elements indexed as references to their component
- Go and Rust extraction (six total: TS/TSX/JS, Java, Python, Go, Rust)
- Personalised PageRank criticality with per-component breakdown
- Incremental re-index via sha-diff
- Memory-mapped graph snapshot for hot-path MCP queries
- Hallucination validator: `validate(symbol)` + `validate_snippet(code)` with Bloom-filter front-door and Jaro-Winkler near-miss suggestions
- Trigram FTS for fuzzy symbol lookup
- New tools: `stats`, `tests_for`, `validate`, `validate_snippet`
- `tessera bench` harness regenerating the README perf chart
- Library API (`tessera_codegraph::Index`)
- Optional Cozo (Datalog) graph engine behind `--features cozo`

## v0.3

- HTTP/SSE MCP transport
- More precise TypeScript import/export resolution
- More precise method and class member qualification
- Semantic git history (DuckDB layer)
- Architecture Decision Memory: ADR ingestion + retrieval at point of edit
- Repository benchmark series (Tuesday post)

## Later

- Runtime trace ingestion (test instrumentation libraries, OpenTelemetry, Sentry)
- Token-budgeted query planner
- Team Server (shared cloud-hosted index, SSO, audit log)
- Kotlin, Scala, and C# extraction
