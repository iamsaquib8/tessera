# Reddit: r/LocalLLaMA

Audience: people running models locally, wary of anything that phones home
or needs a hosted model. Lead with model-agnostic and fully local. No token
multipliers; say what it checks and how it plugs into the agent you
already run.

## Title

`Tessera 1.0: deterministic checks for coding-agent edits. No LLM inside, no network calls, works with whatever model drives your agent`

## Body

Whatever model writes the code, a coding agent fails the same way: it
renames a function, updates the file it was looking at, and says done,
while a caller somewhere else still uses the old name. I built Tessera to
check for that without adding another model to the loop.

**How it works**

Tessera indexes the repository with tree-sitter into a SQLite file in
`.tessera/` (gitignored automatically). The same code gives the same answer
every time. Tessera never calls a model and makes no network calls, so
Qwen, DeepSeek, Llama-family models, or anything else you serve locally all
work. The only thing that matters is the agent harness you run them in.

- **Before an edit:** `tessera blast <symbol>` ranks the callers a change
  can affect and lists the tests to run. `tessera context <symbol>` gives
  the body, callers, callees and tests in one budgeted read.
- **After an edit:** `tessera verify --changed` reports calls to names that
  do not exist, wrong argument counts, references to something the change
  removed, broken imports, and routes or gRPC methods removed while another
  service still calls them. Exit 1 on a blocking problem.

**Plugging it into your agent**

- **Hooks that hold the agent at "done" until the break is fixed:** Claude
  Code, Codex CLI, Gemini CLI, Copilot CLI, Cursor, Qwen Code, Augment and
  Antigravity.
- **MCP tools only:** OpenCode, Kilo Code, Goose, Continue, Cline, Zed,
  Kiro, Amp, Warp and others. Nine read-only tools (`find`, `search`,
  `refs`, `blast`, `verify`, `context`, `path`, `map`, `status`) that return
  compact, paginated text, so they cost few tokens.
- **Aider:** no MCP, but its lint loop can run `tessera verify`.
- **Everything, including humans:** `tessera init --git-hook` installs a
  pre-commit hook that refuses a commit that breaks a call site or an
  import.

`tessera init` detects which of 21 agents a project uses, shows the diff of
every config change, and asks before writing.

**Numbers** (public benchmark, compiler ground truth from scip-typescript
and scip-java):

- 99.7% of high-confidence references are correct on excalidraw, nest,
  Ghost and two Spring applications.
- Recall in TypeScript/JavaScript: 99% of direct calls, 91% of calls on
  typed receivers, 99% of type references.
- Lookups take under 50 ms at p95 on 10,000 files, and a 45,000-file
  repository indexes in under a minute. No GPU, no model.

**Limits**

- Deep resolution (imports, re-exports, tsconfig paths, declared types,
  overloads) is TypeScript, JavaScript and Java. Python, Go, Rust, C, C++,
  C#, Ruby and PHP resolve within a file and by unique name, so common
  names come back as "possible" matches.
- Hooks block only on high-confidence TS/JS/Java findings.
- Dynamic dispatch, reflection, DI by name and build-time codegen are not
  followed. It does not type-check, so keep the compiler and your tests.

Rust, Apache-2.0, 12 languages: https://github.com/iamsaquib8/tessera

A question for this sub: which agent harness do you drive local models
with, and does it have a hook system? I would like to add adapters where the
hook contract allows blocking, and I would rather hear which ones people
actually use than guess.
