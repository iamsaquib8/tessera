# Quickstart

Install the package:

```sh
cargo install tessera-codegraph
```

Index a repository:

```sh
cd path/to/project
tessera index .
```

Re-running `tessera index .` reuses unchanged files via sha-diff. Use `--full` to rebuild from scratch.
Run `tessera watch .` while editing to keep the index fresh automatically.

Ask graph questions:

```sh
tessera find-definition findById
tessera find-references findById
tessera get-outline src
tessera expand-symbol findById
tessera impact findById
tessera watch . --poll-ms 500
tessera validate findByIdd
tessera tests-for findById
tessera stats
tessera search '*Repository*' --kind class --language java
tessera search parseFrom --language java
tessera search 'init*' --kind method --exported
tessera unused --kind function --exported=false
tessera context-pack findById --budget 1500
tessera diff-impact origin/main
tessera imports src/users.ts
tessera imported-by ./users
tessera signature UserService
tessera siblings findById
```

Validate a snippet (stdin):

```sh
echo 'findByIdd(1)' | tessera validate-snippet --language typescript
```

Use JSON output for scripts or agents:

```sh
tessera impact findById --json
```

Use a custom database path:

```sh
tessera index . --db /tmp/project.tessera.db
tessera get-outline src --db /tmp/project.tessera.db
```

Generate the perf chart used in the README:

```sh
tessera bench --path examples/sample
tessera bench --out docs/benchmarks.md       # write the chart to disk
```

Rebuild the memory-mapped snapshot manually:

```sh
tessera snapshot
```

Try the bundled sample:

```sh
tessera index examples/sample --db /tmp/tessera-sample.db
tessera find-definition findById --db /tmp/tessera-sample.db
tessera impact findById --db /tmp/tessera-sample.db
```
