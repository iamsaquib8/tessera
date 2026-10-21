---
description: Show what a change to a symbol (or to the working tree) can break, ranked, with the tests to run
argument-hint: "[symbol]"
---

Run `"${CLAUDE_PLUGIN_ROOT}/scripts/tessera-run" blast $ARGUMENTS` in the project root. With no symbol, run `"${CLAUDE_PLUGIN_ROOT}/scripts/tessera-run" blast --diff HEAD` to rank everything the uncommitted changes affect.

Summarise the result for the user: the callers most likely to break (with `file:line`), any caller marked as name-only or low confidence, and the tests to run. Do not edit anything.
