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
