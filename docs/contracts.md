# Contract map

Services talk through contracts: HTTP routes, gRPC methods, message topics,
and database tables. `tessera map` finds both sides of each one, in any
language Tessera reads, so you can see which other services a change
reaches before you make it.

```sh
tessera map route /api/users      # routes, with the requests that reach them
tessera map rpc GetCart           # gRPC methods: .proto, servers, clients
tessera map topic orders          # Kafka (and Rabbit/JMS/SQS) producers and consumers
tessera map table owners          # migrations, mapped models, SQL in code
tessera map service checkout      # what a service serves and calls
```

```text
rpc ShippingService/GetQuote
  defined     microservices-demo   protos/demo.proto:108
  defined     src/adservice        src/adservice/src/main/proto/demo.proto:106
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
| HTTP | Express, Fastify, Koa-router style routers; NestJS controllers; Next.js `pages/api` and `app/**/route.ts`; Spring MVC and WebFlux mappings; OpenAPI 3 and Swagger documents (as declarations) | `fetch`, axios and axios instances, ky, SWR; Spring Feign interfaces, `RestTemplate`, `WebClient`/`RestClient`; MockMvc and WebTestClient requests in tests |
| gRPC | `.proto` services (declarations); Go `RegisterXServer` and handler methods; Python `XServicer` subclasses; Java `XImplBase`; C# `X.XBase` overrides; C++ `X::Service` overrides; Node `addService` | Go `NewXClient`; Python `XStub`; Java `XGrpc.new*Stub` and stub-typed fields; C# `X.XClient`; C++ `X::NewStub`; Node proto-loader and ts-proto clients |
| Topics | `KafkaTemplate.send`, kafkajs `producer.send`, NestJS `client.emit` | `@KafkaListener` (and Rabbit, JMS, SQS listeners), kafkajs `subscribe`, NestJS `@EventPattern`/`@MessagePattern` |
| Tables | `CREATE`/`ALTER`/`DROP TABLE` in `.sql` migrations (declarations); JPA `@Entity`/`@Table`, TypeORM `@Entity`, Sequelize models, Prisma models | SQL statements in string literals, including JPQL entity names |

Generated code (protobuf stubs and anything marked generated) is skipped.

## Matching and confidence

- Routes normalise to `METHOD /path`, every parameter becoming `{}`
  (`:id`, `{id}`, `[id]`, `${id}`, `*`); hosts, query strings and base-URL
  variables are dropped. A request reaches a route with the same number of
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
  service, even when the two share no name.
- A `tessera.workspace.toml` at or above a repository lists repositories
  mapped together; `map`, run from any of them, combines their contracts.
  Each repository keeps its own index, built on demand.

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
gets a **V5** finding. It blocks when both sides are high confidence; a
listener without a producer is reported, not blocked, since the producer
may live outside the repository.

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

## Limits

- Express routers mounted under a prefix (`app.use("/api", router)`) are not
  followed yet, so their routes miss clients that include the prefix.
- Go, Python and Ruby HTTP frameworks, GraphQL, and queues other than the
  ones above come in later packs.
- `path` follows contracts within one repository; `map` covers workspaces.

## Adding a framework

A pack is a tree-sitter query in [`packs/`](../packs) plus one entry in
`src/facts/packs.rs`: a regex gate, the languages, and the query. Captures
follow one convention (`@route`/`@client`/`@producer`/`@consumer`/`@entity`
mark the fact; `@method`, `@path`, `@prefix`, `@topic`, `@table` carry its
parts), so no Rust is needed for a new framework. Add a fixture under
`tests/fixtures/contracts-*` and an expectation in `tests/contracts.rs`.
