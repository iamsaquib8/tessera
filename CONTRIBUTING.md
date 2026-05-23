# Contributing

Thanks for helping make Tessera better. The project is still young, so the best contributions are small, well-tested improvements that make the graph more accurate or the CLI easier to use.

## Setup

```sh
git clone https://github.com/iamsaquib8/tessera
cd tessera
cargo test
```

## Development Loop

Run the same checks CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

For local smoke testing:

```sh
cargo run -- index examples/sample --db /tmp/tessera.db
cargo run -- impact findById --db /tmp/tessera.db
```

## Good First Contributions

- Add a compact fixture for a parser edge case.
- Improve symbol extraction for TypeScript, JavaScript, or Python.
- Add a query test for definitions, references, outlines, expansion, or impact.
- Improve docs where setup or MCP configuration is unclear.

## Pull Request Guidelines

- Keep the PR scoped to one behavior or one documentation improvement.
- Add or update tests for parser and query changes.
- Preserve the `tessera` binary name even though the crates.io package is `tessera-codegraph`.
- Avoid broad refactors unless they unlock a concrete feature or fix.

## Release Checklist

Maintainers should run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo publish --dry-run
```

Then publish:

```sh
cargo publish
```
