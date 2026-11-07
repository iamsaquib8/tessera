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
- **403:** a non-local `Origin` was supplied, or a server bound to loopback
  received a missing or non-local `Host` header. Loopback-bound servers only
  accept `localhost`, `127.0.0.1`, and `[::1]` as the request host, with an
  optional port; this also applies to `/health`.

## Docker

A public image is not available yet, so build it from a checkout of the
repository:

```sh
docker build -t tessera .
```

Mount the repository you want to check at `/work`; the image uses it as the
root:

```sh
docker run --rm -v "$PWD:/work" tessera status
```

The image keeps its index in `/tmp` inside the container, so it never writes
to your checkout and works whatever user owns the files. To keep the index
between runs, mount a volume and set `-e TESSERA_DB=/index/tessera.db`.

## Windows

Paths are stored and printed with forward slashes. The npm package and the
release zip both ship `tessera.exe`. If the npm launcher reports a missing
platform package, reinstall without `--omit=optional`.

## Frequently asked questions

### Does Tessera send my code anywhere?

No. The binary makes no network calls and opens no outbound connection; it
reads the files under the repository root and writes only to `.tessera/`
(or `--db`). The installers use the network to download it. See
[SECURITY.md](../SECURITY.md).

### Which models and agents does it work with?

Any model, because Tessera never calls one. Any agent that speaks MCP gets the
tools, and `tessera init` sets up 21 agents, with hooks where the agent's hook
system can carry them. See [connect your agent](guides/connect-your-agent.md).

### Should I commit `.tessera/`?

Not the index. A `.gitignore` next to it ignores the index and lets
`config.toml` through, so the project config can be committed. Commit
`baseline.json` and `accepted.toml` too if you use them
([adopt on a legacy repository](guides/adopt-on-a-legacy-repo.md)).

### The hook held my agent for a problem it did not cause

Only problems the agent introduced block. A problem that was in the code
before the session started never does, even in a file the agent edits; the
exception is another call site of the same problem that the agent adds. The
hold also ends after three tries in one turn. If a finding is wrong or
intended, `tessera accept <fingerprint> --reason …` records it, and
`tessera stats` shows how often the hooks led to a fix.

### Why does the hook say nothing about my Python or Go change?

Only high-confidence findings in TypeScript, JavaScript and Java block, and
the hooks show nothing for other languages. Run `tessera verify --changed` to
see them.

### How do I turn the hooks off?

Remove Tessera's entries from the agent's hook file (listed in
[edit-safety hooks](hooks.md#agents)), or uninstall the plugin. `tessera init`
only adds entries, so your own hooks are untouched.

### Can I use the Claude Code plugin and `tessera init`?

Use one, not both. If both are wired, Tessera checks each event once and
gives both runs the same answer.

### Does `verify` replace my compiler or tests?

No. It checks names, argument counts and imports against the index; it does
not check types. Keep running `tsc`, `javac` and your tests
([when not to rely on Tessera](when-not-to-use.md)).

### A test I expected was not selected, or one I did not expect was

Ask `tessera affected` why: `tessera affected origin/main --explain <test>`
says whether the test is selected and the chain from it to the changed code.
Calls Tessera cannot follow, such as dynamic dispatch and reflection, can hide
a test from it; see [run only the affected tests](guides/run-only-affected-tests.md).

### A `Tessera-No-Skip:` trailer in my commit is ignored in CI

CI clones are often shallow (`actions/checkout` fetches one commit), and
Tessera reads trailers from the history the clone has. It says so in a note.
Set `fetch-depth: 0`.
