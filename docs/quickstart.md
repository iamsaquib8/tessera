# Quickstart

Tessera gives your coding agent a deterministic map of your code, and checks
the agent's edits against it. This page takes you from nothing to an agent
that is held at "done" until the breakage it caused is fixed. It takes about
five minutes.

## 1. Install

Pick one. All of them install the same single binary.

```sh
curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
npx tessera-codegraph status             # no install; Node 18+
npm i -g tessera-codegraph               # then use `tessera`
cargo binstall tessera-codegraph         # prebuilt, via cargo-binstall
cargo install tessera-codegraph          # from source (Rust 1.90+)
```

A Homebrew tap and a public container image are not available yet.

The install script verifies the release checksum before installing and puts
the binary in `~/.local/bin` (no sudo).

## 2. Connect your agent

Run this inside your repository:

```sh
tessera init
```

`init` detects the agents you use, shows the diff of every file it would
change, and asks before it writes anything. There is no other setup: the first
query builds the index under `.tessera/`, and every later query refreshes only
what changed.

To see what it would do for one agent without writing anything:

```console
$ tessera init --agent claude --dry-run
Agents:
  Claude Code (--agent)

--- /dev/null
+++ .claude/settings.local.json
@@ -0,0 +1,50 @@
…
--- /dev/null
+++ .mcp.json
@@ -0,0 +1,10 @@
…
--- /dev/null
+++ .tessera/config.toml
@@ -0,0 +1,17 @@
…

Notes:
  Claude Code: Claude Code asks you to approve the project's MCP server the first time; the Tessera plugin is an alternative to these files (use one or the other).

Dry run: nothing written.
```

The diffs are shortened here. For Claude Code, `init` adds two things:

- **The MCP server** (`.mcp.json`), so the agent can call `blast`, `context`,
  `find` and the other tools itself.
- **The edit-safety hooks** (`.claude/settings.local.json`): a note before an
  edit that touches code used elsewhere, a check after each edit, and a hold
  at the end of the turn while the agent's breakage is unfixed.

Other agents get the same MCP server, and hooks where their hook system can
carry them. [Connect your agent](guides/connect-your-agent.md) lists what each
one gets and what to do for the ones with no hooks.

For Claude Code you can use the plugin instead of `init`, with no files in
your project: `/plugin marketplace add iamsaquib8/tessera`, then
`/plugin install tessera@tessera`. Use one or the other, not both.

## 3. See it catch a mistake

The commands below need no agent. They use the TypeScript sample in this
repository, copied into a repository of its own so Tessera indexes only the
sample:

```sh
git clone --depth 1 https://github.com/iamsaquib8/tessera
cp -R tessera/tests/fixtures/typescript demo && cd demo
git init -q && git add -A && git commit -qm sample
```

Before an edit, ask who depends on the function you are about to change:

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

Now make the kind of edit an agent makes: give `formatName` a second parameter
and leave its callers alone.

```sh
perl -pi -e 's/formatName\(name: string\)/formatName(name: string, locale: string)/' src/util/format.ts
```

```console
$ tessera verify --changed
✗ verify: 2 problem(s)
  [V2] src/users/repo.ts:7 `formatName` takes 2 arguments; this call passes 1
  [V2] src/util/format.ts:6 `formatName` takes 2 arguments; this call passes 1
Tests to run: test/repo.test.ts
```

`verify` exits with code 1 when it finds a problem, so it works in scripts and
git hooks. With the hooks from step 2, the agent gets this same list after the
edit and again when it tries to finish. Output on this page leaves out the
`indexing …` progress lines, which go to stderr.

## Where next

Pick the task you have:

- [Check what a change affects before you make it](guides/check-before-you-edit.md)
- [Catch what an agent broke after it edits](guides/catch-what-an-agent-broke.md)
- [Run only the tests a change can affect](guides/run-only-affected-tests.md)
- [Review a pull request with a deterministic report](guides/review-a-pull-request.md)

The [guides page](guides/index.md) lists all of them. If something does not
work, see [troubleshooting](troubleshooting.md).
