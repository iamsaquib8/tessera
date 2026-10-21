# M1 Precision + Bench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace name-level matching with import- and type-aware resolution for TypeScript, JavaScript and Java, give every edge a confidence, and prove the accuracy publicly with `tessera-bench`.

**Architecture:** Extraction (per file, parallel-safe) records richer facts: import bindings, type hints, receivers, arity, heritage, callbacks. A separate **resolve pass** runs after indexing over the changed files plus their dependents, and writes `edges.to_symbol_id`, `confidence` and `resolver`. Queries walk resolved ids, and fall back to name matching only for edges marked `name`. `tessera-bench` lives in its own repository, `~/dev/tessera-bench` (later `iamsaquib8/tessera-bench`). It scores tools against SCIP ground truth.

**Spec:** `docs/design/2026-10-21-overhaul.md` §5.6, §8 (M1 row).
**Builds on:** `docs/plans/2026-10-21-v1-overhaul.md` (M0) and its ledger.

## Global Constraints

Everything in the M0 plan's Global Constraints still applies. In addition:

- **Confidence values:**
  - `high`: resolved through scope, an import, `this`, a typed receiver, or a static class.
  - `medium`: a unique repo-wide name match.
  - `low`: one of several candidates; every candidate is kept and marked as part of an ambiguous group.
  - `name`: unresolved; the old behaviour.
- **Gate:**
  - TS/JS: high-confidence edge precision ≥ 0.95, and recall ≥ 0.85 for each of the direct-call, typed-receiver and type-reference categories, on the bench corpus.
  - M1 grows until TS/JS passes.
  - Java may ship at the standard tier, but the bar is never lowered.
- **Perf:**
  - Full index of the 45k-file monorepo ≤ 1.5× the M0 time (≤ 66 s).
  - Re-indexing one file, including its resolved dependents, has p95 ≤ 300 ms on a 5k-file repo.
- **Schema:** v5. The index is a cache, so it rebuilds on version mismatch.

## Review Focus

1. **Re-exports and barrels.** `index.ts` re-exporting from five files must resolve through every hop and handle cycles without looping.
2. **Shadowing.** A local `const loadUser = …` shadows an import of the same name.
3. **Overloads.** Java `findById(String)` and `findById(String, boolean)` resolve by argument count.
4. **Incremental correctness.** Renaming an export in file A re-resolves file B, which imports it, without a full re-index.
5. **Huge fan-in.** Re-resolving dependents of a file imported by 5,000 files stays within budget; when it can't, fall back to a scheduled full resolve and say so in `status`.

---

## Tasks

### Task 1: Extraction records v2 and schema v5

**Files:**
- `src/types.rs`, `src/db.rs`, `src/extract/mod.rs`, `src/indexer.rs`
- `tests/golden.rs` (dump includes new tables)

**Produces:**
- **`IndexedSymbol`** gains:
  - `container: Option<String>`, the enclosing class or module qualified name;
  - `arity: Option<(u16, Option<u16>)>`, the required count and the maximum (`None` means variadic);
  - `default_export: bool`.
- **`IndexedReference`** gains:
  - `receiver: Receiver`, where `enum Receiver { None, This, Super, Name(String), Other }`;
  - `arg_count: Option<u16>`;
  - new `kind` values `extends`, `implements`, `decorator`, `type`.
- **`IndexedBinding`** (new) has `local: String`, `imported: Option<String>`, `source: String`, `kind: BindingKind`, `line: usize`, where `BindingKind` is one of: `Named`, `Default`, `Namespace`, `Require`, `RequireMember`, `ReExport`, `ReExportAll`, `JavaSingle`, `JavaWildcard`, `JavaStatic`.
- **`IndexedTypeHint`** (new) has `scope: Option<String>`, `name: String`, `type_name: String`, `origin: HintOrigin`, where `HintOrigin` is one of: `Declared`, `New`, `Param`, `Field`, `Return`.
- **Schema v5:**
  - new tables `import_bindings(file_id, local, imported, source, kind, line)` and `type_hints(file_id, scope, name, type_name, origin)`;
  - new columns `refs.receiver`, `refs.receiver_name`, `refs.arg_count`, `symbols.container`, `symbols.arity_min`, `symbols.arity_max`, `symbols.default_export`, `files.module` (Java package or JS module key);
  - `edges` gains `resolver TEXT` and `ambiguous_group INTEGER`.

**Steps:**
- [ ] Extend `graph_dump` to print bindings, hints, receiver/arity and edge confidence.
- [ ] Watch golden fail on the new fields.
- [ ] Implement storage.
- [ ] Accept snapshots only after reviewing that every new line is right.
- [ ] Commit.

### Task 2: TypeScript/JavaScript extraction depth

**Files:** `src/extract/lang/javascript.rs`, `tests/fixtures/{typescript,tsx,javascript}/` (extend), `tests/fixtures/ts-resolution/` (new)

**Behaviour, each with a golden fixture case:**
- `new X()` records a reference of kind `new` with the receiver set to `X`.
- `class A extends B implements C` records `extends B` and `implements C`.
- `@Decorator()` records a `decorator` reference.
- Type annotations, generics arguments, `satisfies` and `as` record `type` references.
- **Bindings:**
  - ESM: named, default, namespace, and type-only imports.
  - Re-exports: `export { x } from` and `export * from`.
  - CJS: `const x = require()`, `const { a, b: c } = require()`, `module.exports = { … }`, and `exports.foo =`.
- **Type hints:**
  - `const r: Repo`;
  - `const r = new Repo()`;
  - constructor parameter properties (`constructor(private repo: Repo)`);
  - class fields with types;
  - function return types.
- **Arity** comes from formal parameters: required, optional, default, and rest.
- **Callbacks:** an anonymous function passed to a call gets a symbol of kind `callback`, named `<callee>("<first string arg>")` when there is one (for example `it("returns the user")` or `app.get("/send")`), otherwise `<callee>(…)`. Calls inside it attribute to that symbol, so Jest tests and Express handlers become callers.
- **React wrappers:** `memo`, `forwardRef`, and `observer` keep the declarator name. An anonymous `export default` function is named `default`.

**Steps:**
- [ ] Fixture first.
- [ ] Watch the golden test fail.
- [ ] Implement.
- [ ] Review and accept the golden diff.
- [ ] Commit per construct group.

### Task 3: Java extraction depth

**Files:** `src/extract/lang/java.rs`, `tests/fixtures/java/` (extend), `tests/fixtures/java-resolution/` (new)

**Behaviour:**
- The package declaration sets `files.module`.
- Single, wildcard and static imports produce bindings.
- Field, local, parameter and constructor-parameter types produce hints.
- `new X()` records a `new` reference with the receiver.
- `extends`/`implements` are recorded.
- Annotations produce `decorator` references.
- Method arity is recorded; varargs set the maximum to `None`.
- Overloads keep distinct symbols: `qualified_name` stays `Class.method`, and arity disambiguates.

**Steps:**
- [ ] Fixture.
- [ ] Watch it fail.
- [ ] Implement.
- [ ] Accept the reviewed golden diff.
- [ ] Commit.

### Task 4: Module resolver

**Files:**
- `src/resolve/modules.rs` (new)
- `src/resolve/mod.rs` (new)
- tests: `src/resolve/modules.rs` unit tests, `tests/fixtures/ts-resolution/` (tsconfig paths, barrel, workspace package)

**Produces:**

```rust
pub enum ModuleTarget { File(i64), External(String), Unresolved }
pub struct ModuleResolver { /* built once per resolve pass */ }
impl ModuleResolver {
    pub fn build(conn: &Connection, root: &Path) -> Result<Self>;
    pub fn resolve(&self, from_file: &str, source: &str, language: Language) -> ModuleTarget;
}
```

**Resolution rules:**
- **JS/TS relative specifiers:**
  - extensions are probed in the order `ts tsx mts cts js jsx mjs cjs d.ts`;
  - then `index.*` inside a directory;
  - the `.js` → `.ts` source mapping that TypeScript ESM uses is supported.
- **`tsconfig.json`:** `baseUrl` and `paths` with one `*` wildcard; `extends` is followed one level.
- **Workspaces:** `package.json` workspace package names map to their directories, honouring `exports` and `main`.
- **External:** anything else, including `node_modules`.
- **Java:** `files.module` plus a class-name index, so `com.acme.user.UserService` resolves to its file; wildcard imports resolve per class on use.
- **Python (light):** dotted modules map to `a/b.py` or `a/b/__init__.py`; relative `from .x import` is supported.

**Tests** include a cycle: two barrels re-exporting each other must terminate.

### Task 5: Resolve pass

**Files:**
- `src/resolve/mod.rs`, `src/resolve/scope.rs`
- `src/indexer.rs` (call it after storing files)
- `tests/resolution.rs` (new)

**Produces:**

```rust
pub struct ResolveReport { pub files: usize, pub edges: usize, pub high: usize, pub medium: usize, pub low: usize, pub unresolved: usize, pub elapsed_ms: u128 }
pub fn resolve(conn: &Connection, root: &Path, files: ResolveScope) -> Result<ResolveReport>;
pub enum ResolveScope { All, Changed(Vec<i64>) }
```

`Changed` expands to the changed files plus every file whose bindings resolve into them, transitively through re-exports. Above 2,000 dependents it switches to `All` and reports that in `ResolveReport` and `status`.

**Rules, in order:**
- **Bare call:**
  1. a same-file definition visible in scope, giving `high` via `local`;
  2. an import binding, resolved through the module and re-export chain to the exported symbol, giving `high` via `import`;
  3. a unique repo-wide definition, giving `medium` via `name`;
  4. several definitions, giving `low` with an ambiguous group (at most 5 candidates);
  5. otherwise unresolved.
- **`this`/`super`:** the enclosing class's members, then the `extends` chain, giving `high`.
- **Name receiver:**
  - a namespace import's module member gives `high`;
  - a type hint in scope (innermost first) leads to the class member, giving `high`, or `medium` if the hint itself came from a return type;
  - a class name leads to the static member, giving `high`;
  - an unknown receiver falls back to the bare-call rules, capped at `medium`.
- **`new X`:** the class, its constructor if present.
- **Arity filter:** among candidates with arity, drop those incompatible with `arg_count`.

**Tests** cover every rule plus Review Focus items 1–4, with exact expected `(from, to, confidence, resolver)` tuples.

### Task 6: Queries on resolved edges

**Files:** `src/query/{refs,blast,context,verify,search,common}.rs`, `src/query/legacy.rs` (shrinks), `tests/lib_api.rs`

**Behaviour:**
- **`refs`:**
  - finds the definitions, then references whose edge points at those ids;
  - groups them as `high`, `medium` and `possible` (low and name);
  - `--min-confidence high|medium|low` defaults to `low`;
  - output says how many were exact.
- **`blast`:**
  - BFS over `to_symbol_id`;
  - PageRank on the resolved subgraph;
  - each caller carries the lowest confidence on its path.
- **`context`:** callers and callees come from resolved edges.
- **`search --unused`:** checks types again, using `type`/`new`/`extends` references.
- **Output:** the words "name-level matching" appear only when a result includes `name`-confidence edges.
- **Cleanup:** remove the legacy name-only paths that are no longer used.

**Steps:** update tests that expected over-matching. Any expectation that changes needs a ruling in the ledger.

### Task 7: Perf and incremental budgets

**Files:** `tests/perf.rs`

**Tests:**
- full index of `synth_repo(10_000)` with resolution ≤ 1.5× the M0 baseline measured at the start of this task;
- a single-file edit plus dependent re-resolve has p95 ≤ 300 ms on 5k files;
- a hub file imported by every module triggers the `All` fallback and still completes ≤ 2× the full-index time.

Profile with `cargo flamegraph` if a budget fails.

### Task 8: `tessera-bench` scaffold

**Repo:** `~/dev/tessera-bench`, a new git repository. Saquib creates the GitHub remote.

| Path | Content |
| --- | --- |
| `corpus.toml` | Pinned repos and commits. TS/React: excalidraw. TS server: a NestJS app. Plain JS: Ghost (core subset). Java: spring-petclinic and spring-petclinic-microservices. Polyglot: GoogleCloudPlatform/microservices-demo. |
| `scripts/fetch.sh` | Shallow clone at pinned SHAs |
| `scripts/scip.sh` | `scip-typescript index` (npm) and `scip-java` (Docker or coursier) |
| `bench/groundtruth.py` | SCIP → `edges.jsonl` (definition symbol, reference site, category) |
| `bench/sample.py` | Stratified seeded sample of ≥ 200 symbols per language, bucketed by fan-in |
| `bench/adapters/tessera.py` | Runs `tessera --json refs/blast` |
| `bench/adapters/<tool>.py` | Competitor adapters, same interface |
| `bench/score.py` | Precision per confidence tier; recall per category; ambiguity rate; ops metrics |
| `Dockerfile` | Pinned toolchain for reproduction |
| `README.md` | Methodology and how maintainers fix their own adapter |

`groundtruth.py` classifies categories by re-parsing each reference site with tree-sitter: direct, typed-receiver, callback, JSX, dispatch, type-reference.

### Task 9: Gate run and iteration

- [ ] Run the bench on the corpus and write `results/<date>-tessera-<sha>.json` and `results/README.md` tables.
- [ ] While the TS/JS gate fails, file the false positives and false negatives by category, fix the top category in Tasks 2–6 (each fix gets a fixture reproducing it), and re-run. Ledger each iteration with before and after numbers.
- [ ] When the gate passes, run the competitor adapters (latest releases, default configs, code-only mode for LLM-dependent tools) and publish their rows with versions.

### Task 10: Docs and release prep

- README language section: TS/JS (and Java if it passed) move to the deep tier, with a link to the bench results.
- `docs/when-not-to-use.md`, `docs/architecture.md`, `docs/mcp.md`: confidence semantics.
- CHANGELOG 0.11.0.
- Version bump to 0.11.0 on the same branch and PR. The launch post draft goes in `launch/2026-xx-accuracy.md`; publishing it is Saquib's call.
