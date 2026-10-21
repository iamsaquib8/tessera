# Reddit: r/ClaudeAI

Audience: people who run Claude Code every day and know what hooks and MCP
are. Lead with the plugin and what each hook does, show a real hook reply,
and state the limits before someone asks.

## Title

`Tessera 1.0: a Claude Code plugin that checks each edit and blocks Stop while a caller Claude broke is still broken`

## Body

I kept watching Claude Code rename a method, update the file it had open,
and say done, while a caller in another directory still used the old name.
Tessera is a deterministic check for that, wired into Claude Code's hooks.

Install it as a plugin (nothing is added to your project):

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

Or run `npx tessera-codegraph init`, which shows the diff of every config
change and asks before writing. Use the plugin or `init`, not both. The
plugin runs `tessera` 1.0.0 from your PATH, or through npx if it is not
installed.

**What the hooks do**

- **Session start:** records what was already changed and refreshes the
  index in the background. Never blocks.
- **Before an edit:** if the function has callers in other files, Claude
  gets a short note with those callers and the tests that reach them. Once
  per function per session.
- **After an edit:** re-indexes the edited files and runs `verify` over
  every file edited so far, and the problems go back to Claude as context.
  It blocks only when a problem is still there after an edit that fixed
  nothing, so a rename spread over several edits is never interrupted.
- **Stop:** runs `verify` over everything changed in the session. While a
  high-confidence problem remains, Claude cannot finish. At most three
  blocks per turn.

This is the actual Stop reply when Claude renames `UserRepo.findById` to
`findOne` and misses one caller (the TypeScript fixture in the repository):

```text
tessera verify: 1 problem(s) to fix
V1 src/users/repo.ts:17 `findById` is not defined in src/users/repo.ts; did you mean UserRepo.findOne?
Tests that reach the change: test/repo.test.ts
Fix these before you finish.
```

The checks: V1 calls to names that do not exist, V2 wrong argument counts,
V3 references to something the change removed, V4 broken imports, V5 a
route, gRPC method or topic removed while another service still calls it,
and V6 the tests that reach the change (advisory, never blocks).

The plugin also adds Tessera's MCP server (`find`, `search`, `refs`,
`blast`, `verify`, `context`, `path`, `map`, `status`; read-only,
paginated, compact text) and two commands, `blast` and `verify`, that you
can run yourself.

No model is involved: tree-sitter parses, SQLite stores, the same code gives
the same answer, and nothing leaves your machine. If you point Claude Code
at DeepSeek's Anthropic-compatible endpoint, the same hooks apply.

**Limits, before you install**

- Only high-confidence findings in TypeScript, JavaScript and Java block.
  Python, Go, Rust, C, C++, C#, Ruby and PHP are indexed, but the hooks do
  not show their findings; `tessera verify --changed` does.
- Files that were already changed before the session started do not hold
  Claude unless it edits them.
- If Tessera itself fails (index missing or busy, anything else), the hook
  prints nothing and exits 0. It never stops Claude because of its own bug.
- It does not type-check. Keep `tsc` and your tests in the loop.

On accuracy: 99.7% of high-confidence references match compiler ground
truth on public TypeScript, JavaScript and Java repositories. To hunt false
positives I ran a blind find-and-replace over the benchmark corpus; after
the fixes it leaves 15 blocking findings, all real breaks.

Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment and
Antigravity get the same hooks through `tessera init`.

Repo (Apache-2.0): https://github.com/iamsaquib8/tessera
How the hooks behave in each agent: https://github.com/iamsaquib8/tessera/blob/main/docs/hooks.md

A question for people who already run hooks: does blocking Stop feel right
for this, or would you rather get the report and decide yourself?
