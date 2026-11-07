# Repository settings checklist

These are account-level settings only the maintainer can apply. They are
written down here so they can be applied once and checked later.

## About

- **Description:** Your agent says done. Tessera checks. Deterministic blast
  radius before every edit and verification after it, for any AI coding
  agent.
- **Website:** the docs site URL once GitHub Pages is enabled. Pages is enabled:
  the docs site is live at `https://iamsaquib8.github.io/tessera/` (set the
  repo's Website field here; `Cargo.toml` `homepage` already points at it).
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
- Enable **Dependabot alerts** and **Dependabot security updates** (Settings →
  Advanced Security). `.github/dependabot.yml` schedules the weekly version
  updates; the alerts and security updates are separate switches. Check with
  `gh api repos/iamsaquib8/tessera/vulnerability-alerts`, which returns 204
  once alerts are on.

## Labels

`.github/labels.yml` lists the labels the repository has, as `gh label list`
shows them, and the issue forms apply only labels from that list
(`tests/repo_hygiene.rs` checks). To add a label, create it first, then add it
to the file:

```sh
gh label create <name> --color <hex> --description <text>
```

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
