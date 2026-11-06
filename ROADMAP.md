# Roadmap

**Tessera's job:** your agent says done, Tessera checks. It gives a
deterministic blast radius before every edit and verification after it,
across files, repos, routes, topics, and tables. It runs locally, uses no
LLM, and is Apache-2.0.

Every milestone ships with proof: a test suite, a published number, or a
demo. Nothing is claimed until it can be reproduced. The full design is in
[docs/design/2026-10-21-overhaul.md](docs/design/2026-10-21-overhaul.md);
what other tools do, and what this roadmap takes from them, is in
[docs/design/2026-10-25-landscape.md](docs/design/2026-10-25-landscape.md).

| Version | Focus | What you get | Status |
| --- | --- | --- | --- |
| v1.0 | Visible and stable | Change-impact UI, pull-request comment, 1.x contracts frozen | released (1.0.1) |
| v1.1 | Contract map, completed | Express mounts, GraphQL, config keys, `path` across repositories, Go/Python/Ruby frameworks | released in 1.2.0 |
| v1.2 | Team and CI | Explainable test selection, a shared index cache, findings in code review tools, a merge gate | released (1.2.0) |
| 1.2.x | Every install works | Every channel published and smoke-tested on every release, `@v1` that follows releases ([#264](https://github.com/iamsaquib8/tessera/issues/264)) | now |
| v1.3 | Never silently wrong | No answer of "fine" without a check, no blocking false positives, a good first five minutes ([#265](https://github.com/iamsaquib8/tessera/issues/265)) | next |
| v1.4 | Proof and the public launch | The refactor eval and a private beta, published; the release we announce ([#267](https://github.com/iamsaquib8/tessera/issues/267)) | planned |
| v1.5 | Fast on large repositories | Hooks, CI and queries on a 15,000-file codebase within budget ([#266](https://github.com/iamsaquib8/tessera/issues/266)) | planned |
| v1.6 | Contracts that know their fields | Field-level breaking changes joined to the consumers they break, payload compatibility ([#173](https://github.com/iamsaquib8/tessera/issues/173)) | planned |
| v1.7 | Inside the agent loop | Graph context in the agent's own tools, an LSP server, edit plans ([#174](https://github.com/iamsaquib8/tessera/issues/174)) | planned |
| v1.8 | Deep tier for Python and Go, precise mode | Import- and type-aware Python and Go, SCIP import and export ([#175](https://github.com/iamsaquib8/tessera/issues/175)) | planned |
| v1.9 | Architecture guardrails | One rules file across languages and contracts, drafted from your code, with a baseline ([#176](https://github.com/iamsaquib8/tessera/issues/176)) | planned |
| v1.10 | Data and ownership | Migration checks joined with the code that reads the data, CODEOWNERS impact, catalog export ([#177](https://github.com/iamsaquib8/tessera/issues/177)) | planned |
| v2.0 | One small surface | The deprecated commands, flags and tools removed; about seven MCP tools; a new stability contract ([#179](https://github.com/iamsaquib8/tessera/issues/179)) | planned |
| After 2.0 | Runtime and extension | OpenTelemetry overlays, column lineage, your own packs, editors, code-anchored notes | ideas |

**Why this order.** Shipping 1.2.0 and measuring it showed where Tessera is
weakest, and none of it is a missing feature. Installs failed on most
channels until the release itself was repaired. The core audit found
thirteen ways Tessera answers "fine" without having checked and nine
blocking false positives, either of which teaches users to switch the hooks
off. On a 15,000-file repository an incremental index takes seconds, which
bounds every hook. And nothing yet measures whether agents miss fewer breaks
with Tessera. Trust and evidence come before the public launch, speed right after it,
and v1.4's results reorder everything after that.

Every 1.x item is additive: 1.x keeps what [docs/stability.md](docs/stability.md)
promises. The only breaking changes are the surface cuts in v2.0, each
deprecated with a warning and a replacement during 1.5 to 1.10 first.

## Where Tessera stands

Agents now read compiler diagnostics after an edit (Claude Code's LSP
plugins, the JetBrains IDE MCP server), so a missing name in TypeScript or
Java is no longer news. What no compiler sees, and what Tessera is for:

- breaks between services and repositories: a removed route, RPC or topic a
  client still uses, and soon a removed field;
- the tests a change reaches, across languages and service boundaries;
- cross-file checks in languages with no type checker (JavaScript, Python,
  Ruby);
- a deterministic gate at "done" in every agent with a hook system, local
  and under a permissive licence;
- accuracy anyone can re-run: precision against SCIP ground truth, and
  hand-labelled contract links with a held-out corpus.

The milestones below build on those, in that order.

## Shipped

- **Foundation (v0.10).** tree-sitter 0.26, discovery that honours
  `.gitignore`, zero-setup incremental SQLite indexing, and an MCP server
  for the 2026-07-28 protocol and for initialize-based clients (2025-11-25,
  2025-06-18, 2025-03-26, 2024-11-05), over stdio and Streamable HTTP. The
  release pipeline builds for npm, Homebrew, `curl | sh`, cargo-binstall and
  Docker; npm publishing and a public container image are not live yet.
- **Precision (v0.11).** Import- and scope-aware resolution for TypeScript,
  TSX, JavaScript and Java, with a confidence on every edge. Against SCIP
  ground truth, high-confidence precision is 0.998 (TS/JS) and 0.995 (Java)
  ([docs/accuracy.md](docs/accuracy.md)).
- **Edit-safety loop (v0.12).** `blast --diff`, `verify` (missing names,
  argument counts, references a change removed, broken imports), hooks in
  Claude Code, Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code,
  Augment and Antigravity that block only on problems the agent introduced
  ([docs/hooks.md](docs/hooks.md)), and `init --git-hook` for every other
  agent. A blind find-and-replace over the benchmark corpus leaves 15
  blocking findings, all real. **Not met yet:** the refactor eval's agent
  arms (call sites agents miss, false-positive blocks) have not run.
- **Contract map (v0.13).** gRPC in six languages, HTTP routes and clients,
  Kafka topics, SQL migrations and ORM models, workspaces of repositories,
  and V5 (a removed contract a client still uses).
- **Visible and stable (v1.0).** `tessera ui` with `--export`, the GitHub
  Action's pull-request comment, and the 1.x contract pinned by
  `tests/stability.rs`.
- **Contract map, completed (v1.1).** Express mounts; Go, Python and Ruby
  web frameworks; GraphQL; config keys; `path` across repositories
  ([docs/contracts.md](docs/contracts.md)). Hand-labelled routes and topics
  on three corpora, one held out, give no wrong link. Recall is the open
  side: 0 of 17 clients on the held-out corpus, whose URLs are built from
  app config, and no topic clients for sarama, Confluent.Kafka or Kotlin.

## v1.2: Team and CI

Make Tessera part of every pull request, and cut CI minutes with test
selection a reviewer can trust.

- **`tessera affected <rev>`.** The tests a change reaches: through calls,
  imports and contracts, so a changed route selects the client's tests in
  another service.
  - A reason for every test (`selected by UserRepo.findById via
    loadUser`), and `--explain` for a skipped one.
  - Safety rails: new, changed and previously failed tests always run;
    changes to lockfiles, build files, config and fixtures run the full
    suite; a `Tessera-No-Skip` commit trailer turns selection off.
  - Output each runner takes: `--format jest`, `vitest`, `pytest`
    (`file::test`), `go-test` (packages and `-run`), `cargo`, `json`.
  - Package boundaries from npm/pnpm/yarn workspaces, Cargo, Go and Maven
    modules, so it composes with Nx and Turborepo rather than replacing them.
- **A shared index cache that never leaves your control.** `tessera cache
  export <file>` and `import <file>`, keyed by tree hash, Tessera version
  and config; CI moves the file (the Action uses `actions/cache`), and a
  branch imports its merge base and re-indexes only what changed. Tessera
  itself makes no network calls.
- **Findings where review happens.** SARIF uploaded to GitHub code scanning
  by the Action; GitLab Code Quality, Azure DevOps and Bitbucket report
  formats; NDJSON for agent loops (`--format ndjson`).
- **A baseline.** `tessera baseline` records today's findings; CI and hooks
  block only on new ones, so a legacy repository can adopt Tessera on day
  one.
- **Accepted breaks.** An intended break gets a reviewed record
  (`.tessera/accepted.toml`, keyed by finding fingerprint, with a reason and
  an expiry) instead of a disabled check.
- **A merge gate in the Action**, with a "tests to run" list a reviewer can
  copy, and test gaps (changed code no test reaches).

**Gate:** on three public monorepos, `affected` selects every test that
fails after a seeded break and skips at least half of the suite on a typical
change.

Measured (tessera-bench, 2026-11-03): 249 of 251 failing test files
selected across 62 seeded breaks (nest 37 of 38, excalidraw 102 of 103,
opentelemetry-go 110 of 110); the two misses use a class or function as a
value, which the index does not record yet (#236). On each repository's
last 40 commits the median share of tests selected is 3.6% (nest), 13.8%
(opentelemetry-go) and 85% (excalidraw, a single React app whose changes
reach most of its tests). The index cache halves a large repository's CI
index (vscode: 23.5 s instead of 44.2 s) but does not reach a tenth of it
until an incremental index stops re-resolving unchanged code (#237).

## 1.2.x: Every install works

Patch releases, no new surface: npm and crates.io through trusted
publishing, the Homebrew tap, a public container image, smoke tests on every
channel and OS that run on every release, release notes from the CHANGELOG,
the docs site pinned to the latest release, and the `v1` tag moved by the
release workflow. Supply-chain basics with it: pinned actions, protected
tags, Dependabot, notices and an SBOM.

**Gate:** a tag with no human steps after it publishes every channel, passes
every smoke test on Linux, macOS and Windows, and moves `@v1`.

## v1.3: Never silently wrong

- **No silent no-ops.** A corrupted or half-built index, a missing binary,
  a watcher that cannot start, a git hook without `tessera` on PATH: each
  either works or says so, in the hook reply and on stderr.
- **No blocking false positives** in the deep tier: Lombok and library
  default methods in Java, anonymous default exports, decorated and
  parameter properties and `paths` without `baseUrl` in TypeScript, V5 and
  diff mapping that blame untouched code.
- **The misses that matter:** values as references, `A::B` names, new
  copies of a baselined break.
- **A good first five minutes:** a misspelt symbol says so instead of "no
  callers"; `init` sets up the project's agents and prints next steps; one
  getting-started page; a README that leads with what a first visitor needs;
  one answer for "tests to run".

**Gate:** a fault-injection suite in which no case exits 0 silently; on the
last 300 commits of five public repositories, fewer than 1 blocking finding
per 100 commits that the compiler does not confirm; someone new to Tessera
gets from install to a hook holding their agent in under five minutes.

## v1.4: Proof and the public launch

- **The refactor eval, run and published:** agent arms with and without
  Tessera, 30 tasks, at least two agents, false-positive blocks counted.
- **Overlap with compilers and vendor checks**, measured, so Tessera claims
  only what they miss.
- **A private beta:** five to ten teams on 1.3 for four weeks, sharing
  `tessera stats --share` and every false block; Tessera's own development
  under the hooks for the same weeks.
- **The launch release:** fixes from the beta and the eval, the cheap
  large-repository wins (index-backed lookups, faster `context`), a public
  demo, the eval report on the docs site, a release candidate to the beta
  group first. The plan is in
  [docs/plans/2026-11-07-public-launch.md](docs/plans/2026-11-07-public-launch.md).

**Gate:** a reproducible report per agent and language (breaks caught at
"done" with and without Tessera, false blocks per 100 edits, time added per
edit), beta data from at least five repositories, and a week of release
candidate use with no new silent no-op or blocking false positive. The
launch claims only what the report shows, and the milestones after this one
are reordered by it.

## v1.5: Fast on large repositories

- **Incremental re-resolution** limited to what a change can affect, so a
  one-file edit re-indexes in milliseconds and a cached CI run skips most of
  the work.
- **Index-backed lookups** for symbol names, `context` and fact scans.
- **Narrower test runs:** `affected` names the tests inside a selected file
  for jest and vitest, not only the file.
- **Perf budgets that count work**, not wall time, plus a large-repository
  benchmark in tessera-bench.

**Gate:** on vscode, post-edit hook p95 under 300 ms, Stop under 2 s, a
cached CI run under 25% of a cold index.

The 2.0 deprecations start here: one or two per minor from v1.5 to v1.10,
each hidden from `--help`, warned about on use, and listed with its
replacement.

## v1.6: Contracts that know their fields

V5 knows when a route, RPC or topic disappears. Most real breaks are
smaller: a removed field, a renamed enum value, a narrowed type.

- **Field-level diff, joined to consumers.** For `.proto`, OpenAPI,
  GraphQL and AsyncAPI documents in the repository, classify each change as
  breaking, dangerous or safe, then name the consumers it breaks at
  file:line. Reads the JSON of `buf breaking` and `oasdiff` where teams
  already run them, instead of reimplementing them.
- **Response shapes from code.** For HTTP handlers whose response type is
  declared (TypeScript, Java, Go structs, Pydantic), compare the fields a
  client reads with the fields the handler returns.
- **GraphQL operation validation.** Indexed client operations checked
  against the schema: a removed field an operation selects is a finding.
- **Event payload compatibility** for mapped topics, with Confluent's
  BACKWARD, FORWARD and FULL modes.
- **Spec drift.** Routes served in code but missing from the OpenAPI
  document, and documented routes no handler serves.
- **V5 across a workspace:** a provider change checked against consumers in
  sibling repositories, at their default branches, as a cross-repo merge
  gate.
- **Recall where the ground truth found gaps:** clients whose URLs come from
  app config, `str.format` and request wrappers; servers in Slim,
  Sinatra, Phoenix and ASP.NET minimal APIs; Kafka clients for sarama,
  Confluent.Kafka and Kotlin; SQS/SNS, RabbitMQ, NATS and Pub/Sub.

**Gate:** field-level truth for two corpora, no wrong link on a held-out
one, and client recall above 0.6 on bank-of-anthos.

## v1.7: Inside the agent loop

Agents forget to call tools they have to choose. Put the graph where they
already look.

- **Context in the agent's own tools.** When the agent greps or reads a
  symbol, a pre-tool hook adds its callers, contracts and tests in a few
  lines, within the same budget as today's pre-edit note.
- **An LSP server** (`tessera lsp`) that publishes `verify` findings as
  diagnostics and serves references and call hierarchy from the index: one
  server for editors and for agents that read LSP diagnostics, such as
  Claude Code's LSP plugins.
- **`tessera plan rename | move | signature`.** A deterministic, ordered list
  of every site to edit, across files, services and contracts, and the
  `verify` check that proves the plan was followed. Tessera plans; the agent
  edits.
- **Fix hints.** High-confidence findings carry the change that resolves
  them (the import to add, the argument list the target takes) in JSON and
  hook replies.
- **A loop breaker.** When the same finding survives several edits in a row,
  the hook stops repeating it and returns everything the agent needs in one
  reply: the sites, the target's signature, and its callers.
- **Tests in the loop (opt-in).** At Stop, run the tests `affected` selects
  and hold the agent while they fail.
- **Handoff.** A finding in the pull-request comment carries a command that
  opens it in the agent.
- **The refactor eval, published.** Call sites missed and false-positive
  blocks, for each agent with and without Tessera, as a vendor-neutral
  benchmark anyone can run.
- **More hook targets** as their contracts allow: Windsurf/Devin (Cascade
  hooks), Kiro, Junie, OpenCode and Factory.

**Gate:** rerun with these features, v1.4's eval shows fewer missed call
sites for every agent measured, with under 5% false-positive blocks; hook
latency stays within v1.5's budget.

## v1.8: Deep tier for Python and Go, precise mode

- **Python:** packages and namespace packages, `pyproject.toml` source
  roots, relative imports, type hints, dataclass and Pydantic fields.
- **Go:** module paths, package-qualified calls, struct methods and embedded
  fields, and interface satisfaction.
- **Precise mode.** Import SCIP indexes where a project has them
  (scip-typescript, scip-java, scip-python, scip-go, rust-analyzer, scip-clang,
  scip-ruby, scip-dotnet) to raise edges to compiler grade with resolver
  `scip`; export SCIP so other tools can read Tessera's graph.
- Deep tier for Ruby, C# and Kotlin follows the same benchmark.

**Gate:** high-confidence precision at least 0.95 and resolved recall at
least 0.85 for Python and Go against scip-python and scip-go ground truth on
public repositories; the bar is not lowered.

## v1.9: Architecture guardrails

- **`.tessera/rules.toml`**, one file across languages: layers
  (`domain/**` may not import `infrastructure/**`), forbidden and deprecated
  calls, cycle bans between packages, and contract rules (only the billing
  service may write the `payments` table; only the gateway may call
  `internal/*` routes).
- **Rules drafted from the current graph** (`tessera rules draft`), and a
  baseline so existing violations do not block.
- **A pre-edit query:** "may `ui/Checkout.tsx` use `db/orders.ts`?", as an
  MCP tool and in the pre-edit note.
- Enforced by `verify`, hooks and CI, with the rule that fired in every
  finding.

**Gate:** checks add under 50 ms to a single-file verify; every seeded
violation in the monorepo fixtures is caught.

## v1.10: Data and ownership

- **Migrations joined with usage** (`verify --migrations`): a dropped or
  renamed column or table that code still reads is a blocking finding;
  destructive and backward-incompatible operations are flagged with the
  safe alternative. Flyway, Alembic, Prisma, TypeORM, Rails and Django
  migrations.
- **CODEOWNERS impact** (`blast --owners`): the teams that own the callers
  and services a change reaches, named in the pull-request comment; and
  CODEOWNERS validation (unowned paths, shadowed rules).
- **Catalog export:** Backstage `catalog-info.yaml` with `providesApis`,
  `consumesApis` and `dependsOn` generated from the contract map, so a
  catalog stops being kept by hand.
- **Modern full-stack contracts:** Next.js Server Actions, tRPC procedures,
  and Remix / React Router loaders and actions.

**Gate:** every unsafe drop in a corpus of historical migrations is caught,
with no false block on the safe ones.

## v2.0: One small surface

The only breaking release, and only for what 1.5 to 1.10 deprecated
([#179](https://github.com/iamsaquib8/tessera/issues/179)):

- **One command per job:** `report` for the pull-request report, `find` for
  names, globs and fuzzy text (absorbing `search`), `outline` for a file's
  symbols; `index PATH`, `watch` and `export --html-out` gone.
- **One switch per idea:** `--json` for machine output, one way to name a
  change, `.tesseraignore` for exclusions, one argument name per concept
  across the CLI and MCP, one confidence vocabulary.
- **About seven MCP tools**, each with a description an agent reads
  correctly.
- **A documented library API**; everything else private.
- **A new stability contract** for 2.x, a migration table in the changelog,
  and a check that lists every deprecated form a project still uses.

**Gate:** every removal was deprecated for at least one minor with a warning
and a replacement; the migration check finds nothing on the tessera-bench
corpora after migration; every 1.x hook reply and exit code keeps its
meaning.

## After 2.0

| Theme | What |
| --- | --- |
| Runtime truth | OpenTelemetry service-graph metrics first, then traces: callers ranked by traffic, static edges no traffic uses, runtime edges with no static evidence |
| Data lineage | Columns traced from migrations through models and API responses to their readers; PII rules in `verify` |
| Extension platform | Framework packs loaded from `.tessera/packs/*.scm` without rebuilding, then WASM extractors and bring-your-own grammar |
| Editors | VS Code and JetBrains extensions over `tessera lsp`: callers on hover, the contract map in a panel |
| Code-anchored notes | `tessera note add <symbol>`, shown in `context` and `blast`, flagged stale when the code changes |
| Diagrams | Service and call diagrams generated from the graph for docs and reviews |
| Dead code | Routes nothing calls, consumers of topics nothing produces, exports nobody imports, after each change |

**Every release also brings:** framework and queue packs, infrastructure
packs (Kubernetes, Terraform, serverless) that map services to deployments,
new agent adapters as hook systems ship, and a benchmark row for every
addition.

## Not planned

From the design: no LLM-extracted semantics, embeddings or vector search;
no hosted service; Tessera does not edit code (it plans, the agent edits);
and no index data or code leaves your machine.

## Contributing to the roadmap

Language requests, agent integrations, and framework packs each have an issue
template. A pack or language is a small, self-contained contribution; see
[CONTRIBUTING.md](CONTRIBUTING.md).
