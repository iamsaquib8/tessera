# Quickstart

Tessera gives your coding agent a deterministic map of your code: where
things are defined, who calls them, what a change affects, and whether the
code it just wrote calls things that exist.

## Install

Pick one. All of them install the same single binary.

```sh
npx tessera-codegraph status             # no install; Node 18+
npm i -g tessera-codegraph               # then use `tessera`
brew install iamsaquib8/tessera/tessera  # macOS / Linux
curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
cargo binstall tessera-codegraph         # prebuilt, via cargo-binstall
cargo install tessera-codegraph          # from source (Rust 1.90+)
docker run --rm -v "$PWD:/work" ghcr.io/iamsaquib8/tessera find <symbol>
```

The install script verifies the release checksum before installing and puts
the binary in `~/.local/bin` (no sudo).

## First commands

Run these from anywhere inside a repository. There is no setup step: the
first query builds the index under `.tessera/`, and every later query
refreshes only what changed.

```sh
tessera find UserRepo.findById --body     # definition, signature, body
tessera refs loadUser                     # every call site
tessera blast formatName                  # who is affected, ranked, plus tests
tessera verify --snippet - --language typescript < new_code.ts
tessera context findById                  # one bundle to read before editing
tessera status                            # index health and setup hints
```

Add `--json` to any command for machine-readable output.

## Connect your agent

Claude Code:

```sh
claude mcp add tessera -- tessera mcp
```

Every other agent, including Cursor, Codex, Gemini CLI, Antigravity, Copilot,
OpenCode, and Kiro, is covered in [integrations](integrations.md).

Next: [the first five minutes](first-five-minutes.md) walks through a real
edit.
