# When not to rely on Tessera (yet)

Tessera is deterministic: the same code always gives the same answer. That is
not the same as complete. These are the current limits, stated plainly.

## Confidence, not certainty

Every reference has a confidence (see [accuracy](accuracy.md)). High means
Tessera followed scope, an import, `this`, or a declared type to one
definition; on the benchmark that is right 99.7% of the time. Medium, low, and name-level results are labelled in the output
("likely", "possible") and are exactly that.

- **Deep tier:** TypeScript, TSX, and JavaScript (ESM and CommonJS) resolve
  through imports, re-exports, tsconfig paths, workspaces, and declared
  types. Java resolves through imports, packages, fields, and overloads by
  argument count.
- **Standard tier:** Python, Go, Rust, C, C++, C#, Ruby, and PHP resolve
  within a file and by unique name. Common names (`get`, `run`) come back as
  "possible".
- `verify` checks bare calls and calls on capitalised receivers that are in
  the index. It does not type-check, and calls into third-party packages are
  never flagged.

## Not followed today

- A receiver whose type comes from a chained call (`data.key("x").index(0)`),
  a getter, or an untyped object (`const deps = {…}` passed around).
- Plain identifier reads (`KEYS.A` used as a value): Tessera records calls,
  `new`, JSX, type references, `extends`/`implements`, and decorators, not
  every read.
- Dynamic dispatch, reflection, string-based calls, dependency injection by
  name, and code generated at build time.
- Contracts outside what `map` reads. HTTP routes, gRPC methods, GraphQL
  fields, topics, tables and config keys are linked across services and
  repositories ([contracts](contracts.md)), with these gaps the ground truth
  measured: clients whose URL is built away from the call (from app config,
  `str.format`, or a request wrapper), servers in Rust actix, PHP Slim,
  Sinatra, Phoenix and ASP.NET minimal APIs, Kafka clients for Go sarama,
  C# Confluent.Kafka and Kotlin, and Next.js Server Actions and tRPC.
- Field-level changes. V5 catches a removed route, RPC or topic that a
  client still uses, not a removed field or a changed type inside one.

## Use the compiler too

For TypeScript, Java, Go, Rust, and C#, the compiler or type-checker is the
ground truth for type errors. Tessera is a fast pre-check on the changed code
and the only check for JavaScript, Python, and Ruby. It never replaces
`tsc`, `javac`, or your tests.
