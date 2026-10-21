# M2 Edit-Safety Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make "your agent says done, Tessera checks" literal: `verify` catches the breakage an edit causes, hooks run it automatically in Claude Code and the other tier-A agents, `tessera init` wires every detected agent, and a git hook backstops everyone else.

**Architecture:** The resolve pass (M1) already knows when a name points into the repository but finds nothing there. M2 records those outcomes, turns them into `verify` findings (V1–V4) plus advisory tests to run (V6), and exposes them through one hook entry point (`tessera hook <agent> <event>`) whose per-agent input and output shapes live in an adapter registry. `init` uses the same registry to write MCP, hook and instruction config. A refactor eval in `tessera-bench` measures the effect.

**Spec:** `docs/design/2026-10-21-overhaul.md` §5.1, §5.3, §5.4, §5.6 (refactor eval), §8 (M2 row), §9 budgets; `docs/agents.md` (sourced hook contracts).

## Global Constraints

Everything in the M0 and M1 plans' Global Constraints still applies. In addition:

- **Only high blocks.** A finding blocks a hook or exits 1 only at `high` confidence, and only for deep-tier languages (TypeScript, JavaScript, Java). Standard-tier findings are advisory.
- **Budgets:** `PostToolUse` end-to-end p95 ≤ 300 ms for one changed file in a 5k-file repo; the pre-edit note is ≤ 150 tokens and shown once per symbol per session; `Stop` re-blocks at most 3 times per session.
- **No guessing at contracts.** A hook adapter is implemented only from a vendor-documented contract in `docs/agents.md`; anything marked unverified there stays unimplemented and `init` says so.
- **init safety:** preview a diff, ask y/N (`--yes` for scripts, `--dry-run` to print only), project-scoped files only unless `--global`, never touch instruction files without `--with-instructions`, idempotent.
- **Gate:** hook budgets met; verify false-positive rate below 5% of blocks on the refactor eval; the eval reports the change in missed call sites, whatever it is.

## Review Focus

1. **Half-finished edits.** An agent's first `Edit` of a two-step rename leaves call sites broken on purpose; `PostToolUse` must report, not block the second step (block only on `Stop`, or when the same finding survives the next edit).
2. **Renames across files.** Renaming an export and its importers in one session must end clean at `Stop`, with no stale findings from the intermediate state.
3. **Hook payload drift.** Cursor loads Claude-format hooks, Grok reads `.claude/settings.json`; the entry point must detect the payload shape it got, not trust the configured agent name.
4. **Generated and vendored files.** Edits under ignored paths are not verified.
5. **Huge diffs.** `Stop` after a 500-file change stays within a bounded time and reports the top findings, not all of them.

---

## Tasks

### Task 1: Resolution outcomes and findings

**Files:** `src/resolve/{mod,scope}.rs`, `src/db.rs`, `src/query/verify.rs`, `tests/verify.rs` (new), fixtures `tests/fixtures/verify-ts/`, `tests/fixtures/verify-java/`

**Produces:**
- Resolver outcome `missing` (resolver value): a name reaches a repository module, class or namespace and nothing by that name exists there.
- `import_bindings.status`: `ok`, `missing_module` (a relative or aliased specifier resolves to no file), `missing_export`.
- Edges keep the call's `arg_count`; a high edge whose target's arity cannot take it is an arity mismatch.
- `verify` checks, each with `file:line`, symbol, confidence and a one-line fix hint:
  - **V1** unresolved call in a changed file (resolver `missing`).
  - **V2** arity mismatch on a high edge.
  - **V3** dangling reference: a reference outside the changed files whose edge resolver is `missing` after the change.
  - **V4** broken import (`missing_module` / `missing_export`).
  - **V6** tests that reach changed symbols (advisory).
- `verify --changed` (working tree vs `HEAD` plus untracked files), `--diff <rev>`, `--staged`, `--snippet`; formats `agent`, `human`, `json`, `sarif`.

**Tests:** one fixture case per check, plus false-positive guards: calls into npm packages, Lombok-style generated members, methods inherited from an external superclass, and standard-tier languages never block.

### Task 2: Hook entry point and session state

**Files:** `src/hook/{mod,payload,session}.rs`, `src/main.rs` (`hook` subcommand), `tests/hook.rs`

- `tessera hook <agent> <event>` reads the event JSON on stdin, detects the payload shape (`tool_input`, `toolArgs`, `toolCall.args`), and writes the agent's documented output shape.
- Session state in `.tessera/sessions/<id>.json`: baseline (HEAD sha and dirty files at `SessionStart`), files edited, symbols already announced, Stop re-block count.
- **PreToolUse (edit tools):** symbols the edit touches with cross-file callers → a note of at most 150 tokens (caller count, top three call sites, tests to run), once per symbol per session; silent otherwise.
- **PostToolUse:** `index_files` on the edited paths, then V1–V4 scoped to them and their dependents. Findings are reported as context; a finding blocks only when it was already present after the previous edit (Review Focus 1).
- **Stop:** verify everything changed since the baseline; block on high findings, with `stop_hook_active` honoured and at most 3 re-blocks.

**Tests:** recorded payloads for each supported agent; the two-step rename and the cross-file rename from Review Focus 1–2; p95 budget test on `synth_repo(5_000)`.

### Task 3: Adapter registry and `init`

**Files:** `src/agents/{mod,registry}.rs`, one module per agent family, `src/init.rs` (rewrite), `tests/init.rs`, `tests/fixtures/agents/`

- One adapter per agent in `docs/agents.md`: detection rule, MCP config path and format (project and global), hook config path and format if tier A/A−/B+, instructions file, trust step to print.
- `tessera init`: detect agents, build every change, show a unified diff, ask y/N; `--yes`, `--dry-run`, `--agent <name>`, `--global`, `--with-instructions`, `--git-hook`.
- Merging never drops a user's existing entries; re-running is a no-op.

**Tests:** a fixture repo per format family (JSON `mcpServers`, VS Code `servers`, TOML `[mcp_servers.x]`, OpenCode `mcp` arrays, YAML extensions); idempotency; diff preview output.

### Task 4: Claude Code plugin

**Files:** `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `plugin/hooks/hooks.json`, `plugin/.mcp.json`, `plugin/skills/tessera/SKILL.md`, `plugin/commands/{blast,verify}.md`, `plugin/bin/tessera-run` (finds a matching `tessera` or falls back to `npx -y tessera-codegraph@<version>`)

**Tests:** `tests/plugin.rs` validates the manifests against the documented schema fields and that every hook command exists.

### Task 5: Other tier-A hook adapters

Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment, then A− agents for the half of the loop they support. Each: recorded payload fixtures from vendor docs, output-shape tests, `init` wiring.

### Task 6: Git backstop

`tessera init --git-hook` installs a pre-commit hook running `tessera verify --staged --format agent` (blocking only on high), through `git rev-parse --git-path hooks`. Test: a commit with a broken import is refused; `--no-verify` still works.

### Task 7: Refactor eval

**Repo:** `~/dev/tessera-bench/eval/`

- 30–50 refactor tasks on the corpus (rename, add a required parameter, remove a parameter, change a return shape, move a function), each with an oracle: the repo's test or compile command plus the ground-truth call-site list.
- Three arms (baseline, MCP only, MCP plus hooks), three repeats, run headless through `claude -p` on Saquib's subscription with Claude Code's default model, model id recorded.
- Checkpointed runner that tolerates rate limits; a 5-task pilot first to size the full run.
- Report: missed call sites, task success, tokens, wall time, and verify blocks that were wrong (false-positive rate).

### Task 8: Docs and release

README and docs for hooks and `init`; `docs/agents.md` re-verified; CHANGELOG 0.12.0; version bump on the same branch and PR; launch post draft in `launch/` (publishing is Saquib's call).
