# X / Twitter thread

Attach `assets/demo.svg` (export a PNG/GIF) or `assets/social-preview.png` to tweet 1. Keep tweet 1 strong enough to stand alone.

---

**1/**
Your AI coding agent burns most of its context window on `grep` + reading files
just to answer "who calls this?" — and sometimes invents functions that don't exist.

I built Tessera to fix both. A local, deterministic code graph for agents.

🧵 github.com/iamsaquib8/tessera
[attach social-preview.png]

**2/**
On a real 1,063-file service, "who calls BaseWorker?" is:

• raw grep + read → ~36,311 tokens
• tessera → ~41 tokens

886× cheaper. On a 951-file Java service, another query: 60× cheaper.

The benchmark ships in the binary — `tessera bench` reproduces it on your repo.

**3/**
The key design choice: the graph is built from the AST (Tree-sitter), NOT an LLM.

Same input → same graph, every time. Zero tokens to build it.

Most "codebase knowledge graph" tools use an LLM extraction pass. That's fine for
breadth — but it's non-deterministic and can hallucinate its own edges.

**4/**
Because Tessera's graph is ground truth, it does the opposite of hallucinating —
it *catches* the model's hallucinations:

$ tessera validate findByIdd
✗ not found — maybe findById (0.98)

A guard rail, deterministic, free to run.

**5/**
What you can ask it:
• impact — who's affected by a change, ranked by personalized PageRank
• connect A B — the shortest call path between two functions
• export — the call graph as Mermaid, paste into a PR
• validate — did the model make this up?

CLI · MCP server · Rust library.

**6/**
11 languages: TS/TSX/JS, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP.
Incremental re-index in ~38ms, so it runs on every save.

Install without Rust:
npm i -g tessera-codegraph
brew install iamsaquib8/tessera/tessera
curl … | sh

**7/**
Works with Claude Code, Cursor, Cline, Zed, Codex — anything that speaks MCP.
There's also a drop-in `/tessera` skill for Claude Code (zero install).

Apache-2.0. Pre-alpha, built in public, solo.

⭐ github.com/iamsaquib8/tessera — a star genuinely helps it reach people.

**8/ (reply-bait)**
Open question I keep going back and forth on: should I add an optional embedding
layer for fuzzy recall ("find the thing that does auth-ish stuff"), or is keeping
it 100% deterministic the whole point?

Tell me I'm wrong.
