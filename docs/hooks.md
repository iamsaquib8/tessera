# Edit-safety hooks

Agents edit code and say "done". The hooks make Tessera check each edit as it
happens and hold the agent at the end of its turn until the breakage it
caused is fixed. They never call a model, and everything stays on your
machine.

## Set up

```sh
tessera init            # detects your agents, shows the diff, asks
```

For Claude Code there is also a plugin, which needs no files in the project:

```text
/plugin marketplace add iamsaquib8/tessera
/plugin install tessera@tessera
```

Use the plugin or `init` for Claude Code, not both. If both are wired,
Tessera checks each event once and gives both runs the same answer.

Cursor and Copilot CLI also run the hooks in `.claude/settings.local.json`,
so with Claude Code and one of them set up, each of their events reaches
Tessera twice. Tessera tells the client from the event's fields (not from
the hook file it came through), answers in that client's shape, checks the
event once and repeats the answer to the second run, and counts one end of
turn once.

## What happens

| Moment | What Tessera does | When it stops the agent |
| --- | --- | --- |
| Session start | Records what was already changed, and refreshes the index in the background | Never |
| Before an edit | If the edit touches a function with callers in other files, adds a short note (at most 600 characters, about 150 tokens): the callers, and the tests that reach it. Once per function per session | Never |
| After an edit | Re-indexes the edited files and runs `verify` over every file edited so far: calls to names that do not exist, wrong argument counts, broken imports, call sites a rename left behind | Only when a problem is still there after an edit that fixed nothing; a rename done over several edits is never interrupted |
| End of turn | Runs `verify` over everything changed since the session started | While high-confidence problems remain, at most three times per turn; problems the agent stops with after that never block it again in the session |

Only high-confidence findings in TypeScript, JavaScript and Java block.
Findings in other languages are not shown by the hooks; run
`tessera verify --changed` to see them.

Only problems the agent introduced block. Before the index learns of the
agent's first edit, Tessera notes every problem it already holds; a finding
that was there then (the same check, file and message, wherever its line
moved) never stops the agent, even in a file it edits. When the agent adds
another call site of the same problem, the problem counts as its own again.
Files that were already changed before the session started are not checked
at the end of a turn unless the agent changes them again. `tessera verify
--changed` and `--diff` (for CI) still report everything.

The end-of-turn check answers within 40 seconds (its hook is given 60),
however large the change: every problem is reported, and on a very large
change the list of tests that reach it may be cut short.

A problem in Tessera itself never stops the agent: if the index is missing
or busy, or anything fails, the hook prints nothing (or the agent's no-op
answer) and exits 0.

## Agents

| Agent | Before an edit | After an edit | End of turn | Wired by |
| --- | --- | --- | --- | --- |
| Claude Code (also with DeepSeek) | note | report, then block | block | plugin, or `.claude/settings.local.json` |
| Codex CLI | note | report, then block | block | `.codex/hooks.json` (approve with `/hooks`) |
| Qwen Code | – | report, then block | block | `.qwen/settings.json` |
| Gemini CLI | – | report | retry | `.gemini/settings.json` |
| GitHub Copilot CLI | – | report | block | `.github/hooks/tessera.json` |
| Cursor | – | report | follow-up message | `.cursor/hooks.json` |
| Augment (Auggie CLI) | – | report | block | `.augment/settings.local.json` |
| Google Antigravity | – | – | continue with the problems | `.agents/hooks.json` |
| Everything else | – | – | – | `tessera init --git-hook` |

A dash means the agent's documented hook contract has no way to say it:
Qwen's and Antigravity's pre-tool hooks would have to approve the edit,
Copilot's can only allow or deny, and Antigravity's post-tool hook cannot
return feedback. The sources are in [agents.md](agents.md).

A turn that ends in an error, an interruption or a step limit is never held;
Tessera checks only turns the agent finished. Antigravity always gets a
`decision` at the end of a turn (`continue` with the problems, otherwise
`stop`), as its contract requires.

## The git backstop

```sh
tessera init --git-hook
```

installs a pre-commit hook that runs `tessera verify --staged`. A commit
whose changes break a call site or an import is refused, whichever agent (or
person) made it. `git commit --no-verify` skips it once. An existing
pre-commit hook is never replaced; `init` prints the line to add instead.

## Session files and turning it off

Each session's state is in `.tessera/sessions/<id>.json`, next to the index
(git-ignored): the files edited, the notes already shown, the problems still
open, and counts of notes, reports and blocks. Files untouched for a week are
removed.

To turn the hooks off, remove Tessera's entries from the hook file in the
table above, or uninstall the plugin. `tessera init` only ever adds entries,
so your own hooks are untouched either way.
