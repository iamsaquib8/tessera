# Adding an agent integration

Agent support is built from the agent vendor's own documentation, never from
guesses. [docs/agents.md](agents.md) is the source of truth. Each row records:

- the MCP config path and format,
- the hook events and how they block or add context,
- the instructions file,
- the distribution mechanism (plugin, extension, or directory listing),
- a link to the vendor documentation that confirms the row.

## The adapter

Each agent is one entry in the registry in
[`src/agents/mod.rs`](../src/agents/mod.rs):

- how to detect it: project files, executables on `PATH`, directories in
  the home directory;
- where its MCP config lives (project and user-wide) and in which format
  (`McpFormat`);
- if it has a hook adapter, its hook file, layout (`HookLayout`) and events;
- its instruction file, the trust step only the user can take, and what
  Tessera cannot do for it yet.

`tessera init` reads the registry; nothing else needs to change for an
agent whose config formats already exist.

## Steps

1. Confirm the agent's MCP config location and format, and its hook
   contract (event names, input JSON, how a hook blocks or adds context),
   in its documentation.
2. Add or update the agent's row in `docs/agents.md`, with the source link.
   Mark anything the documentation does not confirm as **unverified**, and
   do not implement it.
3. Add the registry entry. If its hook output has a new shape, add an
   `Agent` variant in [`src/hook/mod.rs`](../src/hook/mod.rs) and its arms in
   `render` (and `silent` if it needs JSON on every run). New payload field
   names go in [`src/hook/payload.rs`](../src/hook/payload.rs).
4. Add tests: a recorded payload from the vendor docs through
   `tessera hook <agent> <event>` in `tests/hook.rs`, and the agent in
   `every_hook_adapter_is_wired_in_its_documented_shape` in `tests/init.rs`.
5. Add the agent to the table in [hooks.md](hooks.md) and to
   [integrations.md](integrations.md).

Use the agent integration issue template to claim an agent.
