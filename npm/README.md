# tessera-codegraph

The npm distribution of [Tessera](https://github.com/iamsaquib8/tessera):
deterministic edit safety for AI coding agents. It installs the prebuilt
`tessera` binary for your platform. No Rust toolchain or install script is
needed.

```sh
npx tessera-codegraph find <symbol>     # one-off
npm i -g tessera-codegraph               # then: tessera status
```

## How it works

The `tessera-codegraph` package is a small launcher. The binary ships in one
optional dependency per platform, and npm installs only the one that matches
your machine:

| Package | Platform |
| --- | --- |
| `tessera-codegraph-darwin-arm64` | macOS, Apple silicon |
| `tessera-codegraph-darwin-x64` | macOS, Intel |
| `tessera-codegraph-linux-x64-gnu` / `-musl` | Linux x64, glibc or musl |
| `tessera-codegraph-linux-arm64-gnu` / `-musl` | Linux arm64, glibc or musl |
| `tessera-codegraph-win32-x64` | Windows x64 |

Nothing is downloaded at install time, so it works behind proxies and with
`--ignore-scripts`. If you install with `--omit=optional`, the launcher tells
you which package is missing.

Other platforms can build from source with `cargo install tessera-codegraph`.

## Maintainers

`.github/workflows/release.yml` publishes these packages after a GitHub release
is published, using the `NPM_TOKEN` secret.
`npm/scripts/build-platform-packages.js <version> <archives-dir>` unpacks the
release archives and stamps the version. Run the tests with
`node --test npm/test/`.
