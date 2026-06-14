# Reddit — r/ClaudeAI

Audience: Claude Code / MCP users. Lead with the integration + the drop-in skill + concrete agent pain.

## Title
`I built an MCP server (+ drop-in skill) so Claude Code stops grepping your whole repo to answer "who calls this?"`

## Body

If you use Claude Code on a big repo, you've watched it spend tool calls running
`grep` and reading files just to figure out where something is defined or what a
change affects — eating context and sometimes referencing functions that don't
exist.

I made **Tessera** to fix that. It indexes your repo into a local Tree-sitter
graph and exposes deterministic tools over **MCP**:

- `impact` — who transitively calls this, ranked by personalized PageRank (the callers that actually matter, not just a flat list)
- `validate` — does this symbol exist? Catches hallucinated names and suggests the real one ("✗ findByIdd → maybe findById (0.98)")
- `connect A B` — the shortest call path from one symbol to another
- `export` — the call graph as Mermaid you can drop into a PR
- `context_pack` — body + deps + callers + tests in one budgeted bundle, so the agent preps an edit in a single call instead of five

Two ways to use it with Claude Code:

1. **MCP:** `claude mcp add tessera tessera -- mcp --db .tessera/tessera.db`
2. **Zero-install skill:** copy the `/tessera` Agent Skill into `~/.claude/skills/` and Claude will use it for navigation automatically, installing the binary on first use.

It's local and deterministic — the graph is built from the AST, not an LLM pass,
so it's free to rebuild (~38 ms incremental) and it's ground truth. On a real
1,063-file service, "who calls X" dropped from ~36k tokens of grep+read to ~41.

11 languages (TS/TSX/JS, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP),
Apache-2.0. Install via npm / brew / curl / cargo / Docker.

https://github.com/iamsaquib8/tessera

Pre-alpha, solo project — would love to hear how it behaves in your Claude Code
setup and which tool you'd want next.
