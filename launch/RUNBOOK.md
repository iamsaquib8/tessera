# Launch RUNBOOK: Tessera 1.0.0

Goal: put Tessera 1.0 in front of the people who run coding agents every
day, have every install path work for the first visitor, and answer every
question in the first hours. A launch that lands comes from timing and from
being there to reply, more than from polish. Saquib posts everything from
his own accounts; the files in `launch/` are drafts. No asking for upvotes
or stars, and no bought stars.

---

## Phase 0: Prerequisites (only Saquib can do these)

Do them in this order. The repository has to look finished, and every link
in the posts has to work, when the first visitor lands.

- [ ] **Re-login to the Claude CLI** so the refactor eval in tessera-bench
      can run its agent arms. If the results are in before launch, they can
      go into the posts; until then, the posts make no claim about them.
- [ ] **Merge PR iamsaquib8/tessera#16** (`feat/v1-overhaul` into `main`).
      The README, plugin marketplace, Action, screenshots and the image
      links in the dev.to article all read from `main`.
- [ ] **Repository About, topics and social preview**, from
      `docs/repo-settings.md`: the description, the topics, and
      `assets/social-preview.png` (1280×640) under Settings → Social
      preview.
- [ ] **Release secrets:** `NPM_TOKEN`, `CRATES_IO_TOKEN` (in the
      `crates-io` environment), and `HOMEBREW_TAP_TOKEN` (contents write on
      the tap repository).
- [ ] **Create the tap repository** `iamsaquib8/homebrew-tessera`.
- [ ] **Enable GitHub Pages** (source: GitHub Actions) and the **Actions
      write permission** that lets the Tessera workflow comment on pull
      requests.
- [ ] **Create and push `iamsaquib8/tessera-bench`** from the local copy at
      `~/dev/tessera-bench`, public. The README, docs/accuracy.md,
      docs/contracts.md and every post link to it; the numbers are only
      "reproducible" once it is public.
- [ ] Optional: **create `iamsaquib8/tessera-action`** with `action.yml` at
      its root. GitHub Marketplace only lists an action at a repository
      root; `uses: iamsaquib8/tessera/action@v1` works without it.
- [ ] **Remove the stale `~/.cargo/bin/tessera` (0.4.0)** so it does not
      shadow the new binary during smoke tests. `tessera status` warns when
      an older `tessera` comes first on `PATH`.
- [ ] **Tag the release:** `git tag v1.0.0 && git push origin v1.0.0`.
      Wait for the Release workflow to attach the binaries and checksums,
      publish to crates.io and npm, push the Homebrew formula, and push the
      Docker image.
- [ ] **Point a `v1` tag at the same commit**
      (`git tag v1 v1.0.0 && git push origin v1`), so that
      `uses: iamsaquib8/tessera/action@v1` resolves. Nothing in the release
      workflow creates it. Move it on each 1.x release.
- [ ] **Make the GHCR package `tessera` public** (it exists after the first
      image push).
- [ ] **Smoke-test every install path** in a clean shell or VM. Each must
      print `tessera 1.0.0`:
  - [ ] `npx tessera-codegraph --version`
  - [ ] `npm i -g tessera-codegraph && tessera --version`
  - [ ] `brew install iamsaquib8/tessera/tessera && tessera --version`
  - [ ] `curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh`
  - [ ] `cargo binstall tessera-codegraph` and `cargo install tessera-codegraph`
  - [ ] `docker run --rm ghcr.io/iamsaquib8/tessera --version`
  - [ ] Claude Code: `/plugin marketplace add iamsaquib8/tessera`,
        `/plugin install tessera@tessera`, then a deliberate broken rename
        that the Stop hook catches
  - [ ] `tessera init --dry-run` in a project with two or three agents
        configured
  - [ ] The Action on a throwaway pull request: one comment, updated on
        the next push
- [ ] **Native-speaker review** of README.zh-CN.md, README.ja.md,
      README.ko.md, README.es.md and README.pt-BR.md. They are drafts until
      someone who speaks the language reads them; keep the draft note until
      then.
- [ ] **Publish the dev.to article** (`devto-post.md`, set
      `published: true`) and check that every image renders. r/programming
      links to it.

A broken install on launch day turns a front-page moment into nothing. Do
not skip the smoke tests.

## Phase 1: Launch day

Pick Tuesday, Wednesday or Thursday, and keep about three hours free to
reply.

1. **8:00 to 9:00 AM US Eastern: Show HN** (`show-hn.md`). Submit, post the
   first comment, then stay available to answer.
2. **About 30 minutes later: r/LocalLLaMA** (`reddit-localllama.md`) **and
   r/ClaudeAI** (`reddit-claudeai.md`).
3. **About an hour later: the X thread** (`twitter-thread.md`), with the
   three screenshots. Pin post 1.
4. **All day: reply to every comment**, quickly and with specifics. When
   someone finds a limit, agree, link the line in docs/when-not-to-use.md
   or docs/contracts.md, and say whether it is on the roadmap.

Stagger the channels by 30 to 60 minutes so you can answer where the
conversation starts.

## Phase 2: Days 1 and 2

- [ ] **r/rust** (`reddit-rust.md`), flaired as a project. Internals first.
- [ ] **r/programming** (`reddit-programming.md`): a link to the dev.to
      article, not the bare repository.
- [ ] **Directories and awesome lists** (`directories.md`), a few a day,
      each with the description that fits. The MCP registries first.
- [ ] **Answer GitHub issues and discussions** the same day. Turn every
      reported false positive into an issue with the snippet; they are the
      most useful feedback the launch will bring.

## Phase 3: Day 3 onward (compound and fast-follow)

- [ ] Triage feedback into issues and label `good first issue` where a
      language, framework pack or agent adapter is small and
      self-contained.
- [ ] Decide whether to publish the accuracy write-up
      (`2026-10-accuracy.md`) as a separate post about a week after launch.
- [ ] Publish the refactor eval results once they exist, with the same
      care as the accuracy numbers: the method, the raw output, and the
      cases where Tessera did not help.
- [ ] **Plan v1.1** from ROADMAP "After 1.0", choosing by what the launch
      feedback asks for most:
  - Team & CI: `tessera affected` for test selection across languages, an
    index cache keyed by commit, SARIF in code scanning.
  - Architecture rules: layer, forbidden-call and cycle rules enforced by
    `verify`, the hooks and CI.
  - Precise mode: SCIP import for compiler-grade edges, SCIP export, an
    optional `tessera lsp`.
  - Editors: VS Code and JetBrains extensions over the local binary.
  - Runtime truth: OpenTelemetry traces overlaid on the static graph.
  - Data lineage: who reads and writes which tables and columns, with PII
    rules in `verify`.
  - Extension platform: WASM extractor and framework-pack plugins.
  - Agent memory: notes tied to symbols that expire when the code changes.
  - Every release also brings deep-tier languages (Python, Ruby, Go, C#,
    Kotlin), new packs, new agent adapters, and benchmark rows for each.
    The contract-map gaps still open are Express mount prefixes, GraphQL,
    config keys, `path` across repositories, and a third untouched corpus.
- [ ] A 1.1 release with a visible new capability is a fair reason for a
      smaller second round (r/ClaudeAI, r/LocalLLaMA, X). Do not repeat
      Show HN for a minor release.

## What to watch

- **HN:** whether the post climbs from /newest to the front page in the
  first hour. Comments are the signal.
- **Reddit:** upvote ratio and comment velocity in the first two hours.
- **GitHub:** Insights → Traffic (views, clones, referrers) shows which
  channel brought people, so you know where to put effort next time.
- **npm and crates.io downloads** in the first week, against the GitHub
  traffic.

## Hard rules

- Never ask for upvotes or stars in a post, a thread or a DM.
- No fake accounts and no purchased stars.
- Tailor every post to its audience. Moderators remove copy-paste
  cross-posts on sight.
- Every number in a post comes from README.md, docs/accuracy.md,
  docs/contracts.md or the pitch kit. If a reply needs a number that is not
  there, say you have not measured it.
- State the limits before someone else does. "Here is where it falls short"
  earns more trust than overclaiming.
