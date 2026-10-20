# When not to rely on Tessera (yet)

Tessera is deterministic: the same code always gives the same answer. That is
not the same as complete. These are the current limits, stated plainly.

## Name-level matching (until v0.11)

References and callers are matched **by name**. A call to `save()` links to
every function named `save`, whichever class it belongs to. In practice:

- `refs` is exact for distinct names. For common names (`get`, `update`,
  `run`) expect extra results.
- `blast` can over-report callers through common names. Named callers are
  not dropped. When the traversal hits its size cap, the output says the
  result is partial.
- `verify` checks only bare calls and calls on capitalised receivers that are
  in the index. It does not type-check, and calls into third-party packages
  are never flagged.

v0.11 adds import- and scope-aware resolution for TypeScript, JavaScript, and
Java, with a confidence level on every edge.

## Not recorded today

- Class and type usage: `extends`, `implements`, type annotations, and
  decorators or annotations. Because of this, `search --unused` checks only
  functions and methods.
- Calls inside anonymous callbacks, such as Jest `it(() => …)` bodies or
  Express `app.get(…, (req, res) => …)` handlers. They have no enclosing
  symbol, so they appear in `refs` but not as caller edges in `blast`.
- Dynamic dispatch, reflection, string-based calls, dependency injection by
  name, and code generated at build time.
- Edges between repositories or services (routes, queues, tables). That is
  the v0.13 contract map.

## Use the compiler too

For TypeScript, Java, Go, Rust, and C#, the compiler or type-checker is the
ground truth for type errors. Tessera is a fast pre-check on the changed code
and the only check for JavaScript, Python, and Ruby. It never replaces
`tsc`, `javac`, or your tests.
