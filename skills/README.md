# Tessera agent skill

`skills/tessera/SKILL.md` teaches skill-aware agents (Claude Code and
others) when and how to use Tessera: check the blast radius and context
before an edit, and verify after it. If Tessera is missing, the skill tells
the agent how to install it.

```sh
cp -r skills/tessera ~/.claude/skills/tessera
```

Agents that support MCP get the same capabilities as tools via
`tessera mcp`; see [docs/integrations.md](../docs/integrations.md).
