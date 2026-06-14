---
title: "Why I built a code knowledge graph without an LLM"
published: false
description: "Determinism as a feature: a local, AST-built code graph that catches hallucinated symbols instead of producing them."
tags: ai, rust, devtools, llm
cover_image: https://raw.githubusercontent.com/iamsaquib8/tessera/main/assets/social-preview.png
canonical_url: https://github.com/iamsaquib8/tessera
---

> Set `published: true` when you're ready. Cover image resolves once `assets/social-preview.png` is on `main`.

My AI coding agents had two expensive habits. First, they'd spend an enormous
chunk of their context window on `grep` + reading files just to answer structural
questions — "where is this defined", "who calls it", "what breaks if I change
it". Second, they'd occasionally reference functions that simply don't exist.

I built [**Tessera**](https://github.com/iamsaquib8/tessera) to kill both. It's a
local semantic code graph for AI agents — and the design decision I want to talk
about is the one that sounds backwards at first: **the graph is built without an
LLM, on purpose.**

## The token problem, with numbers

Tessera indexes a repo with Tree-sitter into SQLite plus a memory-mapped
snapshot, then answers structural queries from that graph. The benchmark ships in
the binary (`tessera bench`), so these are reproducible, not marketing:

- **951-file Java service** — "who calls `parseFrom`?": ~394,140 tokens of raw grep+read vs **~6,530** through Tessera — **60× cheaper**.
- **1,063-file Node service** — "who calls `BaseWorker`?": ~36,311 vs **~41** — **886× cheaper**.
- **Incremental re-index: ~38 ms** — fast enough to run on every file save.

That "who calls this?" question is exactly what agents spend the most context on,
and it's the one a graph answers almost for free.

## Determinism as a feature

There's a wave of excellent tools that turn your codebase into a "knowledge
graph" using an **LLM extraction pass**. They're great for breadth — multimodal
input, prose explanations of *why* something was designed a certain way.

But think about what that means for the specific job of *not getting lied to*:

- The graph is **non-deterministic** — re-run it, get a slightly different graph.
- It **costs tokens every time you build it.**
- The extraction step can **hallucinate edges of its own.**

Using a graph that can hallucinate to stop a model from hallucinating is a strange
foundation. So Tessera goes the other way:

> The graph is pure Tree-sitter AST + static resolution. Same input → same graph,
> every run, **zero LLM tokens to build.** "Who calls `parseFrom`?" is a fact from
> the parser, not an inference.

And because the graph is ground truth, Tessera can do the *inverse* of
hallucinating — it catches the model's hallucinations:

```sh
$ tessera validate findByIdd
✗ findByIdd not found — maybe findById (0.98)
```

That's a Bloom-filter-fronted existence check plus Jaro-Winkler near-miss scoring,
all deterministic.

## What you can ask

- **`impact <symbol>`** — transitive callers ranked by *personalized* PageRank. Not a flat "who calls X" list — the random surfer teleports back to your edit site, so the callers that actually matter float to the top, with an auditable breakdown.
- **`connect A B`** — the shortest call path between two symbols. "Does `handleRequest` actually reach `writeRow`, and how?"
- **`export --format mermaid`** — the call graph (whole, or the subgraph rooted at one symbol) as a diagram you can paste into a PR.
- **`context-pack <symbol>`** — body + dependency signatures + top callers + relevant tests in one token-budgeted bundle, so an agent preps an edit in a single call.

## It's an engine, not a prompt package

Tessera is a CLI, a Rust library, *and* an MCP server — so it works inside Claude
Code / Cursor / Cline / Zed, but also in CI or a plain script. There's a drop-in
`/tessera` Agent Skill for zero-install use in Claude Code.

11 languages: TypeScript/TSX/JS, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP.
Apache-2.0.

```sh
npm i -g tessera-codegraph     # or brew / curl | sh / cargo / docker
tessera index .
tessera impact findById
```

## The honest part

It's pre-alpha and I'm a solo dev building in public. Edges are resolved by name
(no full type inference yet), so impact across same-named symbols is heuristic —
that's my next big accuracy push. And I keep going back and forth on whether to
add an optional embedding layer for fuzzy semantic recall, or whether keeping it
100% deterministic *is* the point.

If the determinism angle resonates — or if you think I'm wrong about it — I'd love
to hear it. ⭐ [github.com/iamsaquib8/tessera](https://github.com/iamsaquib8/tessera)
