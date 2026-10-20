# Adding a language

Each language is one module behind the `LanguageSupport` trait in
`src/extract/`. The trait has three jobs:

- turn tree-sitter nodes into **symbols** (definitions),
- turn them into **references** (calls, constructors),
- turn them into **imports**.

Scoping, qualified names, and storage are shared across languages.

## 1. Add the grammar

Add the grammar crate to `Cargo.toml` (it must depend on
`tree-sitter-language`, like the existing ones). Then check that it builds
with `cargo +1.90 check`.

## 2. Register the language

In `src/types.rs`, add a `Language` variant and its file extensions to
`from_extension`, `from_name`, and `family`. Then wire the variant into:

- `src/extract/lang/mod.rs`, in `support_for`;
- `src/main.rs`, in the `LangArg` enum and its `From` impl;
- `src/mcp/tools.rs`, in the `language` enum of the `verify` schema;
- `src/query/common.rs`, in `known_global`, for standard-library names that
  `verify` should not flag.

## 3. Write the extractor

Create `src/extract/lang/<language>.rs`. `python.rs` is the smallest complete
example:

```rust
pub struct Kotlin;

impl LanguageSupport for Kotlin {
    fn language(&self) -> Language { Language::Kotlin }
    fn grammar(&self) -> tree_sitter::Language { tree_sitter_kotlin::LANGUAGE.into() }

    fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
        let kind = match node.kind() {
            "function_declaration" => "function",
            "class_declaration" => "class",
            _ => return None,
        };
        let name = cx.text(node.child_by_field_name("name")?);
        Some(cx.symbol(name, kind, node, true))
    }

    fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
        match node.kind() {
            "call_expression" => cx.call_reference(node.child(0)?, "call", node),
            _ => None,
        }
    }

    fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
        (node.kind() == "import_header").then(|| cx.import(cx.text(node), "import", node))?
    }
}
```

`Cx` provides `text`, `symbol` (which qualifies the name with the enclosing
symbol automatically), `reference`, `call_reference` (which picks the callee
name out of member and scoped expressions), and `import`. To find node kinds
and field names, run the grammar's playground, or print a tree with
`tree.root_node().to_sexp()` in a scratch test.

## 4. Add a fixture and tests

1. Create `tests/fixtures/<language>/` with a small app containing:
   - an exported function called from another file,
   - a class with a method called through a receiver,
   - a constructor call,
   - the same function name in two files,
   - a test file,
   - an import,
   - a line of non-ASCII text over 200 bytes.
2. Add `golden!(golden_<language>, "<language>");` to `tests/golden.rs` and
   generate the snapshot:

   ```sh
   INSTA_UPDATE=always cargo test --test golden
   ```

   Read `tests/snapshots/golden__golden_<language>.snap` line by line. It is
   exactly what Tessera will tell agents about that code.
3. Add the language and one known symbol to `indexes_every_supported_language`
   in `tests/cli.rs`, and to `graph_is_identical_across_rebuilds` in
   `tests/robustness.rs`.
4. Add a row to the language list in `README.md`.

## Deep tier

A language reaches the **deep** tier once it resolves references through
imports and receiver types, with a confidence level on each edge, and passes
the `tessera-bench` accuracy gate. That work starts with TypeScript,
JavaScript, and Java in v0.11. Open an issue before starting it for another
language.
