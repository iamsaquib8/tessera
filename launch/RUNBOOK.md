# Launch RUNBOOK

Goal: maximize the probability of 100+ GitHub stars in a few days. Stars come
from a launch *landing*, not from polish — so the order and timing below matter
more than any single post. **You** post everything under your own accounts;
these are drafts. No upvote-begging, no star-buying (both backfire).

---

## Phase 0 — Prep (do before any post)

**Repo must look finished when the first visitor lands.**

- [ ] Merge the `feat/v0.4-10x` branch to `main` (or open the PR and merge).
- [ ] Set GitHub repo **About**: description + homepage, and **topics**:
      `mcp`, `code-graph`, `tree-sitter`, `ai-agents`, `claude-code`, `cursor`, `llm`, `rust`, `developer-tools`, `codegraph`.
- [ ] Upload **Settings → Social preview** → `assets/social-preview.png` (1280×640).
- [ ] Add release secrets: `CRATES_IO_TOKEN` (env `crates-io`), `NPM_TOKEN`. Enable GHCR package write for the repo.
- [ ] Decide the npm name: `tessera-codegraph` (default) — confirm it's free, or switch to a scope in `npm/package.json` + `release.yml`.
- [ ] Create the Homebrew tap repo `iamsaquib8/homebrew-tessera`, add `packaging/homebrew/tessera.rb` at `Formula/tessera.rb`, set `HOMEBREW_TAP_TOKEN` if you automate bumps.
- [ ] **Tag the release:** `git tag v0.4.0 && git push origin v0.4.0`. Wait for the Release workflow to attach binaries, publish to crates.io, publish npm, and push the Docker image.
- [ ] **Smoke-test every install path** in a clean shell/VM: `npx tessera-codegraph --version`, `curl … | sh`, `brew install …`, `docker run … --version`, a downloaded binary. Each prints `tessera 0.4.0`.
- [ ] Fill the Homebrew formula `sha256` (the v0.4.0 source tarball) and push to the tap.
- [ ] Publish the dev.to article (`devto-post.md`, set `published: true`) — you'll link r/programming and X to it.

A broken install on launch day is the #1 way to convert a front-page moment into nothing. Do not skip the smoke test.

## Phase 1 — Launch day (the landing)

Pick **Tue–Thu**. This is the high-variance moment; be free to engage for ~3 hours.

1. **~8:00–9:00 AM ET — Show HN** (`show-hn.md`). Post link + first comment. Then step away from the post itself; just be available to answer.
2. **~+30 min — r/LocalLLaMA** (`reddit-localllama.md`) and **r/ClaudeAI** (`reddit-claudeai.md`). These are your warmest audiences.
3. **~+1 hr — X/Twitter thread** (`twitter-thread.md`) with the social-preview image. Pin it. Reply to your own thread with the repo link.
4. **Throughout — reply to every comment** quickly and substantively. Early engagement is what pushes HN → front page and Reddit → hot. This matters more than the post text.

> Don't fire all channels in the same 10 minutes — stagger ~30–60 min so you can actually respond where the conversation starts.

## Phase 2 — Day 1–2 (sustain)

- [ ] **r/rust** (`reddit-rust.md`) — flair "Project". Lead with internals, not the AI pitch.
- [ ] **r/programming** (`reddit-programming.md`) — link the dev.to article, NOT the bare repo.
- [ ] Start **directory/awesome-list PRs** (`directories.md`) — a few per day, each tailored. The MCP registries (modelcontextprotocol/servers, awesome-mcp-servers, Glama, Smithery) are the highest-relevance.
- [ ] Reply to any GitHub issues/discussions fast — responsiveness on day 1 converts lurkers to stargazers and contributors.

## Phase 3 — Day 3+ (compound + fast-follow)

- [ ] Triage feedback into issues; label `good first issue` generously (invites contributors → more stars).
- [ ] Ship **v0.5.0** within ~a week: Kotlin/Swift/Scala/Lua/Zig + `watch` + `unused` + HTTP MCP. A second release is a second, legitimate "Show HN: Tessera v0.5 — now 16 languages + graph viz" / r/LocalLLaMA wave. Repeat Phase 1 lightly.

## What to watch

- HN: are you climbing /newest → front page in the first hour? Comments are the signal.
- Reddit: upvote ratio + comment velocity in the first 2 hours.
- GitHub: **Insights → Traffic** (views/clones/referrers) tells you which channel actually converted, so you double down next time.

## Hard rules

- Never ask for upvotes/stars in the posts (flag/ban risk). "A star helps it reach people" in a thread is fine; brigading is not.
- No fake accounts, no purchased stars — GitHub delists for it and it's obvious.
- Tailor every post. Maintainers and mods reject copy-paste cross-posts on sight.
- Be honest about pre-alpha status — the "built in public, solo, here's the weak spot" framing earns more goodwill (and stars) than overclaiming.
