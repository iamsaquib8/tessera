# Roadmap

**Tessera's job:** your agent says done, Tessera checks. It gives a
deterministic blast radius before every edit and verification after it,
across files, repos, routes, topics, and tables. It runs locally, uses no
LLM, and is Apache-2.0.

Every milestone ships with proof: a test suite, a published number, or a
demo. Nothing is claimed until it can be reproduced. The full design is in
[docs/design/2026-10-21-overhaul.md](docs/design/2026-10-21-overhaul.md).

| Version | Focus | Primary User Benefit | Status |
| --- | --- | --- | --- |
| v1.0 | **Visible & Stable** | Impact UI, GitHub Action PR comments, frozen 1.x CLI/MCP contracts | released (1.0.1) |
| v1.1 | **Contract Map Completed** | Express mounts, GraphQL, config keys, cross-repo `path`, Go/Python/Ruby web routes | released (1.1.0) |
| v1.2 | **Team & CI Acceleration** | Targeted test selection (`affected` for Jest/Pytest/Cargo/Go), shared commit cache, SARIF | next |
| v1.3 | **Python & Go Deep Tier + `plan`** | Compiler-grade accuracy for Python/Go, proactive refactor planning for agents | planned |
| v1.4 | **Architecture Guardrails & Fixes** | Custom layering rules (`rules.toml`), machine-readable suggested fixes in hooks | planned |
| v1.5 | **Modern Full-Stack & Schema Safety** | Next.js Server Actions, tRPC, migration-to-query verification, CODEOWNERS impact | planned |
| v2.0 | **Telemetry, Lineage & Extensibility** | OpenTelemetry trace overlays, SQL column lineage, custom WASM/SCM plugins | future |

---

## Shipped Foundations (v0.10 – v1.0)

The core architecture, precision resolver, edit-safety hooks, and initial contract map shipped in `v1.0.0` (with patch `v1.0.1`):

- **Foundation (v0.10):** Tree-sitter 0.26 parser with iterative walks, automatic discovery respecting `.gitignore`, zero-setup incremental SQLite indexing, dual-era MCP server (`2025-06-18` + `2024-11-05`), and verified distribution channels (npm, Homebrew, `curl \| sh`, cargo-binstall, Docker).
- **Precision Resolver (v0.11):** Import- and scope-aware resolution for TypeScript, TSX, JavaScript, and Java. Overloads by arity, receiver type inference, confidence scoring on every edge, and [tessera-bench](https://github.com/iamsaquib8/tessera-bench) scoring precision (0.998 TS/JS, 0.995 Java) against SCIP compiler ground truth ([docs/accuracy.md](docs/accuracy.md)).
- **Edit-Safety Loop (v0.12):** `blast --diff` for uncommitted changes, `verify` checking for broken calls, argument mismatches, and dangling imports. Native edit hooks for Claude Code, Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment, and Antigravity ([docs/hooks.md](docs/hooks.md)), plus `init --git-hook` pre-commit backstop.
- **Cross-Repo Contracts (v0.13):** Multi-repository workspaces with cross-service facts for gRPC, HTTP routes, Kafka topics, SQL migrations, and ORMs (Prisma, TypeORM, Sequelize, JPA, Flyway).
- **Visibility & Stability (v1.0):** `tessera ui` local change-impact server with static export (`--export`), GitHub Action PR commenting with job summary fallbacks, and frozen 1.x CLI, MCP, and JSON contracts pinned by `tests/stability.rs` ([docs/stability.md](docs/stability.md)).

---

## v1.1: The Contract Map, Completed

Everything needed to trace requests across real-world microservices and workspaces ([docs/contracts.md](docs/contracts.md)):

- **Mounted Routers:** Express routers mounted under prefixes (`app.use("/api", router)`), resolved across imports and file boundaries.
- **Go, Python & Ruby Web Stacks:** Route declarations and HTTP clients for Go (`net/http`, chi, gin, echo), Python (FastAPI, Flask, Django, `requests`, `httpx`), and Ruby (Rails `routes.rb` with `resources`, `Net::HTTP`, Faraday, HTTParty).
- **GraphQL:** Root fields in schemas, Apollo and NestJS resolvers, and client operations in `.graphql` and `gql` templates (`tessera map graphql`).
- **Config Keys:** Cross-service config binding (`.env`, Dockerfiles, Docker Compose, Spring properties) linked to environment readers in every supported language (`tessera map config`).
- **Cross-Repository Paths:** `tessera path <from> <to>` traverses from a client in one repository to the route handler in another through contracts.

**Gate & Result:** Hand-labelled routes and topics on `opentelemetry-demo`, `spring-petclinic-microservices`, and the held-out `bank-of-anthos` corpus achieved zero wrong links ([docs/contracts.md#accuracy](docs/contracts.md#accuracy)).

---

## v1.2: Team and CI (Next)

Cut CI times on large monorepos and bring Tessera findings directly into team code review workflows.

- **`tessera affected <rev>` for Test Selection:**
  - Selects only the tests reachable by the modified symbols or contracts.
  - Native format flags for test runners:
    - `--format jest` (`--findRelatedTests`)
    - `--format vitest` (`--related`)
    - `--format pytest` (`test_file.py::test_fn`)
    - `--format go-test` (`./pkg/... -run TestName`)
    - `--format cargo` (`-p <pkg> --test <name>`)
    - `--format json` (for custom scripting)
  - Monorepo package/workspace boundary filtering compatible with `nx`, `turbo`, and `pnpm`.
- **Zero-Infra Shared Index Cache:**
  - Index cache keyed by commit tree, shared between CI runs and team members.
  - Built-in GitHub Action cache (`actions/cache`), optional cloud bucket storage (`--cache-bucket s3://...` / GCS), and local directory mounts.
  - Fast fallback to the nearest common ancestor (`git merge-base`) for sub-second incremental index refreshes on branch commits.
- **Universal CI Reporting & Code Scanning:**
  - SARIF 2.1.0 output uploaded to GitHub Code Scanning for inline pull request annotations.
  - Native exit-code and report formats for GitLab (Code Quality & SAST), Azure DevOps, Bitbucket, Buildkite, and Jenkins.

**Gate:** On three public monorepos, `affected` selects 100% of tests that fail after a seeded breaking change, while skipping at least 50% of the full test suite on typical pull requests.

---

## v1.3: Python & Go Deep Tier + Agent Refactor Planning

Elevate Python and Go to compiler-grade precision, and give agents the ability to plan refactors before touching code.

- **Python Deep Tier:**
  - Import resolution across packages, PEP 420 namespace packages, and virtual environments (`pyproject.toml`, poetry, uv).
  - Type-hint receiver inference, `@dataclass` and Pydantic field resolution.
  - Benchmark against `mypy` and `pyright` ground truth in `tessera-bench`.
- **Go Deep Tier:**
  - Package-qualified resolution, Go modules, struct methods, and interface implementation satisfaction edges.
  - Benchmark against `gopls` ground truth in `tessera-bench`.
- **`tessera plan` (Agent Refactor Planner):**
  - Proactive planning commands for agents: `tessera plan rename <old> <new>`, `plan move <file> <dest>`, `plan signature <symbol> <new-sig>`.
  - Emits an ordered, deterministic edit manifest so the agent edits every call site without grepping or guessing.
- **Actionable Remediation Hints in `verify`:**
  - High-confidence findings in JSON and hook payloads include suggested replacement diffs/patches (e.g. arity corrections, missing import statements).
  - Cuts down agent retry loops from 3–4 turns to a single turn.

**Gate:** High-confidence precision ≥ 0.95 and recall ≥ 0.85 on public Python and Go benchmark repositories in `tessera-bench`.

---

## v1.4: Architecture Guardrails & Loop Breaker

Turn architectural boundaries into instant, sub-millisecond local and CI guardrails.

- **Architecture Rules as Code (`.tessera/rules.toml`):**
  - Declarative boundary rules enforced by `tessera verify`, hooks, and CI:
    - Layering constraints (e.g. `domain/**` cannot import `infrastructure/**`).
    - Cross-service data isolation (e.g. `services/billing/**` cannot import `services/auth/db/**`).
    - Circular dependency prevention across packages and modules.
    - Forbidden or deprecated API calls.
- **Agent Loop Breaker in Hooks:**
  - Detects when an agent gets stuck in cyclic failed repairs across consecutive edit turns.
  - Injects a consolidated multi-site diff context and suggested fix summary, or prompts an escape hatch (`--allow-incomplete`).

**Gate:** Zero latency regression on single-file verification (<50 ms); catches 100% of seeded rule violations across monorepo test fixtures.

---

## v1.5: Modern Full-Stack, Migration Safety & Ownership

Bridge modern web RPC patterns, database migration drift, and organizational code ownership.

- **Modern Web RPCs:**
  - **Next.js Server Actions:** Track `"use server"` exported endpoints from backend action files to client components.
  - **tRPC:** Link router procedures (`query`/`mutation`) to frontend `trpc.<proc>.useQuery()` hooks.
  - **Remix / React Router v7:** Connect backend `loader` and `action` functions to client UI consumers.
- **Database Migration Safety (`verify --migrations`):**
  - Cross-check SQL migrations (Flyway, Alembic, Prisma, TypeORM) against active queries in application code.
  - Catch dropped or renamed columns and tables that are still selected in code before landing in production.
- **CODEOWNERS Impact Mapping (`blast --owners`):**
  - Parse `.github/CODEOWNERS` and map blast radius directly to team handles.
  - Automatically tag relevant teams in the GitHub Action PR comment.
- **Editor Integration (LSP Bridge & CodeLens):**
  - Lightweight VS Code / Cursor extension providing symbol CodeLens: caller counts, blast radius, and cross-service contract links directly inside the editor.

**Gate:** Zero false-positive route links on modern full-stack benchmark apps; migration verifier catches 100% of unsafe column drops in historical migration fixtures.

---

## v2.0: Runtime Truth, Lineage & Extensibility

Unify static graphs with dynamic telemetry and extensible plugins.

- **OpenTelemetry Runtime Overlays:**
  - Ingest OpenTelemetry traces and span metrics to overlay runtime request traffic on the static contract map.
  - Highlight dead endpoints, unused consumers, and unexpected runtime call paths that static analysis cannot see.
- **Column-Level Data Lineage:**
  - Deep SQL AST parsing to trace individual columns from database migrations through ORM models, API responses, and frontend consumers.
  - Automated PII propagation tracking in `tessera verify`.
- **WASM & Self-Serve Framework Packs:**
  - Load custom Tree-sitter framework queries directly from `.tessera/packs/*.scm` without recompiling the Rust binary.
  - WASM plugin interface for custom language extractors.

---

## Contributing to the Roadmap

Language requests, agent integrations, and framework packs each have an issue
template. A pack or language is a small, self-contained contribution; see
[CONTRIBUTING.md](CONTRIBUTING.md).
