# Repository settings checklist

These are account-level settings only the maintainer can apply. They are
written down here so they can be applied once and checked later.

## About

- **Description:** Your agent says done. Tessera checks. Deterministic blast
  radius before every edit and verification after it, for any AI coding
  agent.
- **Website:** the docs site URL once GitHub Pages is enabled.
- **Topics:**
  - `mcp`, `mcp-server`, `ai-agents`, `coding-agents`
  - `claude-code`, `cursor`, `codex`
  - `code-graph`, `tree-sitter`, `static-analysis`, `refactoring`
  - `developer-tools`, `rust`
- **Social preview:** upload `assets/social-preview.png` (1280×640).

## Features

- Enable **Discussions** with the categories Q&A, Ideas, and Show and tell.
  The show-and-tell form is in `.github/DISCUSSION_TEMPLATE/`.
- Enable **Pages** from GitHub Actions; `.github/workflows/pages.yml` deploys
  the docs site.
- Enable **Private vulnerability reporting** (Security → Advisories).

## Labels

Sync `.github/labels.yml`, for example with
`gh label create --force <name> --color <hex> --description <text>` for each
entry.

## Release secrets and accounts

| Secret or account | Used by |
| --- | --- |
| `CRATES_IO_TOKEN` in the `crates-io` environment | crates.io publish |
| `NPM_TOKEN` (an npm automation token able to publish `tessera-codegraph*`) | npm publish |
| `HOMEBREW_TAP_TOKEN` (fine-grained, contents write on `iamsaquib8/homebrew-tessera`) | Homebrew formula push |
| Repository `iamsaquib8/homebrew-tessera` | Homebrew tap |
| GHCR package `tessera` set to public | `docker run ghcr.io/iamsaquib8/tessera` |

## Pinned issues

- Language requests
- Agent integrations
- Show and tell: what Tessera caught
