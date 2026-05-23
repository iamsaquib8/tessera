# Benchmarks

Tessera ships a built-in bench harness so the numbers in the README aren't theoretical. Run it yourself:

```sh
tessera bench --path /path/to/your/repo     # real repo
tessera bench --scale 200                   # synthetic 200-file "popular utility" repo
tessera bench --path . --out docs/benchmarks.md   # write the chart to disk
```

## Headline: real production Java service

951 files, 16,368 symbols, 129,959 references. Measured on an M-series Mac, release build, cold cache.

```
Tessera v0.2.0 bench
─────────────────────
  951 files · 16,368 symbols · 129,959 references

Index time
  full         ████████████████████████████████    4,689 ms
  incremental  █                                      66 ms   ·  71× faster

"who calls parseFrom?"
  raw grep + read   ████████████████████████████████   394,140 tokens
  tessera           █                                    6,530 tokens   ·  60× cheaper

"where is parseFrom defined?"
  raw grep + read   ████████████████████████████████     2,766 tokens
  tessera           █████████████████████                1,781 tokens   ·  2× cheaper

Per-query latency  ·  median of 3 runs
  find_definition      3 ms     ~1,781 tokens
  find_references     13 ms     ~16,144 tokens
  get_outline          9 ms     ~78,276 tokens
  impact             617 ms     ~6,530 tokens
  validate             2 ms     ~   48 tokens
```

### How to read it

- **Index time.** Full index is one-time work (or post-clone hook); incremental rerun is what runs after every save. 66 ms on a near-million-LOC Java codebase comfortably meets the plan's <200 ms target.
- **"who calls X?" savings.** The honest story. The raw baseline is "agent runs ripgrep, then reads every file that matches" — the real workflow today. Tessera returns a structured caller list with PageRank-ranked criticality and a single `_meta` block. For hot symbols, **two orders of magnitude cheaper**.
- **Per-query latency.** `impact` is the most expensive query because it runs personalised PageRank over a depth-4 subgraph; on hub symbols with 1,500+ callers we cap the frontier at 800 to keep latency bounded. Everything else stays under 20 ms.

## Methodology

For each run, `tessera bench`:

1. **Cold index** the target path with `--full` into a temporary SQLite DB.
2. **Incremental rerun** the index against the same path with no file changes (exercises the sha-diff skip path).
3. Time three repetitions of each query (`find_definition`, `find_references`, `get_outline`, `impact`, `validate`) against an automatically chosen probe — the most-called function/method in the index, override with `--probe`.
4. Compute two savings ratios:
   - **"who calls X?"** — raw baseline is the total token cost of every source file that contains the symbol name (the work an agent does with grep + read every match). Tessera's number is the size of the `impact` response.
   - **"where is X defined?"** — raw baseline is the mean source-file token count (the work to read one file to confirm a definition). Tessera's number is the size of the `find_definition` response.
5. Render an ASCII bar chart.

The raw token count is approximated as `bytes / 4`, matching the heuristic Tessera uses for its own `_meta.tokens_returned_estimate`. Real tokeniser counts (tiktoken, sentencepiece) vary by ±20 %, so treat ratios as the source of truth rather than absolute numbers.

## Synthetic repo

`tessera bench` with no `--path` generates a deterministic synthetic TypeScript repo (default 50 files; override with `--scale`). It models a "popular utility" topology: every `module_i.ts` calls `sharedHelper` from `util.ts`, plus the two neighbour bridges. This mirrors how high-impact refactors really cascade — a small set of helpers called from many sites.

The synthetic repo is intentionally smaller per-file than real code, so its absolute savings are modest. It's there for **reproducibility**, not headline numbers. For meaningful savings, point at a real repo.

## Reproducibility

The synthetic generator (`generate_synthetic_repo` in `src/bench.rs`) is deterministic — anyone with `cargo install tessera-codegraph` should see comparable shapes. For real-repo numbers, run against any open-source codebase you care about. Numbers will scale roughly linearly with file count for indexing, and roughly with caller fan-out for `impact`.
