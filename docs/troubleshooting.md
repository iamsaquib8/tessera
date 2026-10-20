# Troubleshooting

Start with `tessera status`. It reports the root it picked, the index
location, freshness, file counts, and a hint for each problem it finds.

## "No supported source files found under …"

The root has no files in a supported language, or they are all excluded. Check
that the root is the repository: run from inside it, or pass `--root`. Also
check `.gitignore` and `.tessera/config.toml` excludes.

## An agent runs an old Tessera

`tessera status` warns when the `tessera` on your `PATH` is a different version
from the one you ran:

```text
hint: `tessera` on PATH resolves to ~/.cargo/bin/tessera (tessera 0.4.0), not this binary
```

Remove or upgrade that copy (`cargo uninstall tessera-codegraph`, or reinstall
it), or point the agent's MCP config at the full path of the binary you want.

## Results look stale

Every CLI query re-indexes changed files first. The MCP server watches the
tree. If you edited files through a tool that preserves mtimes, run
`tessera index`. To start over, delete `.tessera/` or run
`tessera index --full`.

## "Index uses schema vN, this binary needs vM"

This happens when a newer or older Tessera opens your index. CLI queries and
`tessera index` rebuild it automatically; the index is a cache, so nothing is
lost.

## MCP: tools return "Index is building"

This is the first index of a large repository. The server answered `initialize`
straight away and is indexing in the background. Retry after a few seconds;
`status` reports when it is done.

## MCP over HTTP returns 400 or 403

- **400 with `-32020`:** a modern client is missing `MCP-Protocol-Version`,
  `Mcp-Method`, or `Mcp-Name` headers, or they do not match the body.
- **400 with `-32022`:** the protocol version is not supported. The error
  lists the versions that are.
- **403:** the request carried a non-local `Origin`. Only localhost origins
  are accepted.

## Docker

Mount the repository at `/work`; the image uses it as the root:

```sh
docker run --rm -v "$PWD:/work" ghcr.io/iamsaquib8/tessera status
```

## Windows

Paths are stored and printed with forward slashes. The npm package and the
release zip both ship `tessera.exe`. If the npm launcher reports a missing
platform package, reinstall without `--omit=optional`.
