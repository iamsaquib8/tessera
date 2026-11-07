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
cargo test --all-targets --all-features --no-fail-fast
cargo test --release --test perf -- --ignored --test-threads=1   # perf budgets
node --test npm/test/*.test.js scripts/*.test.js  # npm launcher and scripts
node scripts/changelog.js check                   # changelog fragments
(cd website && npm ci && npm run build)           # docs site
cargo check --locked --all-targets                # on Rust 1.90, the minimum
cargo deny check                                  # advisories, licenses, sources
python -m pip install -r tests/junit-requirements.txt
cargo build --locked
python tests/junit_schema.py target/debug/tessera # JUnit schema and CLI
```

`.github/workflows/ci.yml` runs these as ten jobs:

| Job | What it checks |
| --- | --- |
| `lint` | `cargo fmt --check` and clippy with warnings denied |
| `test` | The whole test suite on Linux (x86_64 and arm64), macOS and Windows |
| `junit` | The JUnit output against its schema |
| `npm` | The npm launcher and the repository's scripts on Node 18, 20 and 22, and the changelog fragments |
| `docs-site` | That the documentation site in `website/` builds |
| `msrv` | That the crate checks on the minimum supported Rust (`rust-version` in `Cargo.toml`) |
| `musl` | The static Linux builds, x86_64 and arm64 |
| `deny` | `cargo deny check`: advisories, licenses, sources and banned crates (`deny.toml`) |
| `notices` | That `THIRD-PARTY-NOTICES` is up to date with `Cargo.lock` |
| `perf` | The time budgets in `tests/perf.rs` |

## How the tests are organised

| File | Covers |
| --- | --- |
| `tests/golden.rs` | Snapshot of every symbol, reference, import, and edge for each language fixture. Run `INSTA_UPDATE=always cargo test --test golden`, then review the diff with `git diff tests/snapshots`. |
| `tests/lib_api.rs` | The eight query operations through the library API |
| `tests/cli.rs` | CLI behaviour, exit codes, JSON output |
| `tests/mcp_conformance.rs` | MCP over stdio and HTTP, driven against the built binary |
| `tests/robustness.rs` | Adversarial input, determinism, concurrency |
| `tests/perf.rs` | Time budgets (ignored by default) |
| `tests/resolution.rs` | How calls and imports resolve to definitions |
| `tests/v06_correctness.rs` | Indexing edge cases: config include/exclude, bad UTF-8, long signatures |
| `tests/contracts.rs` | The contract map: routes, queues, tables and gRPC across repositories |
| `tests/cycles.rs` | Dependency cycle detection |
| `tests/verify.rs` | `tessera verify`: findings, exit codes and the tests it lists |
| `tests/baseline.rs` | Baselines and accepted findings |
| `tests/affected.rs` | `tessera affected` and its safety rails |
| `tests/cache.rs` | `cache export` and `import` |
| `tests/hook.rs` | Agent hooks (`tessera hook`) |
| `tests/init.rs` | `tessera init` for each coding agent |
| `tests/plugin.rs` | The Claude Code plugin and marketplace files |
| `tests/ui.rs` | `tessera ui` and its static export |
| `tests/docs.rs` | That the commands in the README run |
| `tests/stability.rs` | The 1.x contract in `docs/stability.md`, pinned under `tests/stability/` |
| `tests/action.rs` | The GitHub Action's steps, run against fixture repositories |
| `tests/install_script.rs` | `install.sh` against a fake release server |
| `tests/release.rs` | Package versions, the release workflow and the npm packages |
| `tests/release_major_tag.rs` | The release job that moves the `v1` tag |
| `tests/release_notes.rs` | Release notes taken from the changelog |
| `tests/ci_matrix.rs` | That CI runs the minimum Rust and Node and Linux on arm64 |
| `tests/dependabot.rs` | That Dependabot watches every dependency source |
| `tests/workflow_hardening.rs` | Pinned actions and least-privilege permissions |
| `tests/package_manifest.rs` | The lints, the size-tuned release profile and the crates.io package file list |
| `tests/deny_config.rs` | `deny.toml` and the jobs that run it |
| `tests/sqlite_version.rs` | That the bundled SQLite is at least the minimum version |
| `tests/pre_commit.rs` | `.pre-commit-hooks.yaml` and the recipes in `docs/integrations.md` |
| `tests/container.rs` | The container image: pinned bases, numeric non-root user, built on pull requests |
| `tests/pages.rs` | The docs site publish job and its version banner |
| `tests/repo_hygiene.rs` | That issue-form labels exist and these contributor docs match CI |

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
- New user-facing behaviour needs a changelog fragment in `changes/` (see
  [changes/README.md](changes/README.md); do not edit `CHANGELOG.md`, so that
  pull requests never conflict there) and, if it changes a command, the
  matching page in `docs/`.
- The GitHub release body is the `CHANGELOG.md` section of the tag. Before
  tagging `vX.Y.Z`, run `node scripts/changelog.js release X.Y.Z`, add the
  summary paragraph, and commit; the release workflow fails without that
  section.

By contributing you agree your work is licensed under Apache-2.0.
