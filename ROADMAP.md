# Roadmap

Tessera is a local, deterministic semantic code graph for AI coding agents. The
roadmap should optimize for two outcomes at the same time:

- Agents spend fewer tokens and make fewer navigation mistakes.
- Developers can understand the value in five minutes, trust it on a real repo,
  and want to share or star it.

This is a planning document, not a release contract. Items can move when a
smaller fix creates more user trust than a larger feature.

## Product Thesis

Most coding agents still navigate code by searching text, opening files, and
guessing relationships. Tessera should become the default local graph layer an
agent reaches for before reading code.

The highest-value roadmap items are the ones that create at least one of these
effects:

- A visible "aha" moment in the first run.
- Better correctness on messy, real-world repositories.
- A smaller context window for common agent workflows.
- A shareable artifact: benchmark, graph, demo, screenshot, blog post, or CI
  report.
- Lower friction for contributors and integrations.
- Clearer positioning against grep, repomaps, LSP-only indexing, hosted code
  search, and LLM-extracted graphs.

## Released

### v0.1 - Local Graph Foundation

- Local SQLite graph.
- TypeScript, JavaScript, and Python extraction.
- CLI query surface.
- MCP stdio server.
- Definitions, references, outlines, expansion, and impact queries.

### v0.2 - Agent-Grade Navigation

- Java extraction.
- TSX and React component support.
- Go and Rust extraction, bringing support to six language families.
- Personalized PageRank criticality with per-component breakdown.
- Incremental re-index via SHA diffing.
- Memory-mapped graph snapshot for hot-path MCP queries.
- Hallucination validator: `validate(symbol)` and `validate_snippet(code)`.
- Bloom-filter front door, Jaro-Winkler near-miss suggestions, and trigram FTS.
- New tools: `stats`, `tests_for`, `validate`, and `validate_snippet`.
- `tessera bench` harness for reproducible README performance charts.
- Library API through `tessera_codegraph::Index`.
- Optional placeholder `cozo` graph-engine feature.

### v0.3 - Token-Saver Workflow Tools

- `context_pack` for body, dependency signatures, caller signatures, and tests
  in one bounded response.
- `diff_impact` for changed-symbol impact analysis from a git diff.
- `imports` and `imported_by` for module-level navigation.
- `signature` for ultra-cheap API-shape lookup.
- `siblings` for nearby abstractions that share callers.
- CommonJS `require` and dynamic `import()` import tracking.
- Fuzzy and glob-style `search`.

### v0.4 - Language Breadth and Shareability

- C, C++, C#, Ruby, and PHP extraction, bringing support to 11 languages.
- `connect` for shortest call paths between symbols.
- `export` for Graphviz DOT and Mermaid call graph output.
- Drop-in `/tessera` Agent Skill.
- Install via npm, Homebrew, curl, Docker, cargo, and prebuilt binaries.
- Cross-platform CI and release builds.
- Logo, terminal demo, and social-preview branding.

## v0.5 - Trust, Polish, and Daily Use

Goal: make Tessera feel dependable enough to recommend after one real run.

Status: release-complete for trust, polish, and daily-use workflows. Future
language expansion now lives under real-repo correctness because available
grammar crates require a Tree-sitter core upgrade rather than a safe v0.5 patch.

### Product

- Ship `watch` as a first-class command: incremental daemon mode with clear
  output, `--once` for CI, debounce controls, and stale-index detection.
- Ship `unused` as a first-class command: zero-inbound-reference detection with
  kind, language, exported, path, and limit filters.
- Add `doctor`: check binary version, DB existence, schema version, snapshot
  freshness, ignored-path rules, parser availability, and MCP configuration.
- Add `init`: generate `.tessera` defaults, optional git hooks, MCP configs for
  common agents, and a short next-command prompt.
- Add shell completions for `bash`, `zsh`, `fish`, and PowerShell.
- Add `--explain` or `--why` on high-level queries so users can see why Tessera
  chose a caller, candidate, or near miss.
- Add stable JSON schema snapshots for every CLI and MCP response.
- Add local HTTP/SSE MCP transport for clients that cannot spawn stdio servers.
- Make missing-DB errors actionable: show `tessera index .` and the exact `--db`
  path being read.
- Make empty-result errors actionable: suggest `search`, `validate`, path
  filters, or a full re-index depending on the query.
- Sort all human and JSON outputs deterministically.

### Documentation

- Run a docs truth pass across README, `docs/`, `CHANGELOG.md`, and this file so
  released, unreleased-on-main, and planned features are clearly separated.
- Update `docs/architecture.md` to describe all 11 languages, import tracking,
  `connect`, `export`, `context_pack`, `diff_impact`, `signature`, `siblings`,
  `search`, `unused`, and `watch`.
- Update `docs/integrations.md` so the exposed tool list includes `connect` and
  `export`.
- Update `docs/quickstart.md` with the full install matrix instead of cargo-only
  installation.
- Add a "first five minutes" guide: install, index, run `impact`, run
  `validate`, run `connect`, export Mermaid, wire MCP.
- Add a "when not to use Tessera" section to build trust: generated code,
  language-server-only type inference, dynamic dispatch, runtime-only edges, and
  code not covered by supported grammars.
- Add a troubleshooting page for Windows paths, Docker mounts, stale snapshots,
  schema migrations, ignored directories, and MCP startup failures.

### Launch and Star Growth

- Refresh `launch/` assets for v0.5 with the actual command set and screenshots.
- Add a one-command demo script that records a fresh terminal GIF/SVG from
  `examples/sample`.
- Add repository topics for discovery. Candidate topics: `mcp`, `codegraph`,
  `tree-sitter`, `ai-agents`, `coding-agents`, `code-navigation`,
  `static-analysis`, `developer-tools`, `rust-cli`, `semantic-code-search`.
- Add curated GitHub labels: `good first issue`, `help wanted`, `parser`,
  `language-support`, `mcp`, `cli`, `docs`, `benchmarks`, `performance`,
  `windows`, `packaging`, `needs-repro`.
- Open 10-20 small, well-scoped starter issues from this roadmap so new
  contributors have obvious entry points.
- Add pinned issues for "language requests", "agent integrations", and
  "benchmark results from real repos".
- Add a public project board grouped by release, not by vague priority.
- Add comparison snippets against common alternatives: grep plus file reads,
  LSP-only lookup, aider repomap, hosted code search, and LLM-extracted graphs.
- Add a short demo video or asciinema for each killer workflow:
  `impact`, `validate-snippet`, `connect`, `context-pack`, and `diff-impact`.
- Add copy-paste MCP configs for more agents in the README, with the longer
  variants kept in `docs/integrations.md`.

## v0.6 - Real-Repo Correctness

Goal: reduce false edges, missed edges, and "works on the sample but not my
repo" moments.

Status: correctness-infrastructure complete. Deep per-language resolution and
new language families remain future v0.6/v0.7 work because they require parser
and resolver upgrades, not just CLI hardening.

### Language Depth

- Add new language families after upgrading the Tree-sitter core past the 0.20
  line: Kotlin, Swift, Scala, Lua, and Zig.
- TypeScript and JavaScript:
  - Resolve `tsconfig` path aliases.
  - Track barrel exports and re-exports.
  - Distinguish default, named, namespace, and type-only imports.
  - Resolve CommonJS `module.exports` and `exports.foo` patterns.
  - Model class members and object method calls more precisely.
  - Support decorators and overload signatures.
- Python:
  - Resolve `from x import y`, relative imports, aliases, and package roots.
  - Track class methods, static methods, properties, decorators, and async
    functions more precisely.
  - Improve test detection for `pytest`, `unittest`, and common `tests/`
    layouts.
- Java:
  - Resolve packages, imports, nested classes, constructors, overloads, and
    interface implementations.
  - Improve same-name method disambiguation using containing class and package.
- Go:
  - Resolve module paths, receiver methods, embedded structs, interfaces, and
    test files.
- Rust:
  - Resolve modules, `use` aliases, traits, impl blocks, macros, and crate
    boundaries more precisely.
- C and C++:
  - Improve headers, namespaces, templates, constructors, destructors, macros,
    and function-pointer edge handling.
- C#:
  - Improve namespaces, partial classes, extension methods, properties, and
    async methods.
- Ruby:
  - Improve modules, mixins, singleton methods, blocks, Rails-style structure,
    and dynamic-call caveats.
- PHP:
  - Improve namespaces, traits, methods, functions, includes, and Composer
    autoload roots.

### Correctness Infrastructure

- Build a parser-fixture corpus with compact examples for every supported
  grammar.
- Add at least one realistic fixture per language that resembles a small app,
  not just isolated syntax.
- Add regression tests for malformed files and partial syntax trees.
- Add fixture snapshots for symbols, references, imports, edges, and exported
  flags.
- Add deterministic output tests for human-readable CLI output.
- Add JSON schema compatibility tests for all MCP tools.
- Add performance budgets to CI for indexing, incremental re-indexing, and
  common queries on the sample corpus.
- Ship panic-free indexing guarantees: unreadable files, traversal errors, and
  parser failures degrade into warnings, not crashes.
- Ship better ignored-path defaults for generated, vendored, build, package
  manager, virtualenv, and cache directories.
- Ship opt-in include/exclude config in `.tessera/config.toml`.

## v0.7 - Agent Workflow Layer

Goal: make Tessera not just a graph, but the agent's navigation planner.

Status: first workflow layer complete. Tessera now has deterministic tool
planning and a one-call pre-edit bundle exposed through CLI and MCP. CI checks,
PR comments, pagination, and richer review workflows remain future v0.7/v0.8
work.

- Ship `plan-query`: given a natural-language task shape or requested symbol,
  recommend the cheapest sequence of Tessera tools.
- Add `context-pack v2`: include doc comments, nearby examples, relevant tests,
  public API shape, and diff context under one budget.
- Ship `edit-prep`: combine `validate`, `signature`, `siblings`,
  `context-pack`, and `tests_for` into a single pre-edit bundle.
- Add `review-pack`: summarize changed symbols, high-impact callers, missing
  tests, and likely docs updates for a PR.
- Add `tessera check`: CI-friendly checks for stale indexes, invalid snippets,
  unused public symbols, and high-impact changes without tests.
- Add a GitHub Action that comments a compact `diff_impact` report on PRs.
- Add SARIF or annotations for unused symbols and invalid references where that
  format is useful.
- Add MCP capability/version reporting so agents can adapt to the installed
  Tessera version.
- Add bounded result pagination and cancellation-friendly MCP behavior.
- Add tool descriptions optimized for agent routing, not just human docs.
- Add before/after agent transcripts that show token savings from using Tessera
  instead of search plus file reads.

## v0.8 - Visual Explorer and Shareable Graphs

Goal: make the graph visible enough that people share screenshots.

Status: shareable graph export complete. Tessera can now group Mermaid/DOT
exports, filter tests/exported endpoints, and write a self-contained Mermaid
preview HTML page with a copy button. The full interactive local web explorer,
SVG/PNG rendering, and IDE extensions remain future v0.8/v0.9 work.

- Add `tessera ui`: a local web explorer for symbols, references, impact,
  imports, and call paths.
- Add an impact heatmap grouped by file, directory, and language.
- Add an import graph view with cycle detection.
- Add a call-path explorer for `connect` with expandable branches.
- Ship a Mermaid preview page and copy button.
- Ship graph export controls:
  - Collapse tests, vendor, generated files, and external modules.
  - Group nodes by file, directory, package, module, or language.
  - Limit by depth, fanout, criticality, exported-only, or changed-only.
  - Include edge labels for call, JSX, import, constructor, or macro edges.
- Add SVG and PNG export for docs and PR comments.
- Add optional VS Code extension that shells out to the local binary.
- Add optional JetBrains integration once the CLI workflows are stable.

## v0.9 - Shared Team Graph

Goal: keep the local-first story while making Tessera useful for teams and CI.

Status: first end-to-end slice complete. Larger multi-user server features
remain future work.

Shipped:
- HTTP/SSE MCP transport for shared services and browser-based clients.
- Health endpoint metadata for service version, DB path, indexed root, schema,
  snapshot presence, and advertised endpoint paths.
- End-to-end subprocess coverage for HTTP health, SSE readiness, and MCP
  JSON-RPC query handling.

Remaining:
- Add a long-running daemon that combines file watching, query serving, and
  health endpoints.
- Add multi-root and monorepo workspace support.
- Add content-addressed index cache keyed by file hash, grammar version, and
  extractor version.
- Add remote cache import/export so CI can reuse an index built elsewhere.
- Add Docker Compose and Helm examples.
- Add OpenTelemetry traces and metrics for indexing and query latency.
- Add authentication, project-level permissions, and audit logs for team server
  mode.
- Add read-only share links for exported graphs and PR impact reports.
- Add a team benchmark mode that compares branches or releases over time.

## v1.0 - Stable Platform

Goal: make Tessera boring to depend on.

- Freeze the CLI and MCP contracts for a stable 1.x line.
- Publish a formal support matrix for languages, file extensions, and edge
  types.
- Document known precision limits per language.
- Stabilize the SQLite schema or provide durable migrations between all 1.x
  versions.
- Add semver rules for CLI output, JSON fields, library APIs, and MCP tools.
- Add signed release artifacts, checksums, SBOMs, and supply-chain provenance.
- Add package-manager expansion where useful: Scoop, winget, apt repository,
  GitHub Action setup step, and versioned Docker tags.
- Add plugin or extractor extension points for experimental languages and
  private in-house DSLs.
- Add a docs site with versioned docs, examples, benchmark results, and an
  integration gallery.
- Add public benchmark suites for representative open-source repos.

## High-Leverage Fix Backlog

These are not glamorous, but they are the kind of fixes that make users trust a
developer tool quickly.

- Documentation drift:
  - Keep README, quickstart, integrations, architecture, changelog, and roadmap
    aligned with the same shipped command set.
  - Mark commands as released, unreleased-on-main, or planned.
  - Keep benchmark versions and README version labels current.
- CLI ergonomics:
  - Improve errors for missing DBs, stale schemas, unsupported languages, empty
    results, malformed snippets, and invalid graph-export options.
  - Add `--verbose` diagnostics for ignored files and parser failures.
  - Add `--quiet` for scripts and CI.
  - Add consistent exit codes for not-found, invalid input, stale DB, and
    internal error.
- Determinism:
  - Sort all rows before output.
  - Make graph export stable across runs.
  - Snapshot ambiguous same-name resolution choices in tests.
  - Avoid nondeterministic filesystem traversal ordering.
- Performance:
  - Guard against quadratic traversals on high-fanout symbols.
  - Add memory limits or streaming paths for huge repos.
  - Add benchmark coverage for 10k, 50k, and 100k file repos.
  - Track cold-start MCP latency and snapshot load time.
- Cross-platform behavior:
  - Test Windows path separators, drive letters, case-insensitive filesystems,
    CRLF files, symlinks, Docker volume mounts, and non-UTF-8 filenames.
  - Make install scripts and npm wrapper behavior consistent across platforms.
- MCP robustness:
  - Return structured JSON-RPC errors.
  - Clip or paginate huge responses with explicit continuation hints.
  - Include version and schema metadata in every response or server capability.
  - Keep all query tools read-only and safe to auto-approve.
- Packaging:
  - Verify prebuilt binaries before install.
  - Make `curl | sh` auditable and easy to inspect.
  - Add checksums to releases.
  - Add installation smoke tests for npm, Homebrew, Docker, cargo, and prebuilt
    archives.
- Security and privacy:
  - Document that indexing stays local by default.
  - Document exactly what `diff_impact` shells out to.
  - Avoid sending source code to any remote service.
  - Add threat-model notes for team-server mode before shipping it.

## Growth and Community Backlog

The repository already has core community files, issue templates, PR template,
security policy, CI, and release workflows. The next growth work should be more
specific and more visible.

- GitHub metadata:
  - Keep the community profile complete.
  - Add high-signal topics, description, website/docs link, and social preview.
  - Pin the best demo issue, benchmark issue, and contribution issue.
- Issues:
  - Convert roadmap items into small issues with acceptance criteria.
  - Maintain `good first issue` only for tasks that are genuinely small.
  - Use `help wanted` only where external contributors can succeed without
    private context.
  - Keep `needs-repro` for parser and platform bugs.
- Discussions:
  - Add channels for language requests, agent integrations, benchmarks, and
    show-and-tell graphs.
  - Ask users to post benchmark output from their repos with sensitive paths
    removed.
- Contributor path:
  - Add `CONTRIBUTING.md` examples for parser fixtures, CLI tests, and docs-only
    PRs.
  - Add a "how to add a language" guide once the extractor pattern is stable.
  - Add a "how to add an MCP tool" guide.
  - Add a maintainer checklist for reviewing language extractor PRs.
- Proof and content:
  - Publish one technical post per major release.
  - Publish benchmark posts with raw reproduction commands.
  - Publish "agent before and after Tessera" transcripts.
  - Publish "why deterministic beats LLM-extracted for navigation" with caveats.
  - Publish "how to wire Tessera into Codex, Claude Code, Cursor, Cline,
    Continue, Zed, and Aider".
- Distribution:
  - Keep npm install as the easiest path for agent users.
  - Keep cargo install as the Rust-native path.
  - Keep Homebrew and Docker working for platform trust.
  - Add Windows-native install paths after Windows CI is boring.

Useful GitHub references for this section:

- Community profile checklist:
  https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/about-community-profiles-for-public-repositories
- Repository topics:
  https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics
- Labels:
  https://docs.github.com/en/issues/using-labels-and-milestones-to-track-work/managing-labels

## Star-Attracting Issue Seeds

These are intentionally scoped so they can become GitHub issues directly.

- Fix `docs/integrations.md` exposed-tool list so `connect` and `export` are
  included.
- Update `docs/architecture.md` for the current 11-language extractor set.
- Add `docs/troubleshooting.md` with missing DB, stale snapshot, schema, MCP,
  Windows, and Docker mount fixes.
- Add `tessera doctor`.
- Add `tessera init`.
- Add shell completions.
- Add deterministic JSON snapshots for every CLI command.
- Add fixtures for TS path aliases and barrel exports.
- Add fixtures for Python relative imports and aliases.
- Add fixtures for Java package and same-name method disambiguation.
- Add fixtures for Go receiver method resolution.
- Add fixtures for Rust module and trait method resolution.
- Add fixtures for C++ namespaces and methods in headers.
- Add a public benchmark recipe using one or more open-source repos.
- Add a one-command demo generator for `assets/demo.svg`.
- Add a Mermaid graph example to the README.
- Add `diff-impact` GitHub Action prototype.
- Add `CONTRIBUTING.md` section for writing parser fixtures.
- Add `CONTRIBUTING.md` section for adding a new MCP tool.
- Add a `docs/how-to-add-a-language.md` guide.
- Add release smoke tests for npm, Homebrew, Docker, cargo, and prebuilt
  binaries.
- Add install script checksum verification.
- Add stable exit-code documentation.
- Add `--verbose` ignored-file diagnostics.
- Add `--quiet` script mode.
- Add import-cycle detection.
- Add graph export grouping by directory and language.
- Add `tessera ui` proof of concept.
- Add benchmark CI budget for the sample corpus.
- Add language-support matrix table.
- Add known-limits table per language.

## Success Metrics

Track metrics that prove developer trust, not vanity alone.

- Time to first useful result after install: target under five minutes.
- Successful install rate across npm, Homebrew, Docker, cargo, and prebuilt
  binaries.
- `tessera index .` success rate on real user repos.
- Median incremental re-index time on medium and large repos.
- Median MCP startup time and first-query latency.
- Token savings for common workflows: definition lookup, impact, pre-edit
  context, and PR review.
- Number of benchmark reports from external repos.
- Number of agent integrations with copy-paste configs.
- Number of issues closed by new contributors.
- Ratio of docs issues to feature issues; rising docs issues usually means
  growth, but repeated setup questions mean the docs are failing.
- Stars are a lagging signal; shareable demos, trustworthy docs, and repeatable
  benchmarks are the leading signals.
