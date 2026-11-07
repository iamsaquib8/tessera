# Review a pull request

You want every pull request to show what it breaks, who calls the code it
changed, and which tests to run, in one comment that comes from a program, not
from a model.

## Add the GitHub Action

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
          fail-on: blocking   # or `never` (the default) to only comment
```

The Action posts one comment per pull request and updates it on every push.
With `fail-on: blocking` the job also fails when the pull request introduces a
high-confidence problem. Problems in the baseline never fail it, see
[adopt on a legacy repository](adopt-on-a-legacy-repo.md). On a fork, where
the token cannot comment, the report goes to the job summary.

## See the report before you push

The comment is the output of `verify --format markdown`. Run it locally to see
what the Action will post. Here, a branch changed `formatName` to take a
second parameter and left its callers alone:

```console
$ tessera verify --diff main --format markdown
<!-- tessera -->
### Tessera: 2 problems to fix

1 changed symbol(s), 5 caller(s), 1 test file(s) to run, 0 contract(s) touched.

| Check | Where | Problem |
| --- | --- | --- |
| V2 | `src/users/repo.ts:7` | `formatName` takes 2 arguments; this call passes 1 |
| V2 | `src/util/format.ts:6` | `formatName` takes 2 arguments; this call passes 1 |

<details><summary>Callers of the changed code (top 5 of 5)</summary>

| Caller | Where | Distance |
| --- | --- | --- |
| `UserRepo.findById` | `src/users/repo.ts:6` | 1 |
| `loadUser` | `src/util/format.ts:5` | 1 |
| `loadUser` | `src/users/repo.ts:15` | 2 |
| `handler` | `src/api/handler.ts:4` | 3 |
| `describe("loadUser").it("returns the user")` | `test/repo.test.ts:4` | 3 |

</details>

<details><summary>Tests to run (1)</summary>

```text
test/repo.test.ts
```

</details>

<sub>From [Tessera](https://github.com/iamsaquib8/tessera): a local, deterministic code graph. Only high-confidence findings block.</sub>
```

GitHub renders the table and folds the callers and tests. The command exits 1,
as it does when the Action fails the job.

## Send findings to code scanning or another CI

Set `upload-sarif: true` in the Action, with `security-events: write`
permission, to also get code scanning alerts. Other CI systems read
`--format sarif`, `gitlab`, `azure`, `bitbucket`, `junit` or `ndjson`; each
finding keeps a fingerprint that survives edits which only move lines. The
recipes are in [Tessera in CI](../ci.md#report-formats).

## What it does not cover

- It reports calls, imports and removed contracts. It does not review design,
  naming or logic, and it is not a type checker; keep your compiler and tests
  in the pipeline.
- Only high-confidence findings fail the job. Lower-confidence ones appear as
  advisory notes under the table.
- Tessera does not ship an LLM review. Its comment is the same for the same
  code, every time. See [when not to rely on Tessera](../when-not-to-use.md).

## Next

- [Run only the tests the pull request reaches](run-only-affected-tests.md)
- [Adopt on a legacy repository](adopt-on-a-legacy-repo.md)
- Reference: [the Action's inputs and outputs](../../action/README.md), [Tessera in CI](../ci.md)
