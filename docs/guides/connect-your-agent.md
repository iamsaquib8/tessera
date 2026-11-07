# Connect your agent

You use Claude Code, Cursor, Codex or another agent, and you want Tessera
wired into it: the tools to ask questions, and the hooks that check its edits.

## Run `init`

```sh
tessera init
```

`init` finds the agents you use in this repository, shows the diff of every
file it would change, and asks before it writes. These flags change that:

| Flag | Does |
| --- | --- |
| `--dry-run` | Shows the diffs and writes nothing |
| `--agent <name>` | Sets up that agent even if it is not detected; repeat it for several |
| `--yes` | Writes without asking |
| `--global` | Also changes user-wide config in your home directory |
| `--git-hook` | Adds a pre-commit hook that runs `tessera verify --staged` |
| `--with-instructions` | Also adds usage notes to `AGENTS.md`, `CLAUDE.md` and similar files |

Look at what it would write for Cursor, without writing it:

```console
$ tessera init --agent cursor --dry-run | grep '^+++'
+++ .cursor/hooks.json
+++ .cursor/mcp.json
+++ .tessera/config.toml
```

`init` only adds entries. Your own hooks and servers are untouched, and
running it again prints `Nothing to change.` To undo it, delete Tessera's
entries from those files.

## What each agent gets

Every agent that speaks MCP gets the Tessera tools. Hooks, which check edits
without the agent asking, depend on what the agent's hook system can carry:

| Agent | Before an edit | After an edit | End of turn |
| --- | --- | --- | --- |
| Claude Code, Codex CLI | note | report, then block | block |
| Qwen Code | – | report, then block | block |
| GitHub Copilot CLI, Augment | – | report | block |
| Gemini CLI | – | report | retry |
| Cursor | – | report | follow-up message |
| Google Antigravity | – | – | continue with the problems |
| Everything else | – | – | – |

A dash means the agent has no way to say it. For "everything else" `init`
says so, and suggests the git hook. OpenCode, for example:

```console
$ tessera init --agent opencode --dry-run | grep -A1 '^Notes'
Notes:
  OpenCode: hooks need a JavaScript plugin; use the git hook
```

Zed's MCP config is global only, so `init --agent zed` tells you to add
`--global`. The full table, with config paths and sources, is in
[edit-safety hooks](../hooks.md) and the [agent matrix](../agents.md).

## An agent `init` does not know

`init` names 21 agents. For any other, such as Aider, add the MCP server by
hand if the agent speaks MCP ([integrations](../integrations.md)), and add the
git hook:

```console
$ tessera init --agent aider
error: unknown agent `aider`; known agents: claude, codex, gemini, antigravity, copilot, vscode, cursor, grok, qwen, opencode, kilo, kiro, junie, amp, warp, continue, cline, devin, augment, zed, goose
```

```sh
tessera init --git-hook
```

The git hook refuses a commit with a blocking problem, whoever made it.
`git commit --no-verify` skips it once. An existing pre-commit hook is never
replaced; `init` prints the line to add. Recipes for pre-commit, lefthook and
husky are in [integrations](../integrations.md#every-agent-a-git-backstop).

## Check that it works

1. Restart the agent so it reads the new config. Claude Code asks you to
   approve the project's MCP server the first time.
2. Run `tessera status`. It reports the index and warns when the `tessera` on
   your `PATH` is a different version from the one you ran, which is the usual
   reason an agent runs an old copy ([troubleshooting](../troubleshooting.md)).
3. Ask the agent to rename a function that has callers. The hooks should hold
   it until the callers are fixed. [Catch what an agent broke](catch-what-an-agent-broke.md)
   shows what you will see.

For Claude Code, the plugin does the same without files in your project
(`/plugin marketplace add iamsaquib8/tessera`, then
`/plugin install tessera@tessera`). Use the plugin or `init`, not both.

## What it does not cover

- Where an agent's documented hook contract cannot express a check, the table
  shows a dash and `init` does not invent one. The sources are in the
  [agent matrix](../agents.md).
- Tessera never calls a model, so it works with any model behind the agent.
  It also cannot make an agent read a hook's message.

## Next

- [Check before you edit](check-before-you-edit.md)
- Reference: [integrations by hand](../integrations.md), [MCP tools](../mcp.md)
