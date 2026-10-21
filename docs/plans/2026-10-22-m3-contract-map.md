# M3 Contract Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Tessera knows how services talk to each other: HTTP routes and their clients, gRPC services and their stubs, Kafka topics and their producers and consumers, and tables and the code that maps them, so `map`, `path`, and `verify` see across service boundaries.

**Architecture:** Contract facts are extracted per file at index time, next to symbols, and stored in a `facts` table with the enclosing symbol. Links are not stored: a link is two facts with the same normalised key (`GET /users/{}`, `grpc cartservice/getcart`, `kafka orders`, `table users`), joined at query time, so incremental indexing needs no link pass. Facts in code come from declarative packs (tree-sitter `.scm` queries under `packs/`, one convention for captures); facts in contract files (`.proto`, SQL migrations, OpenAPI, Prisma) come from small text readers. A workspace (`tessera.workspace.toml`) unions the facts of several repositories' indexes at query time.

**Spec:** `docs/design/2026-10-21-overhaul.md` §5.5, §5.1 (`map`, `path`), §5.4 (V5), §8 (M3 row).

## Global Constraints

Everything in the M0–M2 plans still applies. In addition:

- **Confidence:** a literal path, topic, or table name is `high`; a template with parameters `medium`; a concatenation with a variable or an unresolved constant `low`. A link takes the lower confidence of its two facts.
- **Keys:** routes normalise to `METHOD /path` with every parameter as `{}` (`:id`, `{id}`, `${id}`, `[id]`); a client method `ANY` matches every method. gRPC keys are `grpc service/method`, lowercased, without the package. Kafka `kafka topic`. Tables `table name`, lowercased, without schema.
- **Packs are data:** adding a framework is a `.scm` file plus one registry line and a fixture; post-processing is shared by fact kind, not per pack.
- **Gate:** contract links on microservices-demo have published precision and recall in tessera-bench; indexing the largest corpus repo with facts stays within the M0 memory budget.

## Review Focus

1. **Lookalike calls.** `map.get("k", fallback)`, `cache.post(...)`, and `axios.get` must not become routes; route rules need a router-like receiver and a handler argument.
2. **Base URLs and hosts.** `http://cart:8080/api/cart` and `` `${BASE}/api/cart` `` must match the route `/api/cart`.
3. **gRPC casing across languages.** Proto `GetQuote`, Java `getQuote`, Node `getQuote`, Go `GetQuote` are one method.
4. **Generated code.** `*_pb2*.py`, `*.pb.go`, `*Grpc.java` define stubs; they must not count as servers or clients.
5. **Removed contracts.** V5 must fire when a change removes a route or rpc that a client still calls, and stay silent for calls to external APIs.

---

## Tasks

### Task 1: Facts layer and gRPC

**Files:** `src/types.rs` (Language variants `Proto`, `Sql`, `OpenApi`, `Prisma`), `src/walk.rs`, `src/db.rs` (schema v8, `facts`), `src/facts/{mod,grpc,text}.rs`, `src/extract/mod.rs` (`ParsedFile.facts`), `src/indexer.rs`, `tests/contracts.rs`, fixture `tests/fixtures/contracts-grpc/`

- `IndexedFact { kind, key, line, confidence, rule, evidence, from_symbol }`; `facts(id, file_id, kind, key, line, symbol_id, confidence, rule, evidence)` with indexes on `key` and `file_id`.
- `.proto`: `rpc_def` per rpc. Code: `rpc_server` (Go `RegisterXServer`, Python `XServicer` subclasses, Java `XImplBase`, C# `X.XBase`, Node `addService(…X.service, {…})`), `rpc_impl` (methods of those classes; Go methods taking `context.Context` and a `*pkg.Type`), `rpc_client` (Go `NewXClient`, Python `XStub`, Java `XGrpc.new*Stub`, C# `X.XClient`), `rpc_call` (method calls in client files). Generated files are skipped.
- Linking (query time): `rpc_impl`/`rpc_call` with an unknown service resolve by method name against `rpc_def`.

**Tests:** a five-language fixture modelled on microservices-demo: every client call links to its server method; generated stubs produce no facts.

### Task 2: HTTP pack

**Files:** `packs/{express,fastify,nestjs,nextjs,http-clients,spring-mvc,spring-clients}.scm`, `src/facts/{packs,http}.rs`, OpenAPI reader in `src/facts/text.rs`, fixture `tests/fixtures/contracts-http/`

- Routes: Express/Fastify (`app|router|server|fastify.get(path, handler)`), Fastify `route({method, url})`, NestJS `@Controller(prefix)` + `@Get(path)`, Next.js `pages/api/**` and `app/**/route.ts` exports, Spring `@RequestMapping` on the class plus `@GetMapping` etc., OpenAPI `paths`.
- Clients: `fetch`, `axios`/`ky` and axios instances, `useSWR` keys, Feign interfaces, `RestTemplate`/`WebClient` URIs. Hosts and base-URL variables are stripped.

**Tests:** each framework's fixture; Review Focus 1 and 2 cases.

### Task 3: Kafka and tables

**Files:** `packs/{spring-kafka,kafkajs,jpa,typeorm,sequelize}.scm`, SQL and Prisma readers in `src/facts/text.rs`, fixture `tests/fixtures/contracts-data/`

- `producer`/`consumer`: `@KafkaListener(topics = …)`, `KafkaTemplate.send(topic, …)`, kafkajs `subscribe({topic})`, `producer.send({topic})`; Java `static final String` topic constants in the same file resolve.
- `table` (migrations: `CREATE TABLE`, `ALTER TABLE`), `entity` (`@Entity` + `@Table(name)`, TypeORM `@Entity("x")`, Sequelize `define("x")`, Prisma `model X` with `@@map`).

### Task 4: `map`, services, and `path` across services

**Files:** `src/query/map.rs`, `src/main.rs` (`map` subcommand), `src/mcp/tools.rs` (`map` tool), `src/query/path.rs`

- `tessera map <route|rpc|topic|table|service> [query]`: each contract with its definitions, servers or owners, and clients, with `file:line`, enclosing symbol, confidence, and service. A service is the nearest directory with a build manifest (`package.json`, `go.mod`, `pom.xml`, `build.gradle*`, `*.csproj`, `pyproject.toml`, `requirements.txt`, `Dockerfile`).
- `map service <name>`: what it serves and what it calls, grouped by the other service.
- `path` follows contract links from a client's enclosing symbol to the server handler.

### Task 5: Workspaces

**Files:** `src/workspace.rs` (`tessera.workspace.toml`), `src/query/map.rs`

- `[[repo]] path = "../api"` entries (relative to the file); the nearest workspace file above the root is used; each repository keeps its own index, built on demand; facts are unioned at query time.

### Task 6: V5

**Files:** `src/query/verify.rs`, `src/facts/mod.rs`

- V5: a change removes a route, rpc, or topic producer (facts in the file at the base revision that are gone now) while a client elsewhere still uses the key. High when both facts are high. Runs in `verify --changed/--staged/--diff`, post-edit, and stop.

### Task 7: Contract bench, docs, release

**Repo:** `~/dev/tessera-bench/contracts/`: a hand-labelled list of microservices-demo client→server links, a scorer, published precision and recall. Then README, `docs/contracts.md`, CHANGELOG 0.13.0, version bump.
