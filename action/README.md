# Tessera GitHub Action

Comments on each pull request with what the change breaks — calls to names
that do not exist, wrong argument counts, broken imports, routes or gRPC
methods a client still uses — plus the callers of the changed code and the
tests to run. One comment per pull request, updated on every push.

```yaml
name: Tessera
on: pull_request
permissions:
  contents: read
  pull-requests: write
jobs:
  tessera:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: iamsaquib8/tessera/action@v1
        with:
          fail-on: blocking   # or `never` (default) to only comment
```

| Input | Default | |
| --- | --- | --- |
| `version` | the release this action ships with | Release to install, or `latest` |
| `binary` | | A tessera binary to use instead (for example one built in the job) |
| `base` | the pull request's base | Revision to compare against |
| `fail-on` | `never` | `blocking` fails the job on high-confidence problems |
| `comment` | `true` | Post or update the comment; forks fall back to the job summary |
| `token` | `github.token` | Token used to comment |
| `upload-sarif` | `false` | `true` also uploads the findings to code scanning (needs `security-events: write`) |

Findings in the baseline (`tessera baseline`) or accepted with a reason
(`tessera accept`) never count as blocking, so `fail-on: blocking` fails
only on problems the pull request introduced.

| Output | |
| --- | --- |
| `blocking` | `true` when the report has blocking problems |
| `tests` | The test files the change reaches, one path per line (`tessera affected`) |
| `tests-file` | A file holding the same list |
| `full-run` | Why every test should run, when one should; empty otherwise |

## Running only the tests the change reaches

`full-run` is set when a lockfile, build file or CI configuration
changed, a commit carries a `Tessera-No-Skip` trailer, the index holds no
test files, or Tessera could not select. Run everything then; otherwise
run the list:

```yaml
      - uses: iamsaquib8/tessera/action@v1
        id: tessera
      - name: Tests
        env:
          FULL_RUN: ${{ steps.tessera.outputs.full-run }}
          TESTS: ${{ steps.tessera.outputs.tests-file }}
        run: |
          if [ -n "$FULL_RUN" ]; then
            echo "Running every test: $FULL_RUN"
            npx jest
          elif [ -s "$TESTS" ]; then
            xargs npx jest --runTestsByPath < "$TESTS"
          else
            echo "The change reaches no test."
          fi
```

`tessera affected --format jest | vitest | pytest | go-test | cargo`
prints the runner's command line instead; [docs/ci.md](../docs/ci.md)
has the details and the safety rails.

## Code scanning

With `upload-sarif: true`, the findings also appear as code scanning
alerts on the pull request (category `tessera`), each with a fingerprint
that survives edits which only move lines. The job needs permission to
write security events:

```yaml
permissions:
  contents: read
  pull-requests: write
  security-events: write
jobs:
  tessera:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: iamsaquib8/tessera/action@v1
        with:
          upload-sarif: true
```

A fork's token cannot write security events; there the upload is skipped
and the comment still works. Off by default, so existing workflows keep
the permissions they have.

## Index cache

The index is cached with `actions/cache`, keyed on the tree of the merge
base, the Tessera version and the runner's OS. Before indexing, the
Action imports the closest cache (`tessera cache import`), which
re-indexes only the files that differ from the checkout; after the
report, it exports the index (`tessera cache export`) when the exact key
missed. A cache from another Tessera version or configuration is
refused and the index is built from scratch; the job does not fail.

GitHub shares caches from the default branch with every pull request,
but not caches from one pull request with another. To give each new pull
request a warm cache, run the Action on pushes to the default branch too:

```yaml
on:
  pull_request:
  push:
    branches: [main]
# …
      - uses: iamsaquib8/tessera/action@v1
        with:
          base: ${{ github.event.pull_request.base.sha || github.event.before }}
```

On a push there is no pull request to comment on, so the report goes to
the job summary. [docs/ci.md](../docs/ci.md) covers the cache file and the
report formats for other CI systems.
