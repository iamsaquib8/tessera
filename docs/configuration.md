# Configuration

Tessera works with no configuration. Everything here is optional.

## What gets indexed

- **Files:** every file with a supported extension under the repository
  root. The root is the nearest ancestor containing `.tessera/` or `.git`,
  or `--root`.
- **Ignore files:** `.gitignore`, `.ignore`, and `.tesseraignore` are
  honoured, even outside a git checkout.
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

Excludes passed this way are remembered, so automatic re-indexing keeps
applying them. Running `tessera index` again without `--exclude` clears them.
For excludes that should always apply, use `config.toml`.

## Where the index lives

The default is `<root>/.tessera/tessera.db`, with a `.gitignore` written next
to it so it is never committed. Override it with `--db <file>`. The index is a
cache: when a new Tessera version changes the schema, the index rebuilds
itself.
