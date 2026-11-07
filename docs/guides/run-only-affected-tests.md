# Run only the affected tests

A full test run takes too long for every commit, and you do not want to guess
which tests a change can break.

The examples use the TypeScript sample from the [quickstart](../quickstart.md).

## List the tests a change reaches

`tessera affected` compares the working tree with `HEAD`. Give it a revision
to compare with another, such as `origin/main` in CI. Change the body of
`formatName`:

```sh
perl -pi -e 's/return name\.trim\(\);/return name.trim().normalize();/' src/util/format.ts
```

```console
$ tessera affected
1 of 1 test files selected (0 skipped) for changes since HEAD
test/repo.test.ts
$ tessera affected --explain test/repo.test.ts
test/repo.test.ts is selected (reached): describe("loadUser").it("returns the user") → loadUser → UserRepo.findById → formatName
```

The summary goes to stderr, so stdout is only the list. A test is selected
when it reaches changed code through calls, references or imports, or through
a contract such as a route that a client in one service calls and another
serves. Undo the change with `git checkout .`, then edit a file no test
reaches, here `src/util/i18n.ts`, and nothing is selected:

```sh
echo "// note" >> src/util/i18n.ts
```

```console
$ tessera affected
0 of 1 test files selected (1 skipped) for changes since HEAD
```

## Run the whole suite when Tessera says to

Some changes can affect any test, and Tessera says so instead of narrowing.
Undo the last edit, then commit a `package.json` and change it:

```sh
git checkout . && echo '{"name":"sample"}' > package.json
git add package.json && git commit -qm pkg
echo '{"name":"sample","version":"2"}' > package.json
```

```console
$ tessera affected
1 of 1 test files selected (0 skipped) for changes since HEAD
full run: package.json changed (a build file)
test/repo.test.ts
```

Lockfiles, build files, test-runner and CI configuration, and fixture or
snapshot directories also trigger a full run, as does a `Tessera-No-Skip:`
trailer in a commit message. The exit code is 0 whatever is selected.

## Print the runner's command

```console
$ tessera affected --format jest
jest --runTestsByPath test/repo.test.ts
```

`vitest`, `pytest`, `go-test` and `cargo` formats exist too, and `--package`
limits the selection to one workspace package.

## Use it in CI

With the GitHub Action, the selection comes out as step outputs. Run
everything when `full-run` is set, otherwise the list:

```yaml
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0   # so a Tessera-No-Skip trailer in the PR counts
      - uses: iamsaquib8/tessera/action@v1
        id: tessera
      - name: Tests
        env:
          FULL_RUN: ${{ steps.tessera.outputs.full-run }}
          TESTS: ${{ steps.tessera.outputs.tests-file }}
        run: |
          if [ -n "$FULL_RUN" ]; then
            npx jest
          elif [ -s "$TESTS" ]; then
            xargs npx jest --runTestsByPath < "$TESTS"
          fi
```

For other CI systems, a shell version that runs everything when Tessera fails
is in [Tessera in CI](../ci.md#test-selection-tessera-affected).

## What it does not cover

- It selects test files by what they reach in the index. A test that reads a
  file or a service through no call Tessera sees is not selected, which is why
  fixture and snapshot changes run everything.
- Calls Tessera cannot follow (dynamic dispatch, reflection, code generated at
  build time) hide tests from it. Run the full suite on your main branch
  regularly; selection is for pull requests.
- In a shallow clone Tessera cannot read the `Tessera-No-Skip:` trailer and
  says so. See [when not to rely on Tessera](../when-not-to-use.md).

## Next

- [Review a pull request with a deterministic report](review-a-pull-request.md)
- Reference: [Tessera in CI](../ci.md), including the index cache
