# tessera-codegraph (npm wrapper)

Installs the prebuilt [`tessera`](https://github.com/iamsaquib8/tessera) binary — a
local, deterministic semantic code graph + MCP server for AI coding agents — with
no Rust toolchain required.

```sh
npm i -g tessera-codegraph
# or, one-off:
npx tessera-codegraph --help
```

On install, `install.js` downloads the binary for your platform
(`x86_64`/`aarch64` × Linux/macOS/Windows) from the matching GitHub release and
`bin/tessera.js` execs it.

If your platform has no prebuilt binary, install from source instead:

```sh
cargo install tessera-codegraph
```

## Maintainer notes

- Publishing is automated by `.github/workflows/release.yml` (the `npm` job),
  gated on the `NPM_TOKEN` secret. The job stamps the version from the git tag.
- The package name is `tessera-codegraph` (the bare `tessera` name may be taken
  on npm). If you want a scoped name, change `name` here and in the workflow.
- `install.js` reads the release at tag `v<version>`, so the npm version must
  match a published GitHub release tag.
