# Adding an agent integration

Agent support is built from the agent vendor's own documentation, never from
guesses. [docs/agents.md](agents.md) is the source of truth. Each row records:

- the MCP config path and format,
- the hook events and how they block or add context,
- the instructions file,
- the distribution mechanism (plugin, extension, or directory listing),
- a link to the vendor documentation that confirms the row.

## Today: MCP setup

1. Confirm the agent's MCP config location and format in its documentation.
2. Add or update the agent's row in `docs/agents.md`, with the source link.
   Mark anything the documentation does not confirm as **unverified**.
3. Add a setup snippet to `docs/integrations.md`, then try it: the agent
   should list eight tools from `tessera mcp`.

## From v0.12: hook adapters

v0.12 adds an adapter registry so that `tessera init` can configure each agent
and `tessera hook <agent> <event>` can serve its hooks. Each adapter will
declare:

- how to detect that the agent is installed,
- where its MCP config lives and in what format, at project and user level,
- its hook events, their input JSON shape, and how a hook blocks or adds
  context,
- its instructions file and any trust step the user must take.

Each adapter comes with a fixture test that feeds a recorded hook payload
through `tessera hook` and checks the response. The design is in
[docs/design/2026-10-21-overhaul.md](design/2026-10-21-overhaul.md) §5.3.
Use the agent integration issue template to claim an agent.
