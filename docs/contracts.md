# Contract map

Services talk through contracts: HTTP routes, gRPC methods, GraphQL
fields, message topics, database tables, and configuration keys. `tessera map` finds both sides of each one, in any
language Tessera reads, so you can see which other services a change
reaches before you make it.

```sh
tessera map route /api/users      # routes, with the requests that reach them
tessera map rpc GetCart           # gRPC methods: .proto, servers, clients
tessera map topic orders          # Kafka (and Rabbit/JMS/SQS) producers and consumers
tessera map table owners          # migrations, mapped models, SQL in code
tessera map graphql user          # schema fields, resolvers, operations
tessera map config DATABASE_URL   # who sets a key, who reads it
tessera map service checkout      # what a service serves and calls
```

```text
rpc ShippingService/GetQuote
  defined     microservices-demo   protos/demo.proto:108
  defined     src/adservice        src/adservice/src/main/proto/demo.proto:106
  defined     src/currencyservice  src/currencyservice/proto/demo.proto:106
  defined     src/paymentservice   src/paymentservice/proto/demo.proto:106
  served by   src/shippingservice  src/shippingservice/main.go:119  server.GetQuote
  called by   src/checkoutservice  src/checkoutservice/main.go:315  checkoutService.quoteShipping
  called by   src/frontend         src/frontend/rpc.go:88  frontendServer.getShippingQuote
```

Every site names the handler or caller around it, the service it belongs
to (the nearest directory with a build manifest), and a confidence. The
same data is the MCP `map` tool, and `--json` gives it to scripts.

## What it reads

| Contract | Providers | Consumers |
| --- | --- | --- |
| HTTP | Express, Fastify, Koa-router style routers, with `app.use("/prefix", router)` mounts followed through the file the router comes from; NestJS controllers; Next.js `pages/api` and `app/**/route.ts`; Spring MVC and WebFlux mappings; Go `net/http` `HandleFunc`/`Handle` (with Go 1.22 `"GET /path"` patterns), chi (with `r.Route` prefixes), gin and echo (with `Group` prefixes); FastAPI and Flask decorators (with `APIRouter(prefix=…)` and `Blueprint(url_prefix=…)`), Django `path()` in `urlpatterns`; Rails `routes.rb` verbs, `root`, and `resources` (the REST routes, with `only:`/`except:`), inside one `namespace` or `scope`; OpenAPI 3 and Swagger documents (as declarations) | `fetch`, axios and axios instances, ky, SWR; Spring Feign interfaces, `RestTemplate`, `WebClient`/`RestClient`; MockMvc and WebTestClient requests in tests; Go `http.Get`/`Post`/`Head`, `client.Get`, `http.NewRequest(WithContext)` (including `fmt.Sprintf` URLs); Python `requests`, `httpx` (functions, `Client`, `AsyncClient`) and sessions; Ruby `Net::HTTP` with `URI(…)`, Faraday (and its connections), HTTParty |
| gRPC | `.proto` services (declarations); Go `RegisterXServer` and handler methods; Python `XServicer` subclasses; Java `XImplBase`; C# `X.XBase` overrides; C++ `X::Service` overrides; Node `addService` | Go `NewXClient`; Python `XStub`; Java `XGrpc.new*Stub` and stub-typed fields; C# `X.XClient`; C++ `X::NewStub`; Node proto-loader and ts-proto clients |
| Topics | `KafkaTemplate.send`, kafkajs `producer.send`, NestJS `client.emit` | `@KafkaListener` (and Rabbit, JMS, SQS listeners), kafkajs `subscribe`, NestJS `@EventPattern`/`@MessagePattern` |
| Tables | `CREATE`/`ALTER`/`DROP TABLE` in `.sql` migrations (declarations); JPA `@Entity`/`@Table`, TypeORM `@Entity`, Sequelize models, Prisma models | SQL statements in string literals, including JPQL entity names |
| GraphQL | Root fields of `type Query`/`Mutation`/`Subscription` in `.graphql`/`.gql` files and `gql` templates (declarations); Apollo resolver maps; NestJS `@Query`/`@Mutation`/`@Subscription` methods (with `name:`) | Operations in `.graphql` files and `gql`/`graphql` tagged templates, by the root fields they select |
| Config keys | `.env` and `.env.*` files, Dockerfile `ENV`, Compose `environment:` blocks, Spring `application*.yml`/`.properties` | `process.env`, `import.meta.env`, `Deno.env.get`, `os.environ`/`os.getenv`, `os.Getenv`/`LookupEnv`, viper, `System.getenv`, `@Value("${…}")`, `env.getProperty`, `ENV[…]`/`ENV.fetch`, `env::var`/`env!`, `GetEnvironmentVariable`, and `${…}` placeholders in config files |

Configuration keys bind the way Spring's relaxed binding does:
`SPRING_DATASOURCE_URL`, `spring.datasource.url` and `spring.data-source.url`
are one key. Only keys are kept, never values; a setter's evidence is its key
alone. Config keys and tables are listed but not drawn as links between
services, and `path` does not follow them.

Generated code (protobuf stubs and anything marked generated) is skipped.

## Matching and confidence

- Routes normalise to `METHOD /path`, every parameter becoming `{}`
  (`:id`, `{id}`, `[id]`, `<int:id>`, `${id}`, `#{id}`, f-string `{id}`,
  `%d`, `*`); hosts, query strings and base-URL variables are dropped. A request reaches a route with the same number of
  segments, each equal or a parameter, and a compatible method; an exact
  path wins over a parameter match.
- A literal path, topic or table is **high** confidence, a template with
  parameters **medium**, a concatenation with a variable **low**.
- A gRPC call counts only on a variable holding a client (or a client
  constructor's result) and only for a method its service declares, so a
  method named like an rpc on some other object is not a call.
- Topics written as constants in the same file resolve; a constant from
  elsewhere still links both sides that name it, at low confidence.

## Across services and repositories

- `tessera path <from> <to>` follows contracts as call edges, so a path can
  run from a client through a gRPC method into the handler of another
  service, even when the two share no name. A hop reached through a
  contract names it (`via` in JSON).
- A `tessera.workspace.toml` at or above a repository lists repositories
  mapped together; `map`, run from any of them, combines their contracts.
  Each repository keeps its own index, built on demand.
- In a workspace, `path` crosses repositories too: it follows each
  repository's call graph, and a client in one repository whose contract
  another repository serves leads into that repository's handler. `from`
  and `to` may name a symbol in any of them; prefix one with its
  repository (`web/loadUser`) to pick it. Paths in the result carry the
  repository, and each symbol is visited once, so calls that cycle
  between repositories still end.

  ```text
  Call path loadUser → getUser (1 hops)
    loadUser @ web/src/client.ts:7
    ↳ getUser @ api/src/app.js:10 (via route GET /api/users/{})
  ```

  ```toml
  [[repo]]
  path = "../api"

  [[repo]]
  path = "../web"
  ```

## Contract checks

`verify` (and the edit hooks) reads each changed file as it was before the
change. When the change removes a route, a gRPC method, or a topic's
producer that no other file offers while a client still uses it, the client
gets a **V5** finding. It blocks only when both sides are high confidence
and the path matches exactly. It is reported, not blocked, for a listener
without a producer (the producer may live outside the repository), for a
request that reaches the route only through a path parameter
(`/users/me` and `/users/{}`), and for a route that may now be served
under a mounted router (`app.use("/api", users)`). Requests to another
organisation's API (an absolute URL with a dotted host, such as
`api.github.com`) are never clients of a local route. A route still served
for any method (`app.all`) is not removed. V5 looks at this repository
only: a route removed here while a workspace sibling calls it is not
reported yet.

## Accuracy

Hand-labelled gRPC links in
[tessera-bench](https://github.com/iamsaquib8/tessera-bench) (results and
how they were reached are in its `results/README.md`):

| Repository | Servers | Clients |
| --- | --- | --- |
| microservices-demo (Go, C#, Node, Python, Java) | 15 of 15, no wrong links | 34 of 34, no wrong links |
| opentelemetry-demo, held out (first measurement) | 10 of 12, no wrong links | 16 of 26, no wrong links |

The held-out misses (C++ servers, `using static` C# clients, injected Java
stubs) are handled since; that repository now scores 12 of 12 and 26 of 26,
but it is no longer held out.

Hand-labelled HTTP routes and Kafka topics, labelled from the source before
Tessera ran on them:

| Repository | Route servers | Route clients | Topics |
| --- | --- | --- | --- |
| bank-of-anthos, held out (Flask, Spring) | 39 of 39, no wrong links | 0 of 17, no wrong links | none in the repository |
| opentelemetry-demo | 10 of 21, no wrong links | 7 of 38, no wrong links | 0 of 5 sites |
| spring-petclinic-microservices | 16 of 16, no wrong links | 6 of 19 (6 of 6 that call a service directly), no wrong links | none in the repository |

Precision holds; recall on clients is low where URLs are assembled away
from the call: from Flask `app.config`, `str.format`, Locust's
`self.client`, or a wrapper of the project's own. opentelemetry-demo's
misses are mostly servers in frameworks not read yet (Rust actix, PHP Slim,
Sinatra, Phoenix, ASP.NET `MapGet`), and its topic uses Go sarama, C#
Confluent.Kafka and the Kotlin Kafka client, which no pack reads yet.

## Limits

- Express mounts are followed one level: `app.use("/api", users)` where
  `users` is a router in the same file or the file it is required or
  imported from. A router that is itself mounted on another router, or
  passed through a function, keeps its own paths only. V5 compares the
  paths routes are declared with, so a removed route on a mounted router is
  checked without its prefix.
- GraphQL links root fields only: nested resolvers (`User.name`) and
  fragments are not contracts. Schemas built in code (`buildSchema`,
  code-first type classes) are read only when they are `gql` documents.
- Config keys set by Kubernetes manifests, Helm values, CI variables or a
  secrets manager are not read; a key read through a wrapper of your own
  (`config.get("X")`) is not seen.
- Go, Python and Ruby route prefixes are read only within one file and one
  level: a gin or echo `Group` assigned and used in the same block, a chi
  `r.Route` body, a module-level `APIRouter`/`Blueprint`, one Rails
  `namespace` or `scope`. Nested groups, `include_router(…, prefix=…)`,
  `register_blueprint(…, url_prefix=…)`, Django `include()` and chi
  `Mount` do not add their prefix yet, nor do nested Rails `resources`.
- Flask `methods=[…]` and Rails `match … via:` give only the first method
  (Rails `match` is not read); Django `re_path` regexes are not read.
- Rails routes name no handler symbol: the controller action lives in
  another file.
- Python and Ruby routes need a router-like receiver (`app`, `router`,
  `bp`, `*_router`, …) and Go chi routes a router-like name (`r`,
  `router`, `mux`, …); routers named otherwise are missed rather than
  guessed.
- Queues other than the ones above come in later packs.

## Adding a framework

A pack is a tree-sitter query in [`packs/`](../packs) plus one entry in
`src/facts/packs.rs`: a regex gate, the languages, and the query. Captures
follow one convention (`@route`/`@client`/`@producer`/`@consumer`/`@entity`
mark the fact; `@method`, `@path`, `@prefix`, `@topic`, `@table` carry its
parts), so no Rust is needed for a new framework. Add a fixture under
`tests/fixtures/contracts-*` and an expectation in `tests/contracts.rs`.
