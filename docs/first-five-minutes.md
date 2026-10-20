# The first five minutes

This walks through the loop Tessera is built for: look before you edit,
check after you edit. It uses the TypeScript fixture in this repository, so
every output below is real.

```sh
git clone https://github.com/iamsaquib8/tessera && cd tessera/tests/fixtures/typescript
```

## 1. Find the code

```console
$ tessera find findById
method UserRepo.findById src/users/repo.ts:6-8
  findById(id: string): string {
```

Misspell it and Tessera says so, instead of guessing silently:

```console
$ tessera find findByIdd
No exact definition of findByIdd; closest matches:
method UserRepo.findById src/users/repo.ts:6-8
  findById(id: string): string {
```

## 2. See the blast radius before editing

```console
$ tessera blast formatName
4 callers affected by formatName (ranked; name-level matching):
  100.0 d2 handler src/api/handler.ts:4
   63.7 d1 UserRepo.findById src/users/repo.ts:6
   63.7 d1 loadUser src/util/format.ts:5
   54.0 d2 loadUser src/users/repo.ts:15
```

Scores come from personalised PageRank over the call graph: callers that many
paths run through rank higher. `d1` is a direct caller.

## 3. Read one bundle instead of five files

```console
$ tessera context findById
method UserRepo.findById src/users/repo.ts:6-8
  findById(id: string): string {
    return formatName(this.lookup(id));
  }
Calls:
  function formatName(name: string): string {  (src/util/format.ts:1)
  private lookup(id: string): string {  (src/users/repo.ts:10)
Called by:
  function loadUser(id: string): string {  (src/users/repo.ts:15)
```

## 4. Check what you (or your agent) wrote

```console
$ echo 'JSON.parse(x); UserRepo.findByIdd(2); loadUsr(3)' | tessera verify --snippet - --language typescript
✗ verify: 2 problem(s)
  [V1] `findByIdd` is not defined anywhere in the index; did you mean UserRepo.findById? (line 1)
  [V1] `loadUsr` is not defined anywhere in the index; did you mean loadUser? (line 1)
```

`JSON.parse` is not flagged: standard globals and calls on receivers Tessera
cannot type are skipped rather than guessed. `verify` exits with code 1 when
it finds problems, so it works in scripts and git hooks.

## 5. Give it to your agent

```sh
claude mcp add tessera -- tessera mcp
```

The MCP server answers immediately, indexes in the background, and re-indexes
files as they change, so the agent sees its own edits on the next call. See
[integrations](integrations.md) for other agents and [MCP](mcp.md) for the
tools.
