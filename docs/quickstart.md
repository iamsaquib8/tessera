# Quickstart

Install the package:

```sh
npm install -g tessera-codegraph
# or: brew install iamsaquib8/tessera/tessera
# or: curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
# or: docker run --rm -v "$PWD:/work" ghcr.io/iamsaquib8/tessera index /work
# or:
cargo install tessera-codegraph
```

Index a repository:

```sh
cd path/to/project
tessera init --mcp-configs
tessera index .
tessera doctor
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
tessera doctor --json
tessera completions bash
tessera mcp-http --addr 127.0.0.1:8765
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

The v0.5 JSON response schema snapshot lives in
[`docs/json-schemas/tessera-cli-v0.5.schema.json`](json-schemas/tessera-cli-v0.5.schema.json).

For include/exclude rules in larger repos, see [`docs/configuration.md`](configuration.md).

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
