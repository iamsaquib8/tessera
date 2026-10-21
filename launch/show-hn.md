# Show HN

Post only after v1.0.0 is tagged and every install path prints
`tessera 1.0.0` (RUNBOOK, Phase 0).

## Title (pick one; 80 characters or fewer, no emoji)

1. `Show HN: Tessera – Your coding agent says done. Tessera checks the edit`
2. `Show HN: Tessera – Deterministic edit checks for AI coding agents, no LLM`
3. `Show HN: Tessera – Find the callers your coding agent's edit broke, locally`

Recommended: #2. It says what the tool is and what it is not, and "no LLM"
answers the first question most readers will have.

## URL

https://github.com/iamsaquib8/tessera

## Body

(Post as the text. If HN shows only the URL, post it as the first comment
instead, followed by the comment below.)

Tessera is a local tool that checks the edits an AI coding agent makes.
Before an edit, it tells the agent what the change reaches. After the edit,
it checks the change against an index of the whole repository and, through
the agent's own hook system, holds the agent at "done" until the breakage
is fixed.

The problem I kept running into: an agent greps, opens a few files, renames
a method, and reports success. The caller in another module that still uses
the old name turns up later, in CI, in review, or in production.

What it does:

- `tessera blast <symbol>` (or `--diff <rev>`) ranks the callers a change
  can affect and lists the tests to run. `tessera context <symbol>` returns
  the body, callers, callees and tests in one budgeted read.
- `tessera verify --changed` checks a change for calls to names that do not
  exist (V1), wrong argument counts (V2), references to something the change
  removed (V3), broken imports (V4), a route, gRPC method or topic removed
  while a client still uses it (V5), and the tests that reach the change
  (V6, advisory). It exits 1 on a blocking problem.
- Hooks run `verify` after each edit and at the end of the turn in Claude
  Code, Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment and
  Antigravity. Every other agent gets a git pre-commit hook. `tessera init`
  detects which of 21 agents a project uses, shows the diff of every config
  change, and asks before writing.
- `tessera map` links HTTP routes, gRPC methods, Kafka topics and tables to
  the code on both sides (Express, Fastify, NestJS, Next.js, Spring, gRPC in
  six languages, JPA, TypeORM, Sequelize, Prisma). Rename the Go handler for
  `ShippingService/GetQuote` in GoogleCloudPlatform's microservices-demo and `verify`
  names the two clients, in checkoutservice and the frontend, that still
  call it.
- `tessera ui` serves a change-impact page and a contract map on
  127.0.0.1, and a GitHub Action posts the same report on pull requests.

No model builds the index: tree-sitter parses, SQLite stores, and the same
code gives the same answer every time. Nothing leaves the machine. It is
written in Rust, reads 12 languages, and is Apache-2.0.

Numbers, from a public benchmark (tessera-bench) you can rerun:

- High-confidence references match compiler ground truth (scip-typescript,
  scip-java) 99.7% of the time on excalidraw, nest, Ghost and two Spring
  apps. Recall in TS/JS: 99% of direct calls, 91% of calls on typed
  receivers, 99% of type references.
- gRPC links on microservices-demo: 15 of 15 servers and 34 of 34 clients,
  no wrong link. On the held-out opentelemetry-demo, the first measurement
  found 10 of 12 servers and 16 of 26 clients, again with no wrong link.
  Those gaps are fixed now, so that repository no longer counts as held out.
- To hunt false positives I ran a blind find-and-replace over the benchmark
  corpus and fixed every wrong finding it raised. It now leaves 15 blocking
  findings, all real breaks.
- Lookups take under 50 ms at p95 on 10,000 files; a 45,000-file repository
  indexes in under a minute.

The limits: deep resolution (imports, re-exports, tsconfig paths, declared
types, overloads) covers TypeScript, JavaScript and Java only. Python, Go,
Rust, C, C++, C#, Ruby and PHP resolve within a file and by unique name, and
the hooks block only on high-confidence TS/JS/Java findings. Receivers typed
through chained calls or getters, dynamic dispatch, reflection and DI by
name are not followed, and Express routers mounted under a prefix are not
followed yet. It does not type-check, so keep running tsc and your tests.
The full list is in docs/when-not-to-use.md.

Try it in any repository (it indexes on first use):

    npx tessera-codegraph blast <symbol>
    npx tessera-codegraph init

1.0 also fixes the 1.x contract: commands, flags, `--json` fields, MCP tool
schemas, hook replies and exit codes stay stable, and a test suite pins
them.

## First comment (post right after the submission)

Author here. Two things I would like to hear views on:

1. Where to draw the blocking line. Today the hooks block only on
   high-confidence findings in TS/JS/Java, report first and block only when
   the next edit leaves the problem in place (so a rename that spans several
   edits can finish), and give up after three blocks per turn. Everything
   weaker is left to `tessera verify --changed`. If you run agents with
   hooks: would you rather it blocked more often and was sometimes wrong, or
   stayed quiet unless it is sure?

2. Static contracts versus runtime truth. `map` reads routes, gRPC methods,
   topics and tables from source, so anything wired up dynamically is
   invisible to it. The roadmap overlays OpenTelemetry traces on the static
   graph. If you run several services, what would you trust for "who calls
   this endpoint": the static map, traces, or a declared spec (OpenAPI,
   .proto) as the source of truth?

Happy to go into the resolver, the framework packs (tree-sitter queries),
or how the hooks fail open when Tessera itself has a problem.

## Questions to expect, with the short answer

- **Why not tsc or an LSP?** Run them too. Tessera checks only the changed
  code against an index of the whole repository, so it fits in an agent's
  edit loop, works the same way in all 12 languages (including JavaScript,
  Python and Ruby projects with no type checker), and sees breaks between
  services that no compiler sees.
- **Is it an LLM?** No. Parsers build the index; the accuracy numbers come
  from compiler ground truth.
- **Does code leave my machine?** No. The index is a SQLite file in
  `.tessera/`, gitignored, and Tessera makes no network calls.
- **What if Tessera breaks?** A hook that hits a missing or busy index, or
  any failure, prints nothing and exits 0. It never stops the agent.
- **Does it slow the agent down?** Through MCP the server keeps the index
  fresh in the background. A CLI query first runs a freshness check, a stat
  walk under 100 ms on 10,000 files.

## Timing and conduct

- Post Tuesday to Thursday, 8:00 to 10:00 AM US Eastern. Avoid weekends and
  US holidays.
- Stay for the first two to three hours and answer every comment with
  specifics. When someone finds a limit, agree, link the line in
  docs/when-not-to-use.md or docs/contracts.md, and say whether it is on the
  roadmap.
- Do not ask anyone to upvote, and do not send people a direct link to the
  HN post; HN penalises voting rings. Share the repository, not the thread.
- If the post gets no traction, HN allows a repost after some time; change
  the title angle rather than reposting the same text.
