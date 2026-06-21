# Configuration

Tessera works without config. Add `.tessera/config.toml` when a real repo needs
explicit include or exclude rules.

```toml
[include]
# Empty means all supported source files under the repository root.
paths = ["src/", "packages/api/"]

[exclude]
# Relative path prefixes to skip after built-in ignores.
paths = ["src/generated/", "packages/api/fixtures/"]

[ignore]
# Extra directory or file names to skip during traversal.
extra = ["vendor-generated"]
```

Built-in ignored names cover common repository noise:

- VCS: `.git`, `.hg`, `.svn`
- package/build output: `node_modules`, `target`, `dist`, `build`, `out`
- generated/vendor/cache: `coverage`, `vendor`, `vendors`, `third_party`, `.cache`
- ecosystem caches: `.gradle`, `.idea`, `.pytest_cache`, `.mypy_cache`,
  `.ruff_cache`, `.tox`, `.cargo`, `Pods`, `DerivedData`
- Python/env and Tessera internals: `.venv`, `venv`, `__pycache__`, `.tessera`

Indexing warnings are non-fatal. Unreadable files, traversal errors, and parser
failures are reported on stderr while the rest of the index continues.
