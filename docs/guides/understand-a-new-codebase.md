# Understand a new codebase

You joined a project, or your agent did, and you need to know where things
are and how they connect before you change anything. Reading file by file does
not scale; grep finds names, not meaning.

The examples use `tests/fixtures/ts-resolution`, a small TypeScript project
with path aliases and dependency injection. Copy it into a repository of its
own, as in the [quickstart](../quickstart.md).

## Find where things are

Outline a file: its classes, methods, functions and imports, with lines.

```console
$ tessera find --path src/services/users.ts
Outline of src/services/users.ts
  class UserService src/services/users.ts:6
  method UserService.constructor src/services/users.ts:7
  method UserService.name src/services/users.ts:9
  function makeService src/services/users.ts:14
  function isUser src/services/users.ts:21
  variable shared src/services/users.ts:25
  function useShared src/services/users.ts:27
  class Shop src/services/users.ts:31
  method Shop.constructor src/services/users.ts:32
  method Shop.label src/services/users.ts:34
  method Shop.direct src/services/users.ts:39
  import @app/data/repo (src/services/users.ts:1)
  import @app/domain (src/services/users.ts:2)
  import ./di (src/services/users.ts:3)
```

Search by name, fuzzy or with a glob, when you know part of it:

```console
$ tessera search 'user*'
class User src/domain/user.ts:5
method User.constructor src/domain/user.ts:6
method User.display src/domain/user.ts:8
class UserRepo src/data/repo.ts:7
method UserRepo.get src/data/repo.ts:10
method UserRepo.require src/data/repo.ts:14
class UserService src/services/users.ts:6
method UserService.constructor src/services/users.ts:7
method UserService.name src/services/users.ts:9
```

## See how things connect

Who calls a method:

```console
$ tessera refs UserRepo.require
1 references to UserRepo.require (1 definitions; all exact):
  src/services/users.ts:10:22 return this.repo.require(id).display();  ← UserService.name
```

How one thing reaches another, hop by hop:

```console
$ tessera path Shop.label User.display
Call path Shop.label → User.display (2 hops)
  Shop.label @ src/services/users.ts:34
  ↳ UserService.name @ src/services/users.ts:9
  ↳ User.display @ src/domain/user.ts:8
```

`tessera context <symbol>` bundles a symbol's body, callers, callees and
tests in one read ([check before you edit](check-before-you-edit.md)).

## Spot what is unused or tangled

```console
$ tessera search --unused 'use*'
function useShared src/services/users.ts:27
note: Types are checked through type references in TypeScript, JavaScript and Java; elsewhere only functions and methods are. Interface and abstract members are skipped.
```

`--unused` lists the matches that nothing calls or references. `'*'` lists
every one in the project.

`tessera cycles` lists modules that import each other in a loop. On this
project it prints `No circular import cycles found.`

## Draw it

`tessera export` prints the call graph as a Mermaid diagram (`--format dot`
for Graphviz). Give it a symbol to draw only the calls below it:

```console
$ tessera export --from Shop.label --depth 3
graph TD
  n0["Repo.get"]
  n1["Shop.label"]
  n2["User"]
  n3["User.display"]
  n4["UserRepo.require"]
  n5["UserService.name"]
  n6["notFound"]
  n1 --> n5
  n4 --> n0
  n4 --> n2
  n4 --> n6
  n5 --> n3
  n5 --> n4
```

`tessera ui --open` starts a local web page with the same map, your working
tree's change impact and the contract map. Nothing leaves your machine.

## What it does not cover

- "Unused" means no call or reference the index sees. A function called
  through reflection, a framework convention or a string name shows up in the
  list; check before you delete.
- Python, Go, Rust, C, C++, C#, Ruby and PHP resolve within a file and by
  unique name, so common names such as `get` are `possible`, not exact. See
  [accuracy](../accuracy.md).
- Tessera maps code. It does not read documentation or comments, and it does
  not explain why code is written the way it is.

## Next

- [Check before you edit](check-before-you-edit.md)
- Reference: [architecture](../architecture.md), [configuration](../configuration.md) for what gets indexed
