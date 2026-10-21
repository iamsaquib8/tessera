# Draft: "99.7% of Tessera's exact answers match the compiler"

*Status: draft for Saquib. Publishing, and where, is Saquib's call.*

Coding agents find call sites with grep. Grep can't tell `repo.save()` from
`cache.save()`, so an agent either reads too much or misses the caller that
breaks. Tessera 0.11 resolves references the way a reader does: through
imports, re-exports, tsconfig paths, `this`, and declared types. It labels
anything it had to guess.

We measured it instead of claiming it. tessera-bench builds compiler ground
truth (scip-typescript) for pinned public repositories, samples 200
definitions per repository across fan-in buckets, and scores every reference
Tessera reports:

| | high-confidence precision | direct calls | typed receivers | type references |
| --- | --- | --- | --- | --- |
| excalidraw | 0.999 | 1.000 | 0.914 | 0.990 |
| nest | 0.994 | 1.000 | 0.969 | 0.985 |
| Ghost core | 0.990 | 0.879 | 0.698 | 0.963 |
| spring-petclinic (Java) | 0.995 | 0.900 | 0.958 | 1.000 |
| petclinic microservices (Java) | 0.995 | 0.778 | 0.800 | 1.000 |

Ghost's typed-receiver recall is below our own 0.85 bar. Those receivers get
their type through chained calls and getters, and we publish the number
anyway. The benchmark, ground truth, and scoring are open, and other
code-graph tools get adapters their maintainers can correct.

Everything runs locally. No model builds the graph, and the same code gives
the same answer every time.

- Try it: `npx tessera-codegraph refs <symbol>`
- Accuracy details: docs/accuracy.md
- Benchmark: github.com/iamsaquib8/tessera-bench
