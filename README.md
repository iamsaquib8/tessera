<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Tessera" width="420">
  </picture>
</p>

<h3 align="center">Your agent says done. Tessera checks.</h3>

<p align="center">
  Deterministic blast radius before every edit and verification after it, for any AI coding agent.<br>
  Local, no LLM, Apache-2.0.
</p>

<p align="center">
  <a href="https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml"><img src="https://github.com/iamsaquib8/tessera/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/tessera-codegraph"><img src="https://img.shields.io/crates/v/tessera-codegraph.svg" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/tessera-codegraph"><img src="https://img.shields.io/npm/v/tessera-codegraph.svg" alt="npm"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache-2.0"></a>
</p>

<p align="center">
  <img src="assets/demo.svg" alt="tessera blast ranks the callers of formatName; tessera context shows findById with its callers and callees; tessera verify catches two calls to functions that do not exist" width="760">
</p>

## Why

Coding agents edit code they have only partly read. They grep, open a few
files, change a function, and report success. The caller in another module
that still passes the old arguments turns up later, in CI, in review, or in
production.

Tessera gives the agent an exact local map of the code and checks its work
against it:

- **Before an edit:** `blast` ranks every caller a change can affect and lists
  the tests to run. `context` returns the body, callers, and callees in one
  budgeted read instead of five file opens.
- **After an edit:** `verify` checks that every call in the new code resolves
  to something that exists, and names the closest real symbol when it doesn't.
- **Across services** *(coming in v0.13)*: routes, queues, and tables link
  repositories into one map, so a changed endpoint shows the frontend code
  that calls it.

The map is deterministic. tree-sitter parses, SQLite stores, and the same code
always gives the same answer. No model builds it, and nothing leaves your
machine.

## Quick start

Inside any repository:

```sh
npx tessera-codegraph blast <symbol>
```

There is no setup step: the first query builds the index in `.tessera/`, and
every later query refreshes only what changed. Then hand it to your agent:

```sh
claude mcp add tessera -- tessera mcp
```

Setup for Cursor, Codex, Gemini CLI, Antigravity, Copilot, OpenCode, Kiro and 13 more agents is in [docs/integrations.md](docs/integrations.md).

<details>
<summary>Other ways to install</summary>

```sh
npm i -g tessera-codegraph                       # Node 18+, prebuilt binary, no install scripts
brew install iamsaquib8/tessera/tessera          # macOS and Linux
curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh   # checksum-verified
cargo binstall tessera-codegraph                 # prebuilt via cargo-binstall
cargo install tessera-codegraph                  # from source, Rust 1.90+
docker run --rm -v "$PWD:/work" ghcr.io/iamsaquib8/tessera status
```

</details>

## How it works

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/how-it-works-dark.svg">
  <img src="assets/how-it-works-light.svg" alt="Before the edit, blast and context; your agent edits; after the edit, verify. Both read a local tree-sitter and SQLite index." width="860">
</picture>

| Command | Answers |
| --- | --- |
| `tessera blast <symbol>` / `--diff <rev>` | What does this change affect? Callers ranked by impact, plus tests to run |
| `tessera context <symbol>` | What do I need to read before editing this? |
| `tessera verify --snippet <file> --language <lang>` | Does everything this code calls exist? Exits 1 if not |
| `tessera find <symbol>` / `--path <file>` | Where is it defined? What is in this file? |
| `tessera refs <symbol \| file>` | Who calls it? Who imports this file? |
| `tessera search <pattern>` / `--unused` | Which symbols match? Which functions are never called? |
| `tessera path <from> <to>` | Does A reach B, and how? |
| `tessera status` | Is the index fresh? Is anything misconfigured? |

The same eight operations are MCP tools (`tessera mcp`). They are read-only,
paginated, and return compact text so they cost few tokens. Every command
takes `--json`. See [the five-minute tour](docs/first-five-minutes.md) for
real output.

## Works with every agent and every model

Tessera never calls a model, so Claude, GPT, Gemini, Grok, DeepSeek, Qwen, and
local models all work. Support is per agent:

- **MCP today:**
  - Claude Code, Cursor, Codex CLI, Gemini CLI, Antigravity, GitHub Copilot
    (VS Code and CLI), Grok Build, Qwen Code.
  - OpenCode, Kilo Code, Cline, Continue, Zed, Goose, Amp, Kiro, Junie,
    Augment, Warp, Crush, Devin Desktop.
  - Aider through its lint loop.
- **Automatic checks before and after edits** *(v0.12)*: hooks for every agent
  above that has a hook system, starting with a Claude Code plugin, plus a git
  pre-commit backstop for the rest.

Config for each one is in [docs/integrations.md](docs/integrations.md). The
sourced matrix of hook capabilities is in [docs/agents.md](docs/agents.md).

## Languages

TypeScript, TSX/React, JavaScript (ESM and CommonJS), Java, Python, Go, Rust,
C, C++, C#, Ruby, PHP.

All twelve are indexed at the **standard** tier today. Definitions, references,
imports, and calls are matched by name. In v0.11, TypeScript, JavaScript, and
Java move to the **deep** tier: import- and type-aware resolution with a
confidence level on every edge, measured against compiler ground truth in a
public benchmark. [Current limits](docs/when-not-to-use.md) are listed plainly.

## FAQ

**Why not just run `tsc` or use an LSP?** Run them too. Tessera checks only
the changed code, in milliseconds, so it fits in an agent's edit loop. It is
the only check available for JavaScript, Python, and Ruby, and in v0.13 it
covers breaks between services that no compiler sees. An LSP answers one
editor's questions about one project; Tessera keeps one index for the whole
repository, and soon across repositories, that any agent can query.

**Does my code leave my machine?** No. The index is a SQLite file in
`.tessera/`, gitignored automatically. Tessera makes no network calls, and
`blast --diff` runs only a local `git diff`.

**Is it accurate?** It is exact about what it records, and open about what it
doesn't record yet: see [limits](docs/when-not-to-use.md). v0.11 publishes
precision and recall against SCIP ground truth on public repositories,
alongside other code-graph tools, so you can check the numbers yourself.

**Will it slow my agent down?** Queries take milliseconds; perf budgets run in
CI (freshness check under 100 ms and lookups under 50 ms at p95 on 10,000
files). Indexing is incremental, and the MCP server re-indexes edited files in
the background.

## Roadmap

| Version | Milestone |
| --- | --- |
| v0.10 | Foundation: index any repo, zero setup, MCP for every agent |
| v0.11 | Precision: import- and type-aware resolution, public accuracy benchmark |
| v0.12 | Edit-safety loop: hooks that check every edit, in every agent that supports hooks |
| v0.13 | Contract map: routes, queues, and tables across repositories |
| v1.0 | Change-impact UI, GitHub Action, stable contracts |

Further out: CI test selection, architecture rules, SCIP precise mode, editor
extensions, runtime traces, and data lineage. See [ROADMAP.md](ROADMAP.md).

## Contributing

New languages, framework packs, and agent adapters are small, self-contained
pull requests with fixtures. [CONTRIBUTING.md](CONTRIBUTING.md) explains each
one. Issues labelled
[`good first issue`](https://github.com/iamsaquib8/tessera/labels/good%20first%20issue)
are scoped for a first PR.

If Tessera saves your agent from a bad edit, a star helps other people find it.

## License

Apache-2.0. See [LICENSE](LICENSE).
