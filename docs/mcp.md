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

Input:

```json
{ "symbol": "findById" }
```

Returns matching definitions with file, line range, kind, signature, and export status.

### `find_references`

Input:

```json
{ "symbol": "findById" }
```

Returns call/reference sites with one-line context.

### `get_outline`

Input:

```json
{ "path": "src" }
```

Returns a semantic skeleton for a file or directory without symbol bodies.

### `expand_symbol`

Input:

```json
{ "symbol": "findById" }
```

Returns a symbol body and immediate dependencies.

### `impact`

Input:

```json
{ "symbol": "findById", "depth": 4 }
```

Returns transitive callers ranked by simple criticality.

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
