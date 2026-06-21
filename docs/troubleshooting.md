# Troubleshooting

Start with:

```sh
tessera doctor
```

## Missing Database

If a query says the database is missing, run the exact command shown in the
error, usually:

```sh
tessera index . --db .tessera/tessera.db
```

Read-only commands no longer create an empty database silently.

## Stale Snapshot

The MCP server uses `.tessera/snapshot.bin` for hot-path reads. Rebuild it with:

```sh
tessera snapshot --db .tessera/tessera.db
```

Running `tessera index .` also rebuilds the snapshot unless `--no-snapshot` is
used.

## Schema Problems

If `doctor` reports a schema mismatch, rebuild from source:

```sh
tessera index . --full
```

## Ignored Paths

Tessera skips common generated and cache directories including `.git`,
`node_modules`, `target`, `dist`, `build`, `.next`, virtualenvs, `__pycache__`,
and `.tessera`.

If a file is missing from results, confirm the extension is supported and that
the path is not under an ignored directory.

## MCP Startup

Confirm the DB exists before starting MCP:

```sh
tessera doctor --db .tessera/tessera.db
tessera mcp --db .tessera/tessera.db
```

For agent config examples, see `docs/integrations.md`.

## Windows and Docker Paths

Use an explicit `--db` path when the working directory differs between the host,
container, or editor:

```sh
tessera index /work --db /work/.tessera/tessera.db
tessera mcp --db /work/.tessera/tessera.db
```

