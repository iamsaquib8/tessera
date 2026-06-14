# Show HN

## Title (pick one — ≤80 chars, no emoji, HN hates hype)

1. `Show HN: Tessera – a deterministic local code graph so AI agents stop grepping`
2. `Show HN: Tessera – Tree-sitter code graph + MCP server, 60–900x fewer tokens`
3. `Show HN: I built a deterministic code graph to catch when LLMs hallucinate symbols`

> Recommended: #1. Concrete, humble, names the pain.

## URL

https://github.com/iamsaquib8/tessera

## Body (post as the text, or leave blank and put this as the first comment)

Tessera indexes a repo into a local semantic graph with Tree-sitter (SQLite +
a memory-mapped snapshot) and answers structural questions deterministically:
where is X defined, who calls it, what's the blast radius of changing it, how
does A reach B, and — the one I care about most — does this symbol actually
exist or did the model hallucinate it.

I built it because my coding agents spent an absurd fraction of their context
window on `grep` + reading files just to answer "who calls this?", and they'd
periodically invent function names that don't exist. On a real 951-file Java
service, "who calls parseFrom?" is ~394k tokens of grep+read vs ~6.5k through
Tessera (60x); on a 1,063-file Node service one query went from ~36k tokens to
~41 (886x). Incremental re-index is ~38ms, so it can run on every save. Numbers
are reproducible — the bench harness ships in the binary (`tessera bench`).

The deliberate design choice: the graph is **pure AST, not LLM-extracted**.
Same input, same graph, every run, zero tokens to build it. A lot of the newer
"codebase knowledge graph" tools use an LLM extraction pass — great for breadth
and "why was this designed this way" prose, but it's non-deterministic and can
hallucinate its own edges, which feels backwards if the goal is to *stop*
hallucination. Tessera's graph is ground truth, so it can do the inverse:
`tessera validate findByIdd` → "✗ not found; maybe findById (0.98)".

It's a real engine, not a prompt package: CLI, Rust library, and an MCP server
(works with Claude Code, Cursor, Cline, Zed, Codex…). 11 languages
(TS/TSX/JS, Python, Go, Rust, Java, C, C++, C#, Ruby, PHP). Apache-2.0.

Install without a Rust toolchain: `npm i -g tessera-codegraph`, `brew install
iamsaquib8/tessera/tessera`, a curl|sh one-liner, or Docker.

Honest status: it's pre-alpha and I'm a solo dev building in public. The graph
is name-resolved (no full type inference yet), so impact across same-named
symbols is heuristic. Feedback on accuracy, languages, and query design is
exactly what I'm hoping for.

## First comment (post immediately after, from your account — adds context, invites discussion)

Author here. Two things I'd love HN's take on:

1. The token math only matters if the *agent* uses it well. Right now I expose
   everything over MCP with token estimates in every response so the model can
   pick the cheap query — but I suspect there's a better planner-shaped
   abstraction. Curious how others are wiring structural tools into agents.

2. I deliberately avoided embeddings/LLM extraction for the graph itself to keep
   it deterministic and free to rebuild. The tradeoff is I don't get fuzzy
   semantic recall ("find the thing that does auth-ish stuff"). Is that a tradeoff
   you'd make, or do you want both layers?

Happy to answer anything about the Tree-sitter pipeline, the personalized
PageRank impact ranking, or the incremental indexing.

## Timing / tactics
- Post Tue–Thu, ~8:00–10:00 AM ET (HN morning). Avoid weekends.
- Do NOT ask for upvotes anywhere (instant flag). Just share the link with friends to *look* at it.
- Be present to answer comments for the first 2–3 hours — engagement keeps it on /newest → front page.
- If it doesn't catch the first time, you may resubmit once after a few weeks with a different angle (HN allows this for Show HN that got no attention).
