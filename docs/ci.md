# Tessera in CI

Tessera is one binary with no service behind it and no network calls, so
it runs in any CI. The [GitHub Action](../action/README.md) wires up
everything on this page for GitHub; the sections below are for other CI
systems, or for doing it by hand.

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
