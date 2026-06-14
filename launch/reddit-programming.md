# Reddit — r/programming

Audience: broad, allergic to self-promo. Frame as a technical idea/writeup, not an ad. Best posted as a link to the dev.to writeup (below) rather than the bare repo. Lead with the deterministic-vs-LLM idea.

## Title
`Why I built a code “knowledge graph” without an LLM — determinism as a feature`

> Post this linking to the dev.to article (devto-post.md), not the repo directly — r/programming treats bare GitHub links as spam. The article links to the repo.

## Optional self-text (if the sub allows text+link or for the first comment)

Most of the new "turn your codebase into a knowledge graph for your AI" tools run
an LLM extraction pass to build the graph. That's powerful for breadth, but it
struck me as backwards for the job I actually wanted: stop the model from
referencing things that don't exist. An LLM-built graph is non-deterministic and
can hallucinate its own edges.

So I went the other way and built the graph purely from the AST (Tree-sitter →
SQLite). Same input, same graph, zero tokens to build, and because it's ground
truth it can *verify* the model's output instead of adding to the guesswork —
e.g. "you referenced `findByIdd`; that doesn't exist, did you mean `findById`
(0.98)?". The writeup covers the design tradeoffs (determinism vs fuzzy recall,
name-resolution vs full type inference, why personalized PageRank for impact),
with reproducible token benchmarks on real repos.

Not trying to dunk on the LLM-extraction tools — they're solving a broader
problem (multimodal, "why was this designed this way"). This is the narrow,
precise, verifiable counterpart. Curious whether people think determinism is
worth giving up semantic recall, or whether you'd want both layers.
