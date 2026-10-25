# Landscape, 2026-10-25

What else gives AI coding agents code context, checks their edits, or guards
contracts, tests and architecture; where Tessera stands; and what the
roadmap takes from each. Sources are linked at the end of each section.
Figures a vendor reports about itself are marked *vendor claim*; anything
not confirmed from a primary source is marked *unverified*. This is a
working document for planning, not marketing: it describes other tools as
their own documentation does.

## Summary

- **Closest overlap.** Two open-source projects build a tree-sitter graph
  for agents with impact analysis: GitNexus (PolyForm Noncommercial; hooks
  that enrich context, a cross-repo contract registry for HTTP and
  AsyncAPI) and code-review-graph (MIT; tree-sitter + SQLite, a PR comment
  and a `fail-on-risk` merge gate). Neither documents holding an agent at
  "done", and neither publishes precision against compiler ground truth
  (code-review-graph reports impact F1 0.693 and calls its own recall
  figure circular).
- **Agents now read diagnostics natively.** Claude Code's LSP plugins report
  type errors and missing imports after each edit (terminal sessions only);
  the JetBrains IDE MCP server and Zed's agent expose diagnostics tools. In
  TypeScript, Java and Go, a missing name or wrong argument count is already
  a compiler error. Tessera's lasting value is what no single compiler sees:
  breaks across services and repositories, removed contracts, tests to run,
  cross-file checks in dynamic languages, and a deterministic gate at
  "done" in every agent with a hook system.
- **The closest "gate at done"** is GitHub Copilot's coding agent, which
  validates its own work with CodeQL, the advisory database and secret
  scanning before it finishes; since June 2026 the same validation runs for
  third-party agents on GitHub. It is cloud-only and security-focused.
- **Agents under-use pull-only tools.** Cursor replaced embedding indexing
  with local grep; Moderne's docs note that agents do not call local tools
  consistently. Context has to arrive through hooks and the agent's own
  tools, not only through an MCP tool the agent must choose.
- **AI review tools converged** on code graph + LLM, multi-repo context, MCP
  in and out, learned rules and autofix (CodeRabbit, Greptile, Qodo, Cursor
  Bugbot, Copilot code review). All accept MCP servers, so Tessera can be
  the deterministic tool they call.
- **Contract, migration and test tools are spec- or history-based.** Buf,
  oasdiff and GraphQL Inspector diff specs; Pact needs written contract
  tests; Atlas migration lint is Pro-only since v0.38; Develocity,
  Launchable and Datadog select tests from history or coverage. None joins
  a change to the consumers that use it, from code alone, across languages.

## Agent context and code-graph tools

| Tool | Approach | Local/cloud | License/price | Edit loop | Cross-service |
| --- | --- | --- | --- | --- | --- |
| Tessera | tree-sitter + SQLite, deterministic | Local | Apache-2.0 | Blocks at "done" in 8 agents; blast radius before edits | HTTP, gRPC, GraphQL, Kafka, SQL, config, multi-repo |
| GitNexus | tree-sitter + LadybugDB; optional LLM wiki | Local; enterprise SaaS or self-host | PolyForm NC; commercial licence | Pre/PostToolUse context enrichment, stale-index hint; no blocking documented | Repo groups: HTTP route↔consumer, AsyncAPI topics |
| code-review-graph | tree-sitter + SQLite | Local; Action on the runner | MIT | Hooks "where supported"; PR comment; `fail-on-risk` | Framework edges (Spring, Laravel) |
| CodeGraphContext | tree-sitter, optional SCIP, graph DB | Local | MIT | File watch | No |
| Codanna | Rust index + local embeddings | Local | Apache-2.0 | Not documented | No |
| Serena | LSP or JetBrains backend; symbolic edits | Local | GPL-3.0+ (SolidLSP MIT) | None documented | No |
| Sourcegraph MCP | Search + SCIP + LLM Deep Search | Cloud or self-host | Enterprise from $16K/yr | No | Cross-repo definitions; no contract model |
| Augment Context Engine | Embedding retrieval | Index on Augment's cloud | Usage-based | No | Remote mode spans repos |
| Moderne `mod mcp` | OpenRewrite LSTs, recipes | Local (experimental) + platform | Free on OSS, licence for private | Deterministic `change_method_name` etc. | Bulk multi-repo recipes |
| Claude Code LSP plugins | LSP diagnostics after each edit | Local terminal only | Free | Diagnostics after Edit/Write | No |
| JetBrains IDE MCP | IDE PSI analysis | Local IDE | IDE licence | `get_file_problems`, `rename_refactoring` | No |
| Copilot coding agent | LLM + CodeQL/secret/dependency validation | Cloud | Copilot plans | Self-validates before finishing | No |
| Aider | Ranked repo map, auto-lint, `--auto-test` | Local | OSS | Lints each edit; tries to fix test failures | No |

Notes:

- **GitNexus** groups impact into "WILL BREAK" and "LIKELY AFFECTED", maps a
  diff to a risk level, and has `shape_check` (API response shape against
  consumer property accesses). Ideas to take: graph context injected into
  the agent's own searches, confidence-tiered impact, response-shape checks.
- **code-review-graph** reports test gaps in its PR comment and can fail the
  merge. Ideas to take: the merge gate and the test-gap list.
- **Serena, Moderne, JetBrains** offer deterministic rename and move tools.
  Tessera's design keeps edits with the agent; a deterministic edit plan
  (`tessera plan`) gives the agent the same completeness without Tessera
  writing code.
- **Augment, Serena** publish agent-outcome evaluations (with and without
  the tool; Augment's "80%" is a vendor claim). Tessera's refactor eval is
  built but not run.

Sources: <https://github.com/abhigyanpatwari/GitNexus> (and its docs/mcp-reference.md, docs/editor-setup.md, docs/cli-reference.md); <https://github.com/tirth8205/code-review-graph>; <https://github.com/CodeGraphContext/CodeGraphContext>; <https://github.com/bartolli/codanna>; <https://github.com/oraios/serena>; <https://sourcegraph.com/docs/api/mcp>; <https://sourcegraph.com/pricing>; <https://augmentcode.com/blog/context-engine-mcp-now-live>; <https://docs.moderne.io/user-documentation/agent-tools/mcp/overview>; <https://code.claude.com/docs/en/plugins/code-intelligence>; <https://jetbrains.com/help/idea/mcp-server.html>; <https://github.blog/changelog/2025-10-28-copilot-coding-agent-now-automatically-validates-code-security-and-quality>; <https://github.blog/changelog/2026-06-09-security-validation-for-third-party-coding-agents>; <https://aider.chat/docs/repomap.html>; <https://aider.chat/docs/usage/lint-test.html>; <https://cursor.com/docs/context/semantic-search>.

## AI pull-request review

| Tool | Approach | Model | Agent loop |
| --- | --- | --- | --- |
| CodeRabbit | LLM + code graph + analyzers | SaaS, self-host on Enterprise; $24–72/user/mo, free on public repos | CLI `--agent` NDJSON; Claude Code plugin fix loop |
| Greptile | Parsed graph + LLM | SaaS, self-host on Enterprise; credits | "Fix in Claude Code/Codex/Cursor"; multi-repo context |
| Qodo | Multi-agent LLM + context engine | SaaS, on-prem on Enterprise | Pre-PR review; cross-repo, cross-provider |
| Cursor Bugbot | LLM review, Autofix (max 3 attempts per PR) | Usage-based since May 2026 | BUGBOT.md rules; MCP on Team+ |
| Copilot code review | Agentic review, skills and MCP (read-only) | Copilot plans | Review → coding agent |

Benchmarks in use: Martian Code Review Bench (Greptile and Baz each report
first place on a different metric; vendor claims), GitHub ReviewBench (219
PRs, 19 languages), SWE-bench Verified. None measures structural breaks
caught before an agent reports done.

Ideas to take: one-click handoff from a finding to the agent; NDJSON
output for agent loops; a merge gate; GitLab and Bitbucket support.

Sources: <https://coderabbit.ai/pricing>; <https://docs.coderabbit.ai/changelog>; <https://docs.coderabbit.ai/cli/claude-code-integration>; <https://greptile.com/docs/changelog>; <https://qodo.ai/pricing>; <https://cursor.com/docs/bugbot>; <https://github.blog/changelog/2026-07-29-copilot-code-review-agent-skills-and-mcp-now-generally-available>; <https://github.blog/2026-01-14-reviewbench-an-open-benchmark-for-ai-code-review>.

## Contract and breaking-change tools

| Tool | What it checks | Model | Idea for Tessera |
| --- | --- | --- | --- |
| Buf | `buf breaking` on protos against git or a registry; json, junit, github-actions, gitlab-code-quality output; `buf lsp serve` | OSS CLI; BSR paid | Field-level proto diff; approve/reject for intended breaks |
| oasdiff | OpenAPI diff with ERR/WARN levels, `--fail-on` | Apache-2.0 | Severity tiers; ingest its JSON |
| GraphQL Inspector | Schema diff (breaking/dangerous/safe), operation validation | MIT | Validate indexed client operations |
| Apollo GraphOS | Schema checks against real client operations | SaaS | Rank breaks by usage |
| Pact / PactFlow | Consumer-driven contracts; `can-i-deploy` | OSS broker; PactFlow paid | Gate keyed to what is deployed where |
| AsyncAPI CLI, Confluent Schema Registry | Event payload diff; BACKWARD/FORWARD/FULL modes | OSS / platform | Payload compatibility for mapped topics |
| Optic | OpenAPI from traffic | Archived 2026-01-12 | Spec-to-code drift |

Tessera's V5 checks that a route, RPC or topic still exists while something
uses it. The gap is field-level change: a removed required field, a renamed
enum value, a changed type. Joined with Tessera's consumer map, a field-level
finding can name the consumer at file:line, which spec diff tools cannot.

Sources: <https://buf.build/docs/reference/cli/buf/breaking>; <https://github.com/oasdiff/oasdiff>; <https://github.com/graphql-hive/graphql-inspector>; <https://apollographql.com/docs/graphos/platform/schema-management/checks>; <https://docs.pact.io/pact_broker/can_i_deploy>; <https://docs.confluent.io/platform/current/schema-registry/fundamentals/schema-evolution.html>; <https://github.com/opticdev/optic>.

## Migration safety

Atlas classifies destructive, data-dependent and backward-incompatible
changes, but `migrate lint` is Pro-only since v0.38. Squawk (Postgres,
Rust, Apache-2.0/MIT), strong_migrations (Rails) and Bytebase (server) lint
SQL; none knows which application code still reads a column. Tessera
already indexes migrations, models and SQL in code, so a migration check
joined with usage ("this drop breaks `OwnerRepository.findByLastName` at
file:line") is an open gap.

Sources: <https://atlasgo.io/lint/analyzers>; <https://atlasgo.io/versioned/lint>; <https://github.com/sbdchd/squawk>; <https://github.com/ankane/strong_migrations>.

## Test selection

| Tool | Granularity | Basis | Idea for Tessera |
| --- | --- | --- | --- |
| Nx affected, Turborepo | Task / package | Build graph inputs | `--explain` and a reason per selected test |
| Bazel + target-determinator | Target | Hashes at two commits | Cache key: tree SHA + tool version + config |
| Develocity PTS, CloudBees Smart Tests | Test | ML on history (paid) | Always run new, changed, failed and flaky tests |
| Datadog TIA | Test | Per-test coverage | "Tracked files" force a full run; a commit-message escape hatch |
| Harness TI | Test | Call graph from an agent | Show which changed method selected each test |
| Jest, Vitest, pytest-testmon, Go cache | File / test | Static graph or coverage | Output formats each runner takes |

Tessera's edge: symbol-level, deterministic, polyglot, crossing service
boundaries through contracts, with no build system, history or
instrumentation required. Selected tests need a reason and safety rails
before teams trust skipped tests.

Sources: <https://nx.dev/docs/features/ci-features/affected>; <https://turborepo.dev/docs/reference/query>; <https://github.com/bazel-contrib/target-determinator>; <https://docs.develocity.ai/predictive-test-selection>; <https://docs.datadoghq.com/tests/test_impact_analysis>; <https://developer.harness.io/docs/continuous-integration/use-ci/run-tests/ti-overview>; <https://jestjs.io/docs/cli>; <https://vitest.dev/guide/cli>.

## Architecture rules

dependency-cruiser (JS/TS, with a baseline of known violations), ArchUnit
(Java, with frozen violations), import-linter (Python contracts), go-arch-lint,
deptrac (PHP), Konsist (Kotlin) and Nx module boundaries each cover one
language or one build system. SonarQube Server 2026.4 adds an automatic
dependency map with "Generate draft" for intended architecture (commercial
editions); Sonargraph exposes rules to agents through an MCP server. One
rules file across languages and service contracts, checked in agent hooks,
is unclaimed. Adoption depends on two things the leaders have: drafting rules
from the current graph, and a baseline so only new violations block.

Sources: <https://github.com/sverweij/dependency-cruiser>; <https://github.com/TNG/ArchUnit>; <https://import-linter.readthedocs.io>; <https://github.com/fe3dback/go-arch-lint>; <https://community.sonarsource.com/t/architecture-analysis-in-sonarqube-server-2026-4/186120>; <https://hello2morrow.com/products/sonargraph>.

## Static analysis, code search, catalogs, runtime

- **Semgrep** ships `semgrep mcp` and Claude Code/Cursor hooks that pull an
  organisation's rules (Feb 2026); CE is single-file. **CodeQL** caches an
  overlay base for PR analysis. **ast-grep** (MIT) has an experimental MCP.
  **SCIP** has indexers for TypeScript, Java, Python, C/C++, Ruby, .NET and
  Rust, and moved to open governance in March 2026 (*unverified detail*).
- **Backstage** models `providesApis`, `consumesApis` and `dependsOn` by
  hand; Cortex, OpsLevel and Port expose catalogs over MCP. Tessera can emit
  `catalog-info.yaml` from code.
- **OpenTelemetry's service-graph connector** emits
  `traces_service_graph_request_total` with client and server labels: the
  cheapest runtime input for an overlay. Datadog's Service Map ages edges out
  after 30 days without traffic.
- **CODEOWNERS:** last match wins; codeowners-validator checks ownership and
  syntax.

Sources: <https://github.com/semgrep/semgrep>; <https://docs.semgrep.dev/release-notes/february-2026>; <https://github.com/github/codeql-action/blob/main/CHANGELOG.md>; <https://github.com/ast-grep/ast-grep-mcp>; <https://github.com/sourcegraph/scip>; <https://backstage.io/docs/features/software-catalog/descriptor-format>; <https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/connector/servicegraphconnector/README.md>; <https://docs.datadoghq.com/tracing/services/services_map>; <https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners>; <https://github.com/mszostok/codeowners-validator>.

## Where Tessera is ahead

1. A deterministic gate at "done" across eight agents, local, under a
   permissive licence.
2. Polyglot service contracts beyond HTTP (gRPC in six languages, GraphQL,
   Kafka, SQL, config keys), joined across repositories, from code alone.
3. Accuracy anyone can check: precision against SCIP ground truth and
   hand-labelled contract links, with a held-out corpus.
4. One tool for before the edit, after the edit, and the pull request.

## Risks

- Agents and compilers cover missing names and arity in typed languages.
  Tessera must lead on cross-service, cross-repo, tests and dynamic
  languages.
- Build graphs (Nx, Turborepo, Bazel) own test selection where adopted;
  coverage-based selectors own it where history exists.
- Spec-first teams have Buf, oasdiff, Apollo and Pact; Tessera's edge is
  code-first teams and the consumer join.
- Sonar covers architecture rules for its commercial customers.
- Observability vendors (Datadog MCP and Service Map) can give runtime blast
  radius to teams that already pay for them.

## What the roadmap takes from this

Ranked by value to developers working with agents; ROADMAP.md places each.

1. Field-level contract diff joined to consumers; ingest `buf breaking` and
   `oasdiff` JSON.
2. Explainable `affected`: a reason per test, always-run rules, full-run
   triggers and an escape hatch.
3. A baseline of known findings, so only new problems block on legacy code.
4. An LSP server that publishes `verify` diagnostics (editors and Claude
   Code's LSP plugin channel at once).
5. Graph context in the agent's own tools through hooks (the callers of a
   symbol the agent greps for).
6. Migration checks joined with code usage, open source.
7. Architecture rules drafted from the current graph, with a pre-edit "may A
   use B?" query.
8. Accepted-break records keyed by finding fingerprint, instead of
   disabling a check.
9. GraphQL operation validation against the schema.
10. CODEOWNERS impact and validation.
11. Base-plus-overlay index cache keyed by tree, tool version and config.
12. A cross-repo merge gate: a provider change checked against its consumers
    at their default branches.
13. Traffic-weighted impact from OpenTelemetry service-graph metrics.
14. Event payload compatibility for mapped topics.
15. Spec-to-code drift: routes in code missing from OpenAPI, and the reverse.

Also: the agent-outcome benchmark (refactor eval, with and without
Tessera), a merge gate in the Action, GitLab/Bitbucket reports, NDJSON
output, a deterministic `tessera plan`, Backstage export, SCIP import, and
hook targets for Windsurf/Devin, Kiro and Junie as their contracts allow.
