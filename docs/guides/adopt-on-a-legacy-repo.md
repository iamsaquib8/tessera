# Adopt on a legacy repository

Your repository already has broken calls. A check that fails the build on
every old one gets switched off in a week. You want only the problems a change
introduces to block.

The examples use the TypeScript sample from the [quickstart](../quickstart.md),
with one call that is already broken, committed as legacy code:

```sh
perl -pi -e 's/repo\.findById\(id\)/repo.findOne(id)/' src/users/repo.ts
git commit -qam "legacy: a call that is already broken"
```

Now touch the file, as a pull request that edits it would:

```sh
echo "// touched" >> src/users/repo.ts
```

```console
$ tessera verify --changed
✗ verify: 1 problem(s)
  [V1] src/users/repo.ts:17 `findOne` is not defined in src/users/repo.ts; did you mean UserRepo.findById?
```

That problem is not the change's fault, but it fails the check.

## Record what is there today

```console
$ tessera baseline
Baseline: 1 finding(s) in .tessera/baseline.json. Only new findings block from now on.
$ tessera verify --changed
✗ verify: 1 problem(s), 0 blocking
  [V1] src/users/repo.ts:17 `findOne` is not defined in src/users/repo.ts; did you mean UserRepo.findById? (in the baseline)
```

The finding is still reported, marked `(in the baseline)`, and no longer
blocks: not in `verify`, not in the Action's `fail-on: blocking`, not in the
edit hooks. Commit `.tessera/baseline.json`; the index and the rest of
`.tessera/` are git-ignored. When someone fixes an old problem, `tessera baseline --prune`
drops the entries that no longer fire, and `tessera verify --no-baseline` shows
everything.

A finding's fingerprint stays the same when edits only move its line, so the
baseline survives unrelated changes to the file.

## Accept a break you mean to make

A new problem blocks. Give `formatName` a second parameter, and callers that
still pass one argument are new problems:

```console
$ tessera verify --changed
✗ verify: 3 problem(s), 2 blocking
  [V2] src/users/repo.ts:7 `formatName` takes 2 arguments; this call passes 1
  [V2] src/util/format.ts:6 `formatName` takes 2 arguments; this call passes 1
  [V1] src/users/repo.ts:17 `findOne` is not defined in src/users/repo.ts; did you mean UserRepo.findById? (in the baseline)
```

Suppose the first one is intended: the argument is added in two steps, and the
callers move in a later pull request. Do not turn the check off. Record the
break, with a reason and an end date. The fingerprint comes from
`tessera verify --json`:

```console
$ tessera accept 6068b8c9925e26b2 --reason "locale argument is added in two steps; callers move in #412" --expires 2026-12-31 --by sam
Accepted 6068b8c9925e26b2 in …/.tessera/accepted.toml.
$ cat .tessera/accepted.toml
[[accept]]
fingerprint = "6068b8c9925e26b2"
reason = "locale argument is added in two steps; callers move in #412"
expires = "2026-12-31"
by = "sam"
$ tessera verify --changed
✗ verify: 3 problem(s), 1 blocking
  [V2] src/util/format.ts:6 `formatName` takes 2 arguments; this call passes 1
  [V1] src/users/repo.ts:17 `findOne` is not defined in src/users/repo.ts; did you mean UserRepo.findById? (in the baseline)
  [V2] src/users/repo.ts:7 `formatName` takes 2 arguments; this call passes 1 (accepted: locale argument is added in two steps; callers move in #412)
```

`accepted.toml` is committed and reviewed like code. The accepted finding shows
its reason in the terminal and in the pull request comment, and blocks again
after the `expires` day (UTC).

## What it does not cover

- The baseline holds blocking findings only. Advisory findings are never
  recorded because they never block.
- A broken or unreadable `baseline.json` or `accepted.toml` is named on stderr
  and waives nothing, so a bad file cannot turn the checks off.
- `accept` takes one finding's fingerprint at a time. It does not silence a
  check, a file or a kind of problem.

## Next

- [Review a pull request](review-a-pull-request.md) with `fail-on: blocking`
- Reference: [baseline and accepted breaks](../ci.md#baseline-and-accepted-breaks)
