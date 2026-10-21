---
title: "Your coding agent says done. Here is a deterministic way to check"
published: false
description: "A walkthrough of Tessera 1.0: the blast radius before an AI agent's edit, verification after it, and the same check across services, with no model in the loop."
tags: ai, devtools, opensource, rust
cover_image: https://raw.githubusercontent.com/iamsaquib8/tessera/main/assets/social-preview.png
---

> Before publishing: set `published: true`. The images load from `main`, so
> merge the 1.0 branch first and check that each one renders in the preview.

Coding agents edit code they have only partly read. An agent greps, opens a
few files, renames a method, and reports success. The caller in another
module that still uses the old name turns up later: in CI, in review, or in
production.

The usual fixes are to read more of the repository into the context window,
or to ask a second model to review the change. I wanted something else: a
check that gives the same answer every time, runs on just the changed code,
and can stop the agent before it says "done".

That is [Tessera](https://github.com/iamsaquib8/tessera). Version 1.0 is
out, and this post walks through it on two public codebases, with the real
output.

## No model in the loop

Tessera parses the repository with tree-sitter and stores the result in a
SQLite file in `.tessera/`, gitignored automatically. In TypeScript,
JavaScript and Java, references resolve through imports, re-exports,
tsconfig paths, workspace packages, Java packages and declared types, and
every reference carries a confidence. No model builds the index, nothing
leaves the machine, and the same code always gives the same answer.

There is no setup step. The first query builds the index; later queries
refresh only what changed.

## Before the edit: what does this change reach?

The Tessera repository includes a small TypeScript fixture
(`tests/fixtures/typescript`). Ask what a change to `formatName` affects:

```console
$ tessera blast formatName
5 callers affected by formatName (ranked; all exact):
  100.0 d1 UserRepo.findById src/users/repo.ts:6
  100.0 d1 loadUser src/util/format.ts:5
   82.2 d2 loadUser src/users/repo.ts:15
   36.3 d3 handler src/api/handler.ts:4
   36.3 d3 describe("loadUser").it("returns the user") test/repo.test.ts:4
Tests to run: test/repo.test.ts
```

Callers are ranked by impact. `d1` is a direct caller and `d3` is three
calls away. "All exact" means every caller resolved at high confidence, and
the last line names the test file that reaches the change.

Before editing `findById`, the agent can read one bundle instead of opening
five files:

```console
$ tessera context findById
method UserRepo.findById src/users/repo.ts:6-8
  findById(id: string): string {
    return formatName(this.lookup(id));
  }
Calls:
  UserRepo.lookup  src/users/repo.ts:10  private lookup(id: string): string {
  formatName  src/util/format.ts:1  function formatName(name: string): string {
Called by:
  loadUser  src/users/repo.ts:15  function loadUser(id: string): string {
Tests:
  describe("loadUser").it("returns the user") (test/repo.test.ts:4)
Related (shared callers):
  UserRepo
```

## The edit: the agent renames a method

Now ask the agent to rename `findById` to `findOne`. It changes the method
in `src/users/repo.ts` and is ready to stop. With Tessera's hooks installed,
the edit is checked against the whole index. Run by hand, the same check
looks like this:

```console
$ tessera verify --changed
✗ verify: 1 problem(s)
  [V1] src/users/repo.ts:17 `findById` is not defined in src/users/repo.ts; did you mean UserRepo.findOne?
Tests to run: test/repo.test.ts
```

In Claude Code the finding goes back to the agent right after the edit, and
at the end of the turn the Stop hook refuses to let it finish until line 17
is fixed. The agent fixes the caller, and only then reports done.

The hooks are deliberately conservative. A rename spread over several edits
is never interrupted: after an edit, Tessera blocks only when a problem
survives an edit that fixed nothing. At the end of the turn it blocks while
high-confidence problems remain, at most three times. If Tessera itself
fails, the hook prints nothing and exits 0, so a bug in Tessera never stops
your agent.

The same hooks run in Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code,
Augment and Antigravity. For any other agent, and for people,
`tessera init --git-hook` installs a pre-commit check.

`verify` has six checks with stable ids: V1 calls to names that do not
exist, V2 wrong argument counts, V3 references to something the change
removed, V4 broken imports, V5 a contract removed while a client still uses
it, and V6 the tests that reach the change (advisory, never blocks).

## Across services: a gRPC rename

V5 covers breaks between services, which no compiler sees. Take
[microservices-demo](https://github.com/GoogleCloudPlatform/microservices-demo)
at commit `38e7348`: services in Go, C#, Node, Python and Java that talk
over gRPC. Tessera finds both sides of every contract:

```console
$ tessera map rpc GetQuote
rpc ShippingService/GetQuote
  defined     microservices-demo   protos/demo.proto:108
  defined     src/adservice        src/adservice/src/main/proto/demo.proto:106
  defined     src/currencyservice  src/currencyservice/proto/demo.proto:106
  defined     src/paymentservice   src/paymentservice/proto/demo.proto:106
  served by   src/shippingservice  src/shippingservice/main.go:119  server.GetQuote
  called by   src/checkoutservice  src/checkoutservice/main.go:315  checkoutService.quoteShipping
  called by   src/frontend         src/frontend/rpc.go:88  frontendServer.getShippingQuote
```

Now rename the Go handler, the way an agent tidying up names might:

```sh
sed -i '' 's/func (s \*server) GetQuote(/func (s *server) GetShippingQuote(/' src/shippingservice/main.go
tessera verify --changed
```

```text
✗ verify: 2 problem(s)
  [V5] src/checkoutservice/main.go:315 `ShippingService/GetQuote` is no longer served: the change removed it from src/shippingservice/main.go, and this call still uses it
  [V5] src/frontend/rpc.go:88 `ShippingService/GetQuote` is no longer served: the change removed it from src/shippingservice/main.go, and this call still uses it
```

Both callers live in other services, so nothing in the shipping service's
own files points at the problem.

`map` reads HTTP routes (Express, Fastify, NestJS, Next.js, Spring and
their clients), gRPC in Go, C#, Node, Python, Java and C++, Kafka topics,
and tables (SQL migrations, JPA, TypeORM, Sequelize, Prisma).
`tessera path` follows those contracts as call edges, so a path can run
from a client in one service into a handler in another.

## See it: `tessera ui`

`tessera ui --open` serves a page on 127.0.0.1 with the impact of the
working tree: problems to fix, changed symbols, callers by impact,
contracts with services on the other side, and tests to run.

![Change impact for the renamed handler: two V5 problems to fix](https://raw.githubusercontent.com/iamsaquib8/tessera/main/assets/ui-impact.png)

The contract map draws one tile per service, linked by the routes, gRPC
methods and topics between them, and lists every contract, tables
included.

![The contract map of microservices-demo](https://raw.githubusercontent.com/iamsaquib8/tessera/main/assets/ui-map.png)

Nothing loads from the network, and `tessera ui --export site/` writes the
same page as a static site.

## On every pull request

```yaml
- uses: actions/checkout@v4
- uses: iamsaquib8/tessera/action@v1
  with:
    fail-on: blocking   # or `never` (default) to only comment
```

The Action posts one comment per pull request and updates it on every push:
problems to fix, contracts touched, callers of the changed code, and tests
to run. On forks it writes the job summary instead. Any other CI can run
`tessera verify --diff <base> --format markdown`.

## How accurate is it?

Measured against compiler ground truth (scip-typescript and scip-java) on
excalidraw, nest, Ghost and two Spring applications:

- 99.7% of high-confidence references are correct.
- In TypeScript/JavaScript, Tessera finds 99% of direct calls, 91% of calls
  on typed receivers, and 99% of type references.
- gRPC links in microservices-demo: 15 of 15 servers, 34 of 34 clients, no
  wrong link. On the held-out opentelemetry-demo, the first measurement
  found 10 of 12 servers and 16 of 26 clients, also with no wrong link. Those
  gaps are closed now, so it no longer counts as held out.
- To hunt false positives, a blind find-and-replace over the corpus was
  run until every blocking finding it left was a real break: 15 of 15.
- Lookups take under 50 ms at p95 on 10,000 files, and a 45,000-file
  repository indexes in under a minute.

The benchmark and its ground truth are public in
[tessera-bench](https://github.com/iamsaquib8/tessera-bench).

## Where it falls short

- Deep resolution covers TypeScript, TSX, JavaScript and Java. Python, Go,
  Rust, C, C++, C#, Ruby and PHP resolve within a file and by unique name,
  and the hooks do not show findings in those languages.
- Receivers typed through chained calls or getters are not followed.
  That is why Ghost's typed-receiver recall is 0.698.
- Dynamic dispatch, reflection, string-based calls, dependency injection by
  name, and code generated at build time are invisible to it.
- Express routers mounted under a prefix are not followed yet, and HTTP
  frameworks in Go, Python and Ruby, plus GraphQL, come in later packs.
- It does not type-check. Keep `tsc`, `javac` and your tests.

## Try it

```sh
npx tessera-codegraph blast <symbol>   # any repository; indexes on first use
npx tessera-codegraph init             # wire it into your agents
```

`init` detects which of 21 agents a project uses, shows the diff of every
config change, and asks before writing. Claude Code users can install the
plugin instead (`/plugin marketplace add iamsaquib8/tessera`). Commands,
flags, `--json` fields, MCP tool schemas, hook replies and exit codes are
stable across 1.x.

Next on the roadmap: test selection in CI, architecture rules checked at
every edit, a SCIP-based precise mode, and editor extensions. If Tessera
gets something wrong on your code, an issue with the snippet is the most
useful thing you can send: [github.com/iamsaquib8/tessera](https://github.com/iamsaquib8/tessera).
