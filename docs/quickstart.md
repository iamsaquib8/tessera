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

Ask graph questions:

```sh
tessera find-definition findById
tessera find-references findById
tessera get-outline src
tessera expand-symbol findById
tessera impact findById
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

Try the bundled sample:

```sh
tessera index examples/sample --db /tmp/tessera-sample.db
tessera find-definition findById --db /tmp/tessera-sample.db
tessera impact findById --db /tmp/tessera-sample.db
```
