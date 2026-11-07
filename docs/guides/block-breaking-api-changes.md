# Block breaking API changes

One service serves a route, a gRPC method or a topic, and another service
calls it. You want a change that removes it to fail before it merges, not when
the client breaks in production.

The examples use `tests/fixtures/contracts-http` in this repository: an
Express `api` and a TypeScript `web` client. Copy it into a repository of its
own:

```sh
cp -R tessera/tests/fixtures/contracts-http shop && cd shop
git init -q && git add -A && git commit -qm sample
```

## See both sides of a contract

```console
$ tessera map route /api/users
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

Each line names the service (the nearest directory with a build manifest) and
the handler or caller around the site. `map` also reads gRPC methods, GraphQL
fields, message topics, tables and configuration keys:

```console
$ tessera map rpc GetCart
rpc CartService/GetCart
  defined     grpc      protos/demo.proto:7
  served by   cart      cart/CartService.cs:14  CartService.GetCart
  called by   cart      cart/CartServiceTests.cs:12  CartServiceTests.GetsCart
  called by   grpc      currency-cpp/server.cpp:20  CheckCart
  called by   frontend  frontend/rpc.go:10  frontendServer.getCart
```

(That one is from `tests/fixtures/contracts-grpc`, copied the same way.) Run `map` before you
change an endpoint to see who calls it.

## Catch the removal

Delete the `POST /api/users` handler from `api/src/app.js`, as a refactor
might, and leave the client alone:

```sh
perl -ni -e "print unless /router\.post\('\/api\/users'/" api/src/app.js
```

```console
$ tessera verify --changed
✗ verify: 2 problem(s), 1 blocking
  [V5] web/src/client.ts:14 `POST /api/users` is no longer served: the change removed it from api/src/app.js, and this call still uses it
  [drift] api/openapi.yaml:10 `POST /api/users` was removed from `api/src/app.js` but is still documented in `api/openapi.yaml` (advisory)
```

The command exits 1, so a pre-commit hook, the edit hooks and the
[GitHub Action](review-a-pull-request.md) all stop on it. V5 blocks only when
both sides are high confidence and the path matches exactly. Removing
`GET /api/users/{}`, whose callers are a medium and a low match, is reported
as advisory and does not block.

## Keep the spec honest

```console
$ tessera map route --drift
served, not documented
  POST /api/users/{}/avatar   api/src/users.ts:5  uploadAvatar
documented, not served
  DELETE /api/users           api/openapi.yaml:12
method differs
  /api/orders/{}              documented PUT (api/openapi.yaml:15), served PATCH (api/src/orders.ts:4)
```

This one is from `tests/fixtures/contracts-drift`. It compares OpenAPI 3 and
Swagger documents with the routes the code serves.

## What it does not cover

- V5 looks at one repository. A route removed here while a repository in your
  workspace calls it is not reported yet; see
  [monorepos and several repositories](monorepo-and-several-repos.md).
- It catches a removed route, method or topic producer, not a changed field or
  type inside one.
- Clients whose URL is built away from the call (from app config, `str.format`
  or a wrapper of your own), and frameworks without a pack (Phoenix, Kafka
  clients for Go sarama, C# Confluent.Kafka and Kotlin), are missed rather than
  guessed. Measured recall is in [the contract map](../contracts.md#accuracy).

## Next

- [Monorepos and several repositories](monorepo-and-several-repos.md)
- Reference: [contract map](../contracts.md), including every framework it reads
