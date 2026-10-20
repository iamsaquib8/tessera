---
name: tessera
description: Use before and after editing code in a repository - to see what a change affects (callers ranked by impact, tests to run), to find definitions and call sites without grep, to read one budgeted context bundle before editing a symbol, and to verify that newly written code only calls functions that exist. Deterministic local index, no LLM; works with TypeScript/JavaScript/React, Java, Python, Go, Rust, C, C++, C#, Ruby, PHP.
---

# Tessera: check before and after you edit

Tessera keeps a local, deterministic index of the repository (tree-sitter →
SQLite). Use it instead of grep and opening files to answer structural
questions, and use it to check your own edits.

## Setup (once)

Check `tessera --version`. If it is missing, try these in order:

1. `npx --yes tessera-codegraph <command>` (no install; needs Node 18+). Use
   this prefix in place of `tessera` below.
2. `curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh`
   (verifies the checksum; installs to `~/.local/bin`).
3. `brew install iamsaquib8/tessera/tessera`.

There is no index step: the first command builds `.tessera/` and later
commands refresh only what changed. If results look wrong, run
`tessera status`.

## The loop

**Before editing a symbol:**

```sh
tessera blast <symbol>        # who is affected, ranked, and which tests to run
tessera context <symbol>      # body + what it calls + who calls it + tests
```

**Finding things:**

```sh
tessera find <symbol> --body  # definition (name, Class.method, or A::B)
tessera find --path <file>    # outline of a file or directory
tessera refs <symbol>         # every call site
tessera refs <file>           # files that import this file
tessera search 'load*'        # fuzzy or glob search; --unused for dead functions
tessera path <from> <to>      # does A reach B, and how
```

**After writing code that calls project functions:**

```sh
tessera verify --snippet <file> --language typescript
# or: printf '%s' "$CODE" | tessera verify --snippet - --language python
```

`verify` exits 1 and names the closest real symbol for anything that does not
exist. Fix those before saying the task is done.

## Reading results

- Results are paginated. `[25 of 140 shown; more with cursor o:25]` means:
  re-run with `--cursor o:25` if you need more.
- `No exact definition of X; closest matches:` means X does not exist; use
  one of the suggestions.
- Matching is by name for now, so common names (`get`, `run`) can show extra
  callers. Check the file paths.
- Add `--json` for machine-readable output.

## Prefer MCP when available

If the agent supports MCP, register `tessera mcp` once. The same eight tools
(`find`, `search`, `refs`, `blast`, `verify`, `context`, `path`, `status`)
become available without shell calls, and the server keeps the index fresh as
files change.
