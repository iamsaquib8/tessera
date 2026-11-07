# Security policy

## Supported versions

Security fixes go into the latest 1.x release; upgrade within 1.x to get
them (the [stability contract](docs/stability.md) keeps 1.x upgrades
compatible).

| Version | Supported |
| --- | --- |
| 1.2.x | yes |
| earlier | no |

Release archives carry GitHub build provenance; check one with
`gh attestation verify <archive> --repo iamsaquib8/tessera`.

## Reporting a vulnerability

Report it privately through
[GitHub security advisories](https://github.com/iamsaquib8/tessera/security/advisories/new).
Please don't open a public issue. You'll get an acknowledgement within a few
days and a fix or a mitigation plan as soon as one is ready.

## What Tessera does and does not do

- It reads source files under the repository root and writes only to its
  index directory (`.tessera/` by default, or `--db`).
- It makes no network calls. `blast --diff` runs a local `git diff`, and the
  HTML preview from `export --html-out` loads no script, style or font from
  anywhere.
- `tessera mcp --http` binds where you tell it (use `127.0.0.1`). It rejects
  non-local `Origin` headers and serves read-only tools only.
- Release archives are published with SHA256SUMS and build-provenance
  attestations, and `install.sh` verifies the checksum before installing.
