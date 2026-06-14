# Reddit — r/LocalLLaMA

Audience: local-first, token-cost-obsessed, agent builders. Lead with token math + determinism. No hype.

## Title
`Tessera: a local, deterministic code graph that cuts "who calls this?" from ~394k tokens to ~6.5k (MCP + CLI, 11 languages)`

## Body

I kept watching my local coding agents burn their whole context window on `grep`
+ file reads just to answer structural questions ("who calls this function, what
breaks if I change it"), and occasionally invent symbols that don't exist. So I
built **Tessera** — a local semantic code graph (Tree-sitter → SQLite +
mmap snapshot) that answers those deterministically.

Measured on real repos (the bench ships in the binary, so you can reproduce it):

- 951-file Java service — "who calls parseFrom?": **~394,140 tokens** raw grep+read vs **~6,530** through Tessera (**60×**)
- 1,063-file Node service — "who calls BaseWorker?": **~36,311** vs **~41** (**886×**)
- Incremental re-index: **~38 ms** (runs on every save)

The design choice that matters for this sub: the graph is **pure AST, not
LLM-extracted**. No embedding/LLM pass to build it — same input, same graph,
zero tokens, fully local. Because it's ground truth, it can *catch* the model's
hallucinations rather than add to them: `tessera validate findByIdd` → "✗ not
found; maybe findById (0.98)".

Runs as an MCP server (Claude Code / Cursor / Cline / Zed / any MCP client), a
CLI, or a Rust library. 11 languages: TS/TSX/JS, Python, Go, Rust, Java, C, C++,
C#, Ruby, PHP. Apache-2.0.

Install without Rust: `npm i -g tessera-codegraph`, `brew`, `curl | sh`, or Docker.

Repo + benchmarks: https://github.com/iamsaquib8/tessera

It's pre-alpha and I'm solo on it — would genuinely value feedback on the query
design and where the token accounting is misleading. Also curious: would you want
an optional embedding layer on top for fuzzy recall, or do you prefer keeping it
100% deterministic?
