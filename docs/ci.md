# Tessera in CI

Tessera is one binary with no service behind it and no network calls, so
it runs in any CI. The [GitHub Action](../action/README.md) wires up
everything on this page for GitHub; the sections below are for other CI
systems, or for doing it by hand.

## Test selection: `tessera affected`

`tessera affected [REV]` lists the test files a change reaches, each with
the reason it runs. The change is the working tree against `REV` (default
`HEAD`), or the staged files with `--staged`.

```text
$ tessera affected origin/main
1 of 40 test files selected (39 skipped) for changes since origin/main
test/repo.test.ts
$ tessera affected origin/main --explain test/repo.test.ts
test/repo.test.ts is selected (reached): describe("loadUser").it("returns the user") → loadUser → UserRepo
```

A test is selected when:

| Kind | Why |
| --- | --- |
| `changed` | The change adds or edits the test file |
| `always` | It is listed in the file given to `--failed` (tests that failed in an earlier run, one path per line) |
| `reached` | It reaches changed code through calls, references, imports, or a contract the [contract map](contracts.md) links (a client in one service, the handler in another) within `--depth` steps (default 6). Removing or renaming a function selects the tests of its callers; deleting a file selects the tests of its importers; changing a contract definition (`.proto`, OpenAPI, GraphQL schema, SQL, configuration) selects the tests on both sides of each contract it declares |
| `full-run` | The whole suite runs; the reason says why |

The whole suite runs, with the reason, when the change touches:

- a lockfile (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`,
  `Cargo.lock`, `go.sum`, `poetry.lock`, `uv.lock` and others);
- a build file (`package.json`, `Cargo.toml`, `go.mod`, `pom.xml`,
  `build.gradle`, `pyproject.toml`, `requirements*.txt` and others);
- test runner, toolchain or CI configuration (`jest.config.*`,
  `vitest.config.*`, `conftest.py`, `pytest.ini`, `.nvmrc`,
  `rust-toolchain.toml`, `.github/workflows/` and others);
- a fixture or snapshot directory (`fixtures/`, `testdata/`,
  `__snapshots__/` and others), since tests read those through no call
  the index sees.

It also runs when a commit in the range has a `Tessera-No-Skip:` trailer,
or when the change reaches more symbols than Tessera narrows (the
selection would be most of the suite anyway). When the index holds no
test files, the output says so and selects nothing: run everything then.

`--format` prints a runner's command line instead of the list:

| Format | Prints |
| --- | --- |
| `list` (default) | One test path per line |
| `json` | Every test with its kind, reason, package and the chain from the test to the changed code |
| `jest`, `vitest` | `jest --runTestsByPath …`, `vitest run …` |
| `pytest` | `pytest file::Class::test …`, narrowed to the test functions that reach the change |
| `go-test` | `go test <packages> -run '^(TestA\|TestB)$'` per module |
| `cargo` | `cargo test -p <package> --test <target>` |

`--package <name>` restricts the selection to one workspace package (npm,
pnpm or yarn workspaces, Cargo, Go or Maven modules). The summary goes to
stderr, so stdout is only the list or the command. The exit code is 0
whatever is selected; 2 for a usage error (no git repository, an unknown
revision).

The [GitHub Action](../action/README.md#running-only-the-tests-the-change-reaches)
exposes the selection as outputs (`tests`, `tests-file`, `full-run`). In
another CI:

```sh
# Run everything when Tessera says so, knows no tests, or fails.
if ! tessera affected "$BASE" > tests.txt 2> affected.log ||
   grep -q -e '^full run: ' -e '^0 of 0 test files' affected.log; then
  npx jest
elif [ -s tests.txt ]; then
  xargs npx jest --runTestsByPath < tests.txt
fi
```

Over MCP the same selection is the `affected` tool.

## Index cache

Building the index is the slow part of a CI run; checking a change
against it is fast. `tessera cache export` and `tessera cache import`
move a built index between machines as one file, so a run starts from the
index of the commit it branched from and re-indexes only what changed.
Tessera only reads and writes the file: storing it (`actions/cache`,
GitLab's `cache:`, Azure's `Cache@2`, a bucket) is the CI's job.

```sh
tessera cache export "$CACHE_DIR/tessera.cache"   # after a run
tessera cache import "$CACHE_DIR/tessera.cache"   # before the next one
```

**Export** brings the index up to date, then writes a consistent snapshot
of it (taken while other processes may still be writing), compressed with
zstd. The file appears whole or not at all: it is written beside the
target and renamed into place. Its first two lines say what it holds:

```text
tessera-cache
{"format":1,"version":"1.2.0","schema":9,"fingerprint":"…","excludes":[],"tree":"4b825dc…","root":".","files":1832,"size":41254912,"sha256":"…"}
```

`tree` is `git rev-parse HEAD^{tree}` where it was written (`null`
outside git); `root` is where the indexed directory sits in its
repository (`.` for the top, `packages/api` for one package of a
monorepo).

**Import** refuses a file whole, before the index is touched, when it is:

| Refused when | Why |
| --- | --- |
| missing, not a cache, or damaged (checksum) | nothing to trust |
| in another cache format, or from another Tessera version or index schema | another version extracts and resolves differently |
| of another directory of the repository (`root` differs) | its paths mean other files |
| built under another `.tessera/config.toml` | it indexed other files |
| built with other `--exclude` globs than the index already here | the local choice wins; match it or `--reset-excludes` |
| of another repository: fewer than half its files exist here | a branch moves some files, never most |

A refusal exits 3 with the reason on stderr; with `--json` it also prints
`{"imported": false, "reason": "…"}`. Treat it as a cache miss: the next
command builds the index from scratch.

Otherwise the cache replaces the index in one SQLite transaction, under
the same lock the indexer takes, so a reader sees the old index or the
new one. Then every file's content is compared with the cache's, and only
files that differ, are new, or are gone are re-indexed:

```console
$ tessera --json cache import tessera.cache
{ "imported": true, "file": "tessera.cache", "tree": "4b825dc…", "version": "1.2.0",
  "schema": 9, "files": 1832, "reindexed": 14, "unchanged": 1818, "removed": 0 }
```

**Keys.** Key the stored file on the merge base's tree, the Tessera
version and the OS (line endings differ), and restore by the prefix
without the tree, so a branch takes the newest cache when its exact merge
base has none:

```sh
tree=$(git rev-parse "$(git merge-base HEAD "$BASE")^{tree}")
key="tessera-$(uname -s)-$(tessera --version | cut -d' ' -f2)-$tree"
```

Any cache is safe to import: a stale one costs re-indexing, and one from
another version or configuration is refused. A cache only saves time when
it was built from something close to the change, which is why the key
starts from the merge base. Saving from the default branch's pipeline
gives every new branch a warm start.

GitLab CI, where cache keys are fixed before the job runs:

```yaml
tessera:
  variables:
    TESSERA_VERSION: "1.2.0"
    GIT_DEPTH: 0
  cache:
    key: tessera-$TESSERA_VERSION
    paths: [.tessera-cache/]
  script:
    # A missing or refused cache only means indexing from scratch.
    - tessera cache import .tessera-cache/index || true
    - tessera verify --diff "$CI_MERGE_REQUEST_DIFF_BASE_SHA" --format gitlab > gl-code-quality.json || code=$?
    - tessera cache export .tessera-cache/index
    - exit ${code:-0}
```

## Report formats

`tessera verify --changed`, `--staged` or `--diff <rev>` takes
`--format`:

| Format | For | Shape |
| --- | --- | --- |
| `human` | a terminal (default) | text |
| `agent` | hooks and agents | one line per blocking problem |
| `json` | scripts | the `--json` object |
| `markdown` | a pull-request comment | what the Action posts |
| `sarif` | GitHub code scanning, any SARIF viewer | SARIF 2.1.0 |
| `gitlab` | GitLab merge request widget | Code Quality JSON array |
| `azure` | Azure DevOps pipelines | `##vso[task.logissue]` logging commands |
| `bitbucket` | Bitbucket Code Insights | report and annotations JSON |
| `ndjson` | agent loops reading a stream | one finding per line, then a summary |

Every format exits as `verify` does: 1 when a problem blocks, 0
otherwise, 3 when the index cannot be read. Each finding carries a
`fingerprint` (16 hex digits) that stays the same when edits only move
lines; the review formats use it so a tool tracks one issue across
pushes. When one problem occurs at several lines (the same missing name
called twice), the second and later occurrences get `-2`, `-3` … in the
GitLab fingerprint, the Bitbucket `external_id` and the SARIF partial
fingerprint, so each stays its own issue.

V6 (a test that reaches the change) is a test to run, not a problem: the
GitLab, Azure and Bitbucket formats leave it out; SARIF lists it as a
note, NDJSON as a finding.

**SARIF.** Results carry `partialFingerprints: {"tesseraFingerprint/v1":
…}`, and artifact URIs are percent-encoded relative paths
(`src/my%20dir/a.ts`). With the Action, set `upload-sarif: true`;
elsewhere, upload the file with your code scanning tool.

**GitLab.** Each issue has `description`, `check_name` (`V1`…`V5`),
`fingerprint`, `severity` (`major` when it blocks, `minor` when it is
advisory) and `location.path` / `location.lines.begin`:

```yaml
tessera:
  script:
    - tessera verify --diff "$CI_MERGE_REQUEST_DIFF_BASE_SHA" --format gitlab > gl-code-quality.json
  artifacts:
    when: always
    reports:
      codequality: gl-code-quality.json
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
```

**Azure DevOps.** One logging command per problem, `type=error` when it
blocks and `warning` when it is advisory, with `sourcepath`,
`linenumber` and `code`. Property values escape `%` (as `%AZP25`), line
breaks, `;` and `]`; the message after the first `]` is never split, so
it escapes only `%` and line breaks:

```yaml
- script: tessera verify --diff "$(BASE)" --format azure
  displayName: Tessera
```

**Bitbucket.** `{"report": …, "annotations": […]}`: the report (`result`
`PASSED` or `FAILED`, with counts) and one annotation per problem
(`external_id`, `annotation_type: BUG`, `severity` `HIGH` or `LOW`,
`summary`, `path`, `line`). Send each half to its API call; Pipelines
authenticates through its local proxy:

```yaml
- step:
    script:
      - git fetch origin "$BITBUCKET_PR_DESTINATION_BRANCH"
      - |
        tessera verify --diff "origin/$BITBUCKET_PR_DESTINATION_BRANCH" --format bitbucket > tessera.json && code=0 || code=$?
        api="http://api.bitbucket.org/2.0/repositories/$BITBUCKET_REPO_FULL_NAME/commit/$BITBUCKET_COMMIT/reports/tessera"
        jq .report tessera.json | curl -sf --proxy http://localhost:29418 -X PUT "$api" -H 'Content-Type: application/json' -d @-
        jq .annotations tessera.json | curl -sf --proxy http://localhost:29418 -X POST "$api/annotations" -H 'Content-Type: application/json' -d @-
        exit $code
```

Bitbucket takes at most 100 annotations per request; split larger lists
with `jq '.annotations[0:100]'` and so on.

**NDJSON.** Each line is a finding exactly as in `--json`, and the last
line is `{"summary": {"ok", "findings", "blocking", "tests"}}`, so a loop
can act on each finding as it reads it:

```sh
tessera verify --changed --format ndjson | jq -c 'select(.blocking == true)'
```

## Baseline and accepted breaks

On a repository that already has problems, turning on a check that fails
the build on every old one is how checks get turned off. Record what is
there today, and only new problems block:

```sh
tessera baseline                 # writes .tessera/baseline.json; commit it
tessera verify --diff origin/main
```

A finding in the baseline is still reported, marked `(in the baseline)`,
with `waived: {"by": "baseline"}` in JSON, and it does not block: not in
`verify`, not in the Action's `fail-on: blocking`, not in the edit hooks.
`tessera baseline --prune` drops entries that no longer fire, and
`verify --no-baseline` shows everything.

An intended break, such as an endpoint removed on purpose while its last
client is migrated, gets a reviewed record instead of a disabled check:

```sh
tessera accept 3f9a0c1e8b2d4471 --reason "orders v1 retired; web moves in #412" --expires 2026-12-31 --by sam
```

Fingerprints come from `verify --json` (every finding has one; it stays the
same when lines move). The entry goes to `.tessera/accepted.toml`, which is
committed and reviewed like code. Until it expires, the finding is
reported with its reason and does not block; after that it blocks again.

Both files are read leniently in one direction only: a file that cannot be
read is named on stderr and waives nothing, so a broken file never turns
checks off.
