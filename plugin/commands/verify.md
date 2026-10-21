---
description: Check the uncommitted changes for broken call sites, wrong argument counts, and broken imports
---

Run `"${CLAUDE_PLUGIN_ROOT}/scripts/tessera-run" verify --changed --format agent` in the project root.

If it reports problems, fix each one at the `file:line` it gives, then run it again until it reports none. Then run the tests it lists, if any. If it reports none, say so in one line.
