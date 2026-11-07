# Plan: the public launch of Tessera 1.x

Written 2026-11-07, after 1.2.0. Builds on the reordered roadmap
([#268](https://github.com/iamsaquib8/tessera/pull/268)).

## What "public launch" means

1.2.0 is released but nobody has been told: npm was unpublished until this
week, the Homebrew tap and public image do not exist, and the edit-safety
claim has never been measured. The launch is the release we announce (Show
HN, Reddit, directories, agent communities) and stand behind. Target:
**Tessera 1.4.0, launch week of 2027-02-22.**

It is ready when:

1. **It installs first time, everywhere.** Install script, npm/npx,
   crates.io, Homebrew, Docker, the GitHub Action and the Claude Code plugin
   work on Linux, macOS and Windows, proven by the release's own smoke tests.
2. **It is never silently wrong.** No answer of "fine" without a check; no
   known blocking false positive in TypeScript, JavaScript or Java.
3. **A new user gets value in five minutes.** `tessera init` sets up the
   project's agents and says what to do next; the README and one getting
   started page get them there; a misspelt symbol never reads as "safe".
4. **The claim is measured.** A published, reproducible eval says how many
   breaks Tessera catches at "done" for at least two agents, and what it
   costs in false blocks and time.

Not required for launch: large-repository speed beyond the cheap fixes
(documented limits instead), field-level contracts, the agent loop, the deep
tier for Python and Go, and the 2.0 surface cuts (#179).

## Principles

- **Gates, not dates.** Dates are targets; a phase ends when its gate passes.
- **Claim only what is measured.** Launch copy is rewritten around the eval;
  anything unmeasured is described as what it does, not what it achieves.
- **No new surface before launch** except what first-run use needs. Every
  new command or flag enters the 1.x contract for good.
- **Unattended releases.** From 1.2.1 on, a tag is the only manual step.

## Phases

### Phase 0: Every install works (2026-11-07 to 2026-11-30) [#264] → 1.2.1

- Trusted publishing on npm (six packages) and crates.io (#95); Homebrew tap
  and token; GHCR image public.
- Smoke tests run on every release and check the right thing (#96, #245);
  the release moves `v1` (#263); notes from the CHANGELOG (#110); a fallback
  runner for the macOS arm64 build.
- Plugin and `init` stop falling back to npx versions that are not published
  (#71); install docs match what is live (#72, #243).
- Docs site serves the latest release, not main (#111).

**Gate:** 1.2.1 is tagged and nothing else is done by hand: every channel
publishes, every smoke test passes on three OSes, `@v1` moves.

### Phase 1: Never silently wrong, and a good first five minutes (2026-12-01 to 2027-01-10) → 1.3.0 [#265]

Trust:
- Silent no-ops: #181, #186, #185, #227, #187, #182, #188, #191, #226,
  #208, #250, #252, #253, #254, #256, #259.
- Blocking false positives: #196, #197, #198, #199, #194, #193, #207, #222.
- Misses users will hit first: #236, #217, #261, #183 (done in 1.2.0).

First run:
- An unknown symbol says so instead of "no callers" (#74).
- `init` sets up the agents the project uses and prints next steps (#73);
  `init` updates the npx entries it wrote earlier (#71).
- One getting-started page built on `init` (#80); README cut to what a first
  visitor needs, translations off the front page until refreshed (#79).
- One answer for "tests to run" (#75); MCP tool descriptions an agent reads
  correctly (#77, #163).

**Gate:** a fault-injection suite (corrupt index, index in progress, missing
binary, no watcher, invalid input) where no case exits 0 silently; fewer than
1 blocking finding per 100 commits that the compiler does not confirm, on the
last 300 commits of five public repositories; someone new to Tessera goes
from install to a hook holding their agent in under five minutes, timed.

### Phase 2: Proof and beta (2026-12-15 to 2027-01-31, overlaps Phase 1) [#267]

- Decide what evidence counts (#169), then run the refactor eval (#133) on
  1.3: with and without Tessera, 30 tasks, at least two agents (Claude Code
  and one other), TypeScript and Java first.
- Measure overlap with compiler and vendor post-edit checks (#170), so the
  pitch names what only Tessera catches.
- **Private beta:** five to ten design partners (teams and individual agent
  users) on 1.3 for four weeks, sharing `tessera stats` (#166 makes that a
  one-liner) and every false block.
- Dogfood: the hooks on Tessera's own development for the same four weeks.

**Gate:** a reproducible eval report (breaks caught at "done" with and
without Tessera, false blocks per 100 edits, time added per edit) and beta
data from at least five repositories.

### Decision point (2027-01-31)

- **Clear effect:** go to Phase 3 with the edit-safety claim.
- **Mixed effect:** fix what the report names (the weakest agent, language
  or check) in Phase 3 and launch with narrower claims; or slip one release.
- **Small effect:** launch on what the data supports (cross-service
  contracts, test selection, dynamic languages), not on edit safety.

### Phase 3: Launch release (2027-02-01 to 2027-02-20) → 1.4.0

- Fixes from the beta and the eval; nothing new unless the beta asks for it.
- The cheap large-repository wins: index-backed symbol lookup (#233),
  `context` (#232, #234), the gRPC scan (#230). #237 if it is ready; if not,
  docs state the tested size (and the hook time on vscode) honestly.
- Launch assets:
  - the eval report on the docs site, linked from the README;
  - a demo anyone can open (#164) and a 90-second video of an agent held at
    "done" and fixing the break;
  - `launch/` posts rewritten around the numbers (Show HN, Reddit, dev.to);
  - beta users' quotes, with permission;
  - README, quickstart and Action README reviewed by someone outside the
    project.
- Release 1.4.0 through the unattended pipeline; RC first (`1.4.0-rc.1`,
  npm `next` tag) to the beta group for one week.

**Gate:** RC used by the beta group for a week with no new silent no-op or
blocking false positive; every channel and smoke test green; the
`launch/RUNBOOK.md` checklist complete.

### Launch week (2027-02-22)

- Monday: tag 1.4.0 (if the RC passed), Show HN.
- Tuesday to Thursday: r/ClaudeAI, r/LocalLLaMA, r/programming, r/rust one a
  day; dev.to post; MCP and agent directories (`launch/directories.md`);
  posts in the communities of the agents whose hooks Tessera supports.
- All week: answer every issue and comment within hours; a 1.4.1 patch for
  whatever the traffic finds.

## After the launch

- **v1.5: Fast on large repositories** (#266), led by #237.
- Then, in the order the eval supports: field-level contracts (#173), the
  agent loop (#174), the deep tier for Python and Go (#175).
- The 2.0 surface cuts (#179) run as a deprecation track from v1.5 on, one or
  two per minor, with 2.0 as a later, separate milestone.

## Success, 90 days after launch

- Adoption from public counters (Tessera collects nothing): npm and
  crates.io downloads, Action and plugin installs, stars.
- Retention proxy: public repositories with a Tessera workflow or a committed
  `.tessera/baseline.json` still active after 30 days.
- Quality: every reported silent no-op or blocking false positive fixed in a
  patch within a week.
- Community: five external contributors with merged packs or fixes (34
  `good first issue` tickets are open).

## Risks

| Risk | Mitigation |
| --- | --- |
| The eval shows a small effect | The decision point changes the pitch or the work before launch, not after |
| Vendors ship the same checks in their agents | #170 measures the overlap; lead with what compilers and vendors cannot see |
| No beta partners sign up | Start recruiting in Phase 0 (agent communities, the contributors on #24–#29, colleagues); dogfooding is the floor |
| Large repositories are slow at launch | Documented limits and measured hook times; #237 is the first post-launch milestone |
| Release depends on accounts and runner queues | Phase 0 makes releases unattended; every remaining manual step is in `launch/RUNBOOK.md` |
| One maintainer | Agents for implementation, the beta for testing, scope cuts before date slips |
