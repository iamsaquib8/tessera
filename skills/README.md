# Tessera Agent Skill

`tessera/SKILL.md` is a drop-in [Agent Skill](https://docs.claude.com/en/docs/claude-code/skills)
that teaches Claude Code (and other skill-aware agents) to use Tessera for
deterministic code navigation instead of burning context on `grep` + file reads.

## Install (Claude Code)

```sh
mkdir -p ~/.claude/skills
cp -r skills/tessera ~/.claude/skills/tessera
```

Then in any session: ask a navigation question ("who calls `parseConfig`?",
"does `frobnicate` exist?", "how does `handleRequest` reach the DB?") and the
skill activates. It will install the `tessera` binary on first use if needed
(no Rust toolchain required — it can use the npm wrapper).

## Other agents

The same `SKILL.md` works with any tool that supports the Agent Skills format.
For a deeper, always-on integration, wire up the MCP server instead — see
[../docs/integrations.md](../docs/integrations.md).
