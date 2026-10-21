# Accuracy

Every reference Tessera records carries a confidence. This page says what
each level means and how often it is right, measured against compiler
ground truth on public repositories.

## Confidence levels

| Level | Meaning | Example |
| --- | --- | --- |
| **high** | Resolved through scope, an import (followed through re-exports and barrels), `this`/`super`, a typed receiver, a class name, or a Java package | `this.repo.save()` where `repo: UserRepo` is a declared field |
| **medium** | One definition in the repository has this name, or the receiver's type came from a declared return type | `x.render()` with an untyped `x`, when only one `render` exists |
| **low** | Two to five definitions share the name; all are kept as one ambiguous group | `.load()` with several `load` methods |
| **name** | Unresolved, matched by name only. Output says so | a call on a value whose type Tessera cannot see |

A reference that resolves outside the repository (an npm package, the JDK, a
type such as `Map`) is not reported as a reference to a repository symbol at
all, so `rows.get()` on a `Map` never shows up under your own `get`.

`refs --min-confidence high|medium|low` and the MCP `min_confidence`
argument drop weaker results. `blast` gives each caller the weakest
confidence on its path.

Resolution is deep for **TypeScript, TSX, JavaScript** (ESM and CommonJS),
and **Java** (imports, packages, fields, parameters, `this`, static calls,
and overloads by argument count). Other languages use the same rules where
they apply (scope, unique names) and otherwise stay at name level.

## Measured precision and recall

From [tessera-bench](https://github.com/iamsaquib8/tessera-bench), run on
2026-10-21 against scip-typescript and scip-java ground truth. Precision
counts every reference Tessera reports for 200 sampled definitions per
repository; resolved recall counts ground-truth references Tessera found at
high or medium confidence.

| Repository | high precision | direct calls | typed receivers | type references |
| --- | --- | --- | --- | --- |
| excalidraw (TypeScript, React) | 0.999 | 1.000 | 0.914 | 0.990 |
| nest (TypeScript) | 0.994 | 1.000 | 0.969 | 0.985 |
| ghost core (JavaScript, TypeScript) | 0.990 | 0.879 | 0.698 | 0.963 |
| **TypeScript/JavaScript** | **0.998** | **0.990** | **0.912** | **0.986** |
| spring-petclinic (Java) | 0.995 | 0.900 | 0.958 | 1.000 |
| spring-petclinic-microservices (Java) | 0.995 | 0.778 | 0.800 | 1.000 |
| **Java** | **0.995** | **0.872** | **0.935** | **1.000** |

The release gate for the deep tier is high-confidence precision of at least
0.95 and resolved recall of at least 0.85 for direct calls, typed receivers,
and type references. It passes for TypeScript/JavaScript and for Java.
Two repositories are below it in one category: Ghost's typed receivers (53
sites), whose types come through chained calls (`data.key("x").index(0)`) and
getters that Tessera does not follow yet, and the microservices sample (9
direct calls, 40 typed receivers). The Java corpus is small, so read its
numbers with their sample sizes.

## How it is measured

- **Ground truth:** SCIP indexes built by scip-typescript and scip-java at
  pinned commits. Both attribute `new X()` to X's constructor, and the
  scoring does the same.
  Sites the indexer itself could not resolve, and files it did not index,
  are not scored.
- **Sample:** 40 definitions from each of five fan-in buckets per
  repository, fixed seed.
- **Not measured:** plain identifier reads (`KEYS.A` used as a value), which
  Tessera does not record as references.
- The same harness has adapters for other tools; their rows are published
  with versions and raw output, and maintainers can correct their own
  adapter.

To reproduce, clone tessera-bench and run `scripts/fetch.sh`,
`scripts/scip.sh`, and `python3 -m bench.run tessera`.
