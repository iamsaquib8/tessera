# Contributing to Tessera

Small, tested pull requests are the most useful. The three most valuable kinds
are each self-contained:

1. **A language**: add or deepen extraction for a grammar
   ([guide](docs/how-to-add-a-language.md)).
2. **An agent integration**: document or wire up an agent
   ([guide](docs/how-to-add-an-agent.md)).
3. **A fix with a fixture**: anything Tessera got wrong on real code, reduced
   to a few lines in `tests/fixtures/`.

Issues labelled `good first issue` are scoped to fit one of these.

## Setup

```sh
git clone https://github.com/iamsaquib8/tessera
cd tessera
cargo test            # Rust 1.90+
```

## The checks CI runs

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --release --test perf -- --ignored   # perf budgets
node --test npm/test/                            # npm launcher
```

## How the tests are organised

| File | Covers |
| --- | --- |
| `tests/golden.rs` | Snapshot of every symbol, reference, import, and edge for each language fixture. Run `INSTA_UPDATE=always cargo test --test golden`, then review the diff with `git diff tests/snapshots`. |
| `tests/lib_api.rs` | The eight query operations through the library API |
| `tests/cli.rs` | CLI behaviour, exit codes, JSON output |
| `tests/mcp_conformance.rs` | MCP over stdio and HTTP, driven against the built binary |
| `tests/robustness.rs` | Adversarial input, determinism, concurrency |
| `tests/perf.rs` | Time budgets (ignored by default) |

When you change what Tessera extracts, the golden snapshot diff is the review.
Explain each changed line in your PR description.

## Code style

- Match the surrounding code.
- No multi-line comment blocks. At most one short line, and only when the
  reason is not obvious.
- Commit messages use Conventional Commits prefixes (`feat:`, `fix:`,
  `docs:`, …) with a plain-English body explaining why.

## Pull requests

- One change per PR, with the test that proves it.
- Run the checks above before you push.
- New user-facing behaviour needs a line in `CHANGELOG.md` and, if it changes
  a command, the matching page in `docs/`.

By contributing you agree your work is licensed under Apache-2.0.
