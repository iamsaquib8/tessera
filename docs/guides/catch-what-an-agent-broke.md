# Catch what an agent broke

Your agent edited code and said "done". You want to know whether it left a
call to a function that no longer exists, a call with the wrong number of
arguments, or an import that no longer resolves, without reading the diff.

The examples use the TypeScript sample from the [quickstart](../quickstart.md).

## Check the working tree

Rename `formatName` in `src/util/format.ts` to `formatDisplayName` and leave
its callers alone, as an agent that edited one file might:

```sh
perl -pi -e 's/export function formatName\(/export function formatDisplayName(/' src/util/format.ts
```

```console
$ tessera verify --changed
✗ verify: 2 problem(s)
  [V3] src/users/repo.ts:7 `formatName` no longer exists in src/util/format.ts, which this change edited
  [V4] src/users/repo.ts:1 `formatName` is not exported by ../util/format
```

`--changed` checks every change since `HEAD`, untracked files included.
`--staged` checks what is staged, and `--diff <rev>` checks the tree against a
revision. All three exit with code 1 when a high-confidence problem is found.
The checks are V1 (a name that is not defined), V2 (wrong argument count), V3
(a call left behind by a rename or removal), V4 (a broken import) and V5 (a
removed contract a client still uses, see
[block breaking API changes](block-breaking-api-changes.md)). A sixth id, V6,
names a test that reaches the change; it is a test to run, never a problem.

## Hold the agent until it is fixed

Wire the hooks with `tessera init` ([connect your agent](connect-your-agent.md)).
They check each edit and the end of the turn. These are the three answers
Tessera gives Claude Code for the rename above. They are the real output of
`tessera hook claude <event>`, given the JSON that Claude Code sends:

Before the edit (`pre-edit`), a note:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Tessera: this edit touches code used from other files.\n- `formatName` (src/util/format.ts:1) has 1 use(s) in other files: src/users/repo.ts:7; tests: test/repo.test.ts.\nKeep those call sites working, or update them in the same change."}}
```

After the edit (`post-edit`), the problems, with room to finish:

```json
{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"tessera verify: 2 problem(s) to fix\nV3 src/users/repo.ts:7 `formatName` no longer exists in src/util/format.ts, which this change edited\nV4 src/users/repo.ts:1 `formatName` is not exported by ../util/format\nIf the change is not finished yet, carry on; Tessera checks again when you stop."}}
```

When the agent tries to stop (`stop`), a block:

```json
{"decision":"block","reason":"tessera verify: 2 problem(s) to fix\nV3 src/users/repo.ts:7 `formatName` no longer exists in src/util/format.ts, which this change edited\nV4 src/users/repo.ts:1 `formatName` is not exported by ../util/format\nFix these before you finish."}
```

The agent reads the reason and keeps working. Three rules keep the hooks from
getting in the way:

- Only problems the agent introduced block. Problems that were in the code
  before the session started never do.
- The hold ends after three tries in one turn, so a false positive cannot
  trap the agent.
- If the index is missing or Tessera fails, the hook prints nothing and exits
  0.

Agents without hooks are covered by `tessera init --git-hook`, a pre-commit
hook that refuses a commit with a blocking problem, whoever made it. See
[edit-safety hooks](../hooks.md) for the table of what each agent supports.

## See what the hooks did

```sh
tessera stats    # edits checked, blocks, problems fixed, hook latency; stays local
```

## What it does not cover

- It is not a type checker. `verify` finds names, argument counts and imports
  that do not match the index; it does not check types, and it never flags
  calls into third-party packages. Keep running `tsc`, `javac` and your tests.
- Only high-confidence findings in TypeScript, JavaScript and Java block in
  the hooks. Findings in other languages show in `tessera verify --changed`.
- A break outside the code Tessera reads (generated code, reflection,
  dependency injection by name) is not seen. See
  [when not to rely on Tessera](../when-not-to-use.md).

## Next

- [Run only the tests a change can affect](run-only-affected-tests.md)
- [Adopt on a legacy repository](adopt-on-a-legacy-repo.md), if `verify` reports problems that were there first
- Reference: [edit-safety hooks](../hooks.md)
