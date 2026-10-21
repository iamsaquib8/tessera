# Tessera

**Your agent says done. Tessera checks.**

Open source (Apache-2.0) · [github.com/iamsaquib8/tessera](https://github.com/iamsaquib8/tessera) · `npx tessera-codegraph init`

## The problem

AI coding agents edit code they have only partly read. An agent greps,
opens a few files, renames a method, and reports success. The caller in
another module, or in another service, that still uses the old name
surfaces later: in CI, in review, or in production. Agents have no cheap,
exact way to know what an edit reaches or to check that it broke nothing.

## What Tessera does

A local, deterministic code graph that every agent can query and that checks
every edit:

- **Before the edit:** the callers a change can affect, ranked, and the
  tests to run. One budgeted read instead of five file opens.
- **After the edit:** calls to names that do not exist, wrong argument
  counts, broken imports, call sites a rename left behind. Hooks run the
  check after each edit and hold the agent at "done" until it is fixed.
- **Across services:** HTTP routes, gRPC methods, Kafka topics and tables
  linked to the code on both sides, in every language, so removing an
  endpoint that another service still calls is caught.
- **On the pull request:** a GitHub Action comments what the change breaks,
  its callers, and its tests. `tessera ui` shows the same thing locally.

No model builds the graph: tree-sitter parses, SQLite stores, the same code
gives the same answer every time, and nothing leaves the machine.

## Proof

| | |
| --- | --- |
| Precision | 99.7% of high-confidence references match compiler ground truth on public TypeScript, JavaScript and Java repositories |
| Recall | 99% of direct calls, 91% of calls on typed receivers, 99% of type references (TS/JS) |
| Contracts | Every gRPC server (15/15) and client (34/34) in GoogleCloudPlatform/microservices-demo, across Go, C#, Node, Python and Java, with no wrong link |
| False positives | A blind find-and-replace over the public benchmark corpus leaves 15 blocking findings, all real breaks |
| Speed | Lookups under 50 ms p95 on 10,000 files; a 45,000-file repository indexes in under a minute |
| Reach | 21 agents set up by one command; edit-checking hooks in 8 (Claude Code, Codex, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment, Antigravity); 12 languages |

The benchmark, its ground truth and adapters for other tools are public in
[tessera-bench](https://github.com/iamsaquib8/tessera-bench).

## Why now, why this

- Agents are becoming the editor, and most major agents now ship a hook
  system. A model-agnostic checker that plugs into all of them was missing.
- LSPs answer one editor's questions about one project, and stop at the
  service boundary. Tessera is one index per repository or workspace, built
  for agent loops, and it sees across services.
- Deterministic and local: works in regulated codebases where sending code
  to another model is not an option.

## Where it goes next

Test selection in CI, architecture rules enforced at every edit, SCIP
precise mode, editor extensions, runtime traces over the static graph, and
data lineage. See the [roadmap](../../ROADMAP.md).

## Try it

```sh
npx tessera-codegraph blast <symbol>    # no setup: indexes on first use
npx tessera-codegraph init              # wire it into your agents
```
