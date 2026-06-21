# MCP Server

Tessera exposes its code graph through a stdio MCP server.

```sh
tessera mcp --db .tessera/tessera.db
```

Index first:

```sh
tessera index .
```

## Tools

### `find_definition`

```json
{ "symbol": "findById" }
```

Returns matching definitions with file, line range, kind, signature, and export status. Falls back to fuzzy matches when there is no exact hit.

### `find_references`

```json
{ "symbol": "findById" }
```

Returns call/reference sites with one-line context.

### `get_outline`

```json
{ "path": "src" }
```

Returns a semantic skeleton for a file or directory without symbol bodies.

### `expand_symbol`

```json
{ "symbol": "findById" }
```

Returns a symbol body and immediate dependencies.

### `impact`

```json
{ "symbol": "findById", "depth": 4 }
```

Returns transitive callers ranked by personalised PageRank. Each caller includes a `breakdown` (`pagerank`, `fanout_in`, `fanout_out`, `exported`, `test_coverage`, `depth_decay`) so the score is auditable.

### `validate`

```json
{ "symbol": "findByIdd" }
```

Returns `{ exists, bloom_hit, candidates }`. Used to catch hallucinated identifiers — the Bloom-filter check short-circuits negatives, and unresolved queries get up to five near-miss suggestions ranked by Jaro-Winkler confidence.

### `validate_snippet`

```json
{
  "code": "findById(id); findByIdd(id);",
  "language": "typescript"
}
```

Parses the snippet with the same Tree-sitter pipeline used by `index` and validates every call against the graph. Returns per-call resolution plus near-miss suggestions for unresolved calls.

### `search`

```json
{
  "pattern": "*Repository*",
  "kinds": ["class", "interface"],
  "languages": ["java"],
  "exported": true,
  "path_prefix": "consumer/",
  "limit": 50
}
```

Fuzzy / `*`-glob search across indexed symbols, filterable by kind, language, exported, and path prefix. Use this instead of running `grep -r` + reading files when looking up a symbol whose exact name you don't remember.

### `context_pack`

```json
{ "symbol": "findById", "budget": 1500 }
```

Returns `{ symbol, body, dependency_signatures, caller_signatures, tests }` token-budgeted. Replaces 3-5 round trips an agent makes to "understand" a symbol before editing it. Body is clipped to ~40 % of the budget; dependency and caller signatures (no bodies) fill the rest; tests trim to fit.

### `plan_query`

```json
{ "task": "edit findById safely", "symbol": "findById" }
```

Returns an ordered tool plan with commands, reasons, inferred intent, and token
estimates. Use it as the first MCP call when an agent knows the task shape but
not the cheapest Tessera workflow.

### `edit_prep`

```json
{ "symbol": "findById", "budget": 1800 }
```

Returns a pre-edit bundle: `validate`, `signature`, `siblings`,
`context_pack`, `tests_for`, and next recommended checks. This is the v0.7
agent workflow shortcut for preparing a symbol edit without five separate tool
calls.

### `diff_impact`

```json
{ "from": "origin/main", "to": "HEAD", "depth": 3 }
```

Shells out to `git diff -U0`, maps changed hunks to indexed symbols, and runs PageRank impact on each. Single tool call answers "what does this PR break?" — returns the changed symbols plus the top impacted callers, deduplicated and ranked by criticality.

### `imports`

```json
{ "path": "src/users.ts" }
```

List imports / uses / requires declared in a file. Path can be a directory prefix.

### `imported_by`

```json
{ "source": "./users" }
```

Inverse of `imports`: list files that import a given module / source path. Substring-match, so `./users` finds importers of `./users.ts`, `./users/index.ts`, etc.

### `signature`

```json
{ "symbol": "UserService" }
```

Returns just the signature plus, for container kinds (class / struct / interface / trait / enum / record / impl / module), the signatures of nested members. No bodies — vastly cheaper than `expand_symbol` when the agent only needs the shape.

### `siblings`

```json
{ "symbol": "findById" }
```

Symbols that share callers with the target, ranked by overlap count. Useful for finding the cluster of related abstractions to refactor together.

### `stats`

```json
{}
```

Summary statistics: counts, languages, kinds, top fan-out symbols, snapshot presence.

### `tests_for`

```json
{ "symbol": "findById" }
```

Returns tests whose call graph transitively touches the symbol — heuristically by inspecting caller file paths for test/spec markers.

### `unused`

```json
{
  "kinds": ["function"],
  "languages": ["typescript"],
  "exported": false,
  "path_prefix": "src/",
  "limit": 50
}
```

Returns indexed symbols with zero inbound references and zero inbound call edges. Test files are excluded from the report.

## Example MCP Configuration

```json
{
  "mcpServers": {
    "tessera": {
      "command": "tessera",
      "args": ["mcp", "--db", ".tessera/tessera.db"]
    }
  }
}
```

The project must be indexed before the MCP server can return useful results.
