# Stability

From 1.0, scripts, CI jobs, agent configs and MCP clients built on Tessera
keep working across every 1.x release. This page lists what that covers.
`tests/stability.rs` pins each item against `tests/stability/*.json`, so a
release that drops one fails its own tests.

## What 1.x keeps

**Commands and flags.** Every command and flag in 1.0 keeps its name and
meaning, including the values of enum flags (`verify --format human`,
`agent`, `json`, `sarif`, `markdown`). New commands, flags and values may
appear. A flag that is replaced keeps working, with a warning on stderr,
until 2.0.

**Exit codes.**

| Code | Meaning |
| --- | --- |
| 0 | Success; for `verify`, no blocking problem |
| 1 | `verify` found a blocking problem |
| 2 | Usage error: an unknown flag, a missing argument, a bad value |
| 3 | Index or I/O error: the index could not be opened, built or read |

Hooks (`tessera hook`) always exit 0: a Tessera failure never stops an
agent.

**`--json` fields.** Every field of every command's JSON output stays, with
the same name and meaning (the tests pin the names). New fields may be
added at any level, so parse JSON leniently. Values that describe the
index (symbol and file `id`s, `elapsed_ms`, counts) are facts about one
index, not identifiers to store.

**Check ids.** `V1` to `V6` keep their meaning in every format, including
SARIF rule ids:

| Id | Finding |
| --- | --- |
| V1 | Call to a name that does not exist where it points |
| V2 | Argument count does not match the target |
| V3 | Reference to a name the change removed |
| V4 | Import of a missing module or export |
| V5 | Contract removed while a client still uses it |
| V6 | Test that reaches the changed code (advisory, never blocks) |

New checks get new ids.

**MCP tools.** Tool names and argument names stay. New tools and new
optional arguments may appear; no tool gains a required argument. A
tool's `format: "json"` result has the same fields as the matching
command's `--json` output.

**Hooks.** Every `tessera hook <agent> <event>` command that `tessera init`
wrote in 1.x keeps working, and each reply keeps the shape the agent's own
hook contract documents. If a vendor changes that contract, Tessera follows
the vendor.

**The pull-request report.** The `<!-- tessera -->` marker that the GitHub
Action uses to find its own comment, and the Action's inputs and outputs.

**Review formats.** The fields of `verify --format sarif`, `gitlab`,
`bitbucket` and `ndjson` (pinned like `--json`), the shape of `azure`
logging commands, and each finding's `fingerprint`
([docs/ci.md](ci.md)).

**Cache files.** A cache file written by one Tessera version is imported
only by that version; any other refuses it and indexes from scratch.

**Files you write.** `tessera.workspace.toml` and `.tessera/config.toml`
keep their format.

## What may change in 1.x

- Human-readable output, the one-line `agent` format, and MCP text results:
  wording, layout and order.
- Which findings appear. Better resolution removes false positives and
  finds more problems; confidence levels follow the evidence.
- Ranking: the order of search hits, callers and suggestions.
- The index database in `.tessera/`. A new schema rebuilds it on first use;
  nothing outside Tessera should read it.
- Framework packs under `packs/`, which are compiled in and not a plugin
  interface yet.
- Anything marked experimental in its own documentation.

## Changing the contract

An addition is a deliberate change to the pinned files: run
`TESSERA_UPDATE_STABILITY=1 cargo test --test stability` and commit the
updated `tests/stability/*.json` with the change. A removal waits for 2.0.
