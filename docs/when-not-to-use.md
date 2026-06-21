# When Not To Use Tessera

Tessera is a deterministic local code graph built from Tree-sitter syntax. That
is intentionally different from a language server, runtime tracer, or
LLM-extracted project summary.

Use another tool, or treat Tessera as partial signal, when:

- The code is generated and changes frequently. Indexing works, but results may
  be noisy and low-value.
- The relationship depends on type inference only available from a full language
  server or compiler.
- Dynamic dispatch, reflection, metaprogramming, dependency injection, or runtime
  routing creates edges not visible in syntax.
- Runtime-only edges matter, such as telemetry traces, RPC calls, SQL callbacks,
  template invocations, or framework magic.
- The language or file type is not covered by Tessera's supported grammars.
- You need a semantic explanation of why the system exists, not exact navigation
  facts about symbols, references, imports, and call paths.

Tessera is strongest when the question is local, factual, and graph-shaped:
"where is this defined?", "who calls it?", "does A reach B?", "what changed in
this PR?", and "did the agent hallucinate this symbol?"

