# Troubleshooting

Start with `tessera status`. It reports the root it picked, the index
location, freshness, file counts, and a hint for each problem it finds.

## "No supported source files found under …"

The root has no files in a supported language, or they are all excluded. Check
that the root is the repository: run from inside it, or pass `--root`. Also
check `.gitignore` and `.tessera/config.toml` excludes.

## "refusing … home directory or the filesystem root"

Tessera was started outside any project, often by an MCP client that launches
servers from `~` or `/`. Pass `--root <repo>` in the server command, or start
it from inside the repository.

## "… is not a Tessera index; refusing to overwrite it"

`--db` or `TESSERA_DB` points at another program's SQLite file. Tessera never
modifies it. Pick a different path.

## "another tessera process is indexing"

Another CLI run, watcher, or MCP server is updating the same index. The query
answered from the existing index, which may be a few files behind. Run it
again in a moment for fresh results.

## "N files could not be read" or "N files have syntax errors"

`tessera status` lists unreadable files (usually permissions) and files that
parsed with syntax errors. Symbols after a syntax error may be missing until
the file parses cleanly.

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

The image keeps its index in `/tmp` inside the container, so it never writes
to your checkout and works whatever user owns the files. To keep the index
between runs, mount a volume and set `-e TESSERA_DB=/index/tessera.db`.

## Windows

Paths are stored and printed with forward slashes. The npm package and the
release zip both ship `tessera.exe`. If the npm launcher reports a missing
platform package, reinstall without `--omit=optional`.
