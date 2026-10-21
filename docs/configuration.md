# Configuration

Tessera works with no configuration. Everything here is optional.

## What gets indexed

- **Files:** every file with a supported extension under the repository
  root. The root is the nearest ancestor containing `.tessera/` or `.git`,
  or `--root`.
- **Ignore files:** `.gitignore`, `.ignore`, `.tesseraignore`,
  `.git/info/exclude`, and your global git excludes file are honoured, even
  outside a git checkout.
- **Hidden files and directories** (names starting with `.`) and symlinks
  are skipped.
- **Always skipped:** `node_modules`, `.git`, `.tessera`, `__pycache__`,
  virtualenvs, and tool caches. `dist`, `build`, `target`, `out`, `vendor`,
  `coverage`, and `.next` are skipped only at the repository root, so a Java
  package named `build` is still indexed.
- **Skipped as minified:** files named `*.min.*` or `*.bundle.*`, and files
  with a line over 5,000 bytes or an average line over 300 bytes.
- **Skipped as too large:** files over 1 MiB. The limit is configurable.

Skipped files show up as a count in `tessera status`.

## `.tessera/config.toml`

```toml
[index]
max_file_bytes = 2_097_152      # default 1 MiB

[include]
paths = ["src/", "packages/api/"]   # only these path prefixes; empty = everything

[exclude]
paths = ["src/generated/"]          # path prefixes to skip
globs = ["**/*.generated.ts"]       # glob patterns to skip

[ignore]
extra = ["fixtures"]                # directory or file names to skip anywhere
```

`tessera init` writes a commented starter file.

## One-off excludes

```sh
tessera index --exclude 'legacy/**' --exclude '**/*.pb.go'
```

Excludes passed this way are remembered, so automatic re-indexing, `watch`,
and git hooks keep applying them. Passing `--exclude` again replaces the
list; `tessera index --reset-excludes` clears it. For excludes the whole team
should share, use `config.toml`.

## Where the index lives

The default is `<root>/.tessera/tessera.db`. A `.gitignore` written next to
it ignores the index but not `config.toml`, so the project config can be
committed. Override the location with `--db <file>` or the `TESSERA_DB`
environment variable. Tessera refuses to use a file that is not a Tessera
index, so a mistyped `--db` cannot overwrite another database.

The index is a cache. It is rebuilt when a new Tessera version, a change to
`config.toml`, or a new set of excludes could change what gets indexed.
Writers take a lock on `<db>.lock`; a query that finds another process
indexing answers from the existing index instead of waiting.
