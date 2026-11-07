# Monorepos and several repositories

Your code is split across packages in one repository, or across repositories
that call each other, and a change in one place breaks another. You want
Tessera to see across the boundary.

## One repository, several services

Run Tessera at the repository root. One index covers every package, and
`map` links the services to each other. With the `shop` project from
[block breaking API changes](block-breaking-api-changes.md):

```console
$ tessera map service web
service web
  serves route ANY /api/health  web/pages/api/health.ts:1
  calls  route GET /api/orders/{} on nest  web/src/client.ts:18
  calls  route GET /api/users/{} on api  web/src/client.ts:8
  calls  route GET /repos  web/src/client.ts:23
  calls  route POST /api/users on api  web/src/client.ts:14
```

`map service <name>` lists what a service serves and what it calls, with the
service on the other end. `tessera path` follows a call from a client into
the handler that serves it, through the contract. To run only one package's
tests in CI, use `tessera affected --package <name>`; npm, pnpm and yarn
workspaces, Cargo, Go and Maven modules count as packages.

## Several repositories

Each repository has its own index. A `tessera.workspace.toml` in a directory
above them lists the repositories to map together. From the directory where
you cloned tessera, make two repositories out of the sample's `api` and
`web`, with the file beside them:

```sh
mkdir ws && cd ws
for r in api web; do
  mkdir $r && cp -R ../tessera/tests/fixtures/contracts-http/$r/. $r/
  git -C $r init -q && git -C $r add -A && git -C $r commit -qm sample
done
printf '[[repo]]\npath = "api"\n\n[[repo]]\npath = "web"\n' > tessera.workspace.toml
```

From either repository, `map` now shows both:

```console
$ cd web && tessera map route /api/users
route GET /api/users/{}
  defined     api  api/openapi.yaml:7
  served by   api  api/src/app.js:6  getUser
  called by   web  web/src/client.ts:8  loadUser (low)
  called by   web  web/src/client.ts:9  loadUser (medium)
route POST /api/users
  defined     api  api/openapi.yaml:10
  served by   api  api/src/app.js:7  router.post('/api/users')
  called by   web  web/src/client.ts:14  saveUser
```

`path` crosses the repositories too. Name a symbol in either, or prefix it
with its repository (`web/loadUser`) to pick one:

```console
$ tessera path loadUser getUser
Call path loadUser → getUser (1 hops)
  loadUser @ web/src/client.ts:7
  ↳ getUser @ api/src/app.js:10 (via route GET /api/users/{})
```

Use this before you change the API: it shows the client in the other
repository that your change will break. Each repository's index is built the
first time a query needs it.

## What it does not cover

- `verify` checks the repository it runs in. If you remove `POST /api/users`
  in `api` while `web` is a workspace sibling that calls it, `verify` in `api`
  does not report it yet. Here is the same removal as in the previous guide,
  run in the `api` repository:

  ```console
  $ cd api && tessera verify --changed
  ✗ verify: 1 problem(s), 0 blocking
    [drift] openapi.yaml:10 `POST /api/users` was removed from `src/app.js` but is still documented in `openapi.yaml` (advisory)
  ```

  Map before you change, and run `verify` in the client repository's CI too.
- Config keys and tables are listed but not drawn as links between services.
- Each repository keeps its own index, so a workspace with many large
  repositories pays for each one the first time.

## Next

- [Block breaking API changes](block-breaking-api-changes.md)
- Reference: [contract map](../contracts.md#across-services-and-repositories), [configuration](../configuration.md)
