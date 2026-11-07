# Security policy

## Supported versions

Security fixes go into the latest 1.x release; upgrade within 1.x to get
them (the [stability contract](docs/stability.md) keeps 1.x upgrades
compatible).

| Version | Supported |
| --- | --- |
| 1.2.x | yes |
| earlier | no |

## Verifying a release

Download an archive and `SHA256SUMS` from the
[release page](https://github.com/iamsaquib8/tessera/releases), then check the
build provenance and the checksum:

```sh
gh attestation verify tessera-x86_64-unknown-linux-musl.tar.gz --repo iamsaquib8/tessera
sha256sum -c --ignore-missing SHA256SUMS      # macOS: shasum -a 256 -c --ignore-missing SHA256SUMS
```

The attestation says which workflow and tag built the archive. `install.sh`
checks the same checksums before it installs anything.

## Reporting a vulnerability

Report it privately through
[GitHub security advisories](https://github.com/iamsaquib8/tessera/security/advisories/new).
Please don't open a public issue. You'll get an acknowledgement within 3 days
and a fix or a mitigation plan within 30 days.

## What Tessera does and does not do

- It reads source files under the repository root and writes only to its
  index directory (`.tessera/` by default, or `--db`).
- It makes no network calls. `blast --diff` runs a local `git diff`, and the
  HTML preview from `export --html-out` loads no script, style or font from
  anywhere.
- `tessera mcp --http` binds where you tell it (use `127.0.0.1`). It rejects
  non-local `Origin` headers and serves read-only tools only.
- The only sockets it opens are those two listeners: `mcp --http` where you
  bind it, and `tessera ui` on `127.0.0.1`. It opens no outbound connection.
  The installers, not the binary, use the network: `install.sh` downloads
  from GitHub, `npx` and `npm i -g` from the npm registry (the Claude Code
  plugin falls back to `npx` when `tessera` is not on `PATH`), and
  `cargo install` from crates.io.
- Release archives are published with SHA256SUMS and build-provenance
  attestations, and `install.sh` verifies the checksum before installing.
