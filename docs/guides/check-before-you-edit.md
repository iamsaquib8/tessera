# Check before you edit

You are about to change a function, and you do not know what else depends on
it. Or your agent is, and it has opened three files and guessed.

The examples use the TypeScript sample from the [quickstart](../quickstart.md).

## See who a change affects

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

`d1` is a direct caller, `d2` calls a caller, and so on. The score ranks the
callers that many call paths run through above those that only one does.
"all exact" means every caller was resolved to this function with high
confidence; weaker matches are labelled `likely` or `possible`.

To check a whole change instead of one symbol, give `blast` a revision. It
finds the symbols the change edited, then ranks everything that reaches them:

```console
$ tessera blast --diff HEAD
Changed symbols in HEAD..working tree:
  formatName src/util/format.ts:1
5 callers affected by HEAD..working tree (ranked; all exact):
  100.0 d1 UserRepo.findById src/users/repo.ts:6 via formatName
  100.0 d1 loadUser src/util/format.ts:5 via formatName
   82.2 d2 loadUser src/users/repo.ts:15 via formatName
   36.3 d3 handler src/api/handler.ts:4 via formatName
   36.3 d3 describe("loadUser").it("returns the user") test/repo.test.ts:4 via formatName
Tests to run: test/repo.test.ts
```

## Read one bundle instead of five files

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

`context` gives the body, what it calls, what calls it and the tests that
reach it, within a size budget.

## Let the agent do it

Once your agent is connected ([connect your agent](connect-your-agent.md)),
it calls `blast` and `context` as MCP tools. In agents with a pre-edit hook,
Tessera also adds a short note before an edit to a function that has callers
in other files (at most 600 characters, once per function per session):

```text
Tessera: this edit touches code used from other files.
- `formatName` (src/util/format.ts:1) has 1 use(s) in other files: src/users/repo.ts:7; tests: test/repo.test.ts.
Keep those call sites working, or update them in the same change.
```

## What it does not cover

- Calls Tessera cannot follow: a receiver typed by a chained call
  (`data.key("x").index(0)`), dynamic dispatch, reflection and string-based
  calls. Those callers are missing, not wrong.
- Precision is measured, not promised: 99.8% of high-confidence references
  are right in TypeScript and JavaScript, 99.5% in Java. See
  [accuracy](../accuracy.md) and [when not to rely on Tessera](../when-not-to-use.md).
- A change to a route, topic or table reaches other services through
  contracts, not calls; use [`map`](block-breaking-api-changes.md).

## Next

- [Catch what an agent broke after it edits](catch-what-an-agent-broke.md)
- Reference: [MCP tools](../mcp.md), [accuracy](../accuracy.md)
