# X / Twitter thread

Eight posts. Post 1 has to work on its own. Each screenshot gets its own
post. X does not accept SVG, so export `assets/demo.svg` to PNG first (open
it in a browser and screenshot it at 2x, or
`rsvg-convert -z 2 -o demo.png assets/demo.svg`).

Each paragraph below is one line, so it pastes as written. Every post is
under 280 characters, counting a link as 23.

---

**1/**

Your coding agent says done. Tessera checks.

A local tool that checks every edit an AI agent makes against an index of the whole repo, and holds the agent at "done" until the callers it broke are fixed.

No LLM. Same code, same answer. Apache-2.0.

github.com/iamsaquib8/tessera

[attach: demo.svg exported to PNG]

**2/**

The failure it targets: an agent greps, opens a few files, renames a method, and reports success. The caller in another module that still uses the old name shows up later, in CI, in review, or in production.

Before an edit: `blast` and `context`. After it: `verify`.

**3/**

Hooks run `verify` after each edit and at the end of the turn. Claude Code's Stop hook blocks with:

V1 src/users/repo.ts:17 `findById` is not defined in src/users/repo.ts; did you mean UserRepo.findOne?

Also Codex, Cursor, Gemini CLI, Copilot CLI and 3 more.

**4/**

It works across services. Rename the Go handler for ShippingService/GetQuote in microservices-demo, and `verify` names the two clients, in checkout and the frontend, that still call it.

`tessera ui` shows the same thing as a local page on 127.0.0.1.

[attach: assets/ui-impact.png]

**5/**

`tessera map` links HTTP routes, gRPC methods, Kafka topics and tables to the code on both sides: Express, NestJS, Next.js, Spring, gRPC in six languages, JPA, Prisma and more.

The contract map draws each service and what runs between them.

[attach: assets/ui-map.png]

**6/**

Measured, not claimed:

- 99.7% of high-confidence references match compiler ground truth (TS/JS/Java)
- gRPC in microservices-demo: 15/15 servers, 34/34 clients, no wrong link
- false-positive hunt: a blind find-and-replace leaves 15 blocking findings, all real

The benchmark is public.

**7/**

Limits: deep resolution is TypeScript, JavaScript and Java. Python, Go, Rust, C, C++, C#, Ruby and PHP resolve within a file and by unique name. It does not type-check, so keep running tsc and your tests.

Full list: github.com/iamsaquib8/tessera/blob/main/docs/when-not-to-use.md

**8/**

Try it in any repo, no setup:

npx tessera-codegraph blast <symbol>
npx tessera-codegraph init

`init` finds which of 21 agents you use and shows a diff before writing. On pull requests: uses: iamsaquib8/tessera/action@v1

Rust, 12 languages, stable 1.x contract.

---

Pin post 1. Answer questions in replies rather than quote posts. Do not ask
for reposts or stars.
