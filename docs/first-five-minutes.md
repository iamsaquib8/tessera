# First Five Minutes

This path gets from install to a useful graph result without reading source
files manually.

## 1. Install

```sh
npm install -g tessera-codegraph
# or: brew install iamsaquib8/tessera/tessera
# or: cargo install tessera-codegraph
```

## 2. Initialize and Index

```sh
cd path/to/project
tessera init --mcp-configs
tessera index .
tessera doctor
```

`doctor` checks the database, schema, snapshot, parser smoke tests, ignored-path
rules, and the MCP command to paste into an agent.

## 3. Ask the First Questions

```sh
tessera impact findById --explain
tessera validate findByIdd --explain
tessera connect handleRequest writeRow
tessera export --from findById --format mermaid
```

Use `--json` on any query when feeding another tool or script.

## 4. Keep It Fresh

```sh
tessera watch .
```

`watch` reuses the same incremental SHA-diff indexer as `index`, with polling and
debounce controls for daily editing.

## 5. Wire MCP

Use the generated snippets under `.tessera/mcp/`, or add this shape to your
agent config:

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

