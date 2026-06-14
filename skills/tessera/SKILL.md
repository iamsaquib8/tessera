---
name: tessera
description: Use when navigating or reasoning about an unfamiliar or large codebase - finding where a symbol is defined, who calls it, what a change impacts, whether a symbol actually exists (catching hallucinated names), tracing how two functions connect, or visualising the call graph. Answers deterministically over a local Tree-sitter graph instead of burning context on grep + file reads. Supports TypeScript/TSX, JavaScript, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP.
---

# Tessera — deterministic code navigation

Tessera indexes a repo into a local semantic graph (Tree-sitter → SQLite) and
answers structural questions in milliseconds, for a fraction of the tokens that
`grep` + reading files costs. Unlike an LLM-extracted graph, every answer is
**ground truth from the parser** — same input, same answer, zero tokens to build
the graph.

## When to reach for this

- "Where is `X` defined?" → `find-definition`
- "Who calls `X`? What breaks if I change it?" → `impact` (PageRank-ranked)
- "Does `X` exist, or did I hallucinate it?" → `validate`
- "How does `A` reach `B`?" → `connect`
- "Show me the call graph around `X`" → `export`
- "What does this file/dir contain?" → `get-outline`
- Before editing a symbol: `context-pack` (body + deps + callers + tests in one call)

Prefer these over `grep -r` + opening files: they return only the structural
answer, with a token estimate, and never miss a cross-file caller.

## Setup (once per machine)

Check if it's installed: `tessera --version`. If not, install (no Rust needed):

```sh
npm i -g tessera-codegraph          # or: npx tessera-codegraph <cmd>
# or  brew install iamsaquib8/tessera/tessera
# or  curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
# or  cargo install tessera-codegraph
```

## Workflow

1. **Index** the repo (incremental; re-run anytime — unchanged files are reused in ~ms):
   ```sh
   tessera index . 
   ```
2. **Query.** Add `--json` for machine-readable output. Every command prints a
   `_meta` line with a token estimate and a cheaper alternative.
   ```sh
   tessera find-definition parseConfig
   tessera impact parseConfig          # transitive callers, ranked by criticality
   tessera validate prseConfig         # ✗ + "maybe parseConfig (0.97)"
   tessera connect handleRequest writeRow   # shortest call path A → B
   tessera export --from parseConfig --format mermaid   # subgraph as Mermaid
   tessera context-pack parseConfig    # everything needed to edit it, token-budgeted
   ```

## Best practice for agents

- **Validate before trusting a symbol name** you're about to reference in code:
  `tessera validate <name>` catches hallucinations deterministically and suggests
  the real name. This is the one check an LLM-extracted graph cannot do reliably —
  its own graph may contain the hallucination.
- **Use `impact` before refactoring** to enumerate every affected caller instead
  of guessing.
- **Re-index after edits** (`tessera index .`) — it's incremental and cheap.

## MCP server (richer integration)

Tessera also speaks MCP, exposing all of the above as tools (`find_definition`,
`impact`, `validate`, `connect`, `export`, `context_pack`, …):

```sh
tessera mcp --db .tessera/tessera.db
```

Wire it into Claude Code / Cursor / Cline / Codex / Zed per
[docs/integrations.md](https://github.com/iamsaquib8/tessera/blob/main/docs/integrations.md).
