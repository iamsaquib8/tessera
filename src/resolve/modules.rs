use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

use crate::types::Language;

const JS_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "d.ts"];
// TypeScript ESM imports name the emitted `.js` file; the source is `.ts`.
const JS_SOURCE_FOR_EMITTED: &[(&str, &[&str])] = &[
    ("js", &["ts", "tsx"]),
    ("jsx", &["tsx"]),
    ("mjs", &["mts"]),
    ("cjs", &["cts"]),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleTarget {
    File(i64),
    External(String),
    Unresolved,
}

#[derive(Debug, Clone, Default)]
struct TsConfig {
    /// Directory `paths` targets are relative to, root-relative.
    base: Option<String>,
    paths: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone)]
struct WorkspacePackage {
    dir: String,
    entry: Option<String>,
}

/// Maps import specifiers to indexed files. Built once per resolve pass from
/// the `files` table plus the tsconfig and package.json files on disk.
#[derive(Debug, Default)]
pub struct ModuleResolver {
    files: HashMap<String, i64>,
    /// Directory (root-relative, "" for the root) → nearest tsconfig.
    tsconfigs: HashMap<String, TsConfig>,
    packages: HashMap<String, WorkspacePackage>,
    /// `(package, class)` → file, from each Java file's module and stem.
    java_classes: HashMap<(String, String), i64>,
}

impl ModuleResolver {
    pub fn build(conn: &Connection, root: &Path) -> Result<Self> {
        let mut stmt = conn.prepare("SELECT id, path, language, module FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut files = HashMap::new();
        let mut java = Vec::new();
        for row in rows {
            let (id, path, language, module) = row?;
            if language == Language::Java.to_string() {
                java.push((id, path.clone(), module));
            }
            files.insert(path, id);
        }
        let mut resolver = Self::from_files(root, files);
        for (id, path, module) in java {
            let stem = Path::new(&path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            resolver
                .java_classes
                .insert((module.unwrap_or_default(), stem), id);
        }
        Ok(resolver)
    }

    /// Builds the JS/TS and Python parts from a path → id map; reads tsconfig
    /// and package.json files under `root`.
    pub fn from_files(root: &Path, files: HashMap<String, i64>) -> Self {
        let mut resolver = Self {
            files,
            ..Self::default()
        };
        resolver.load_tsconfigs(root);
        resolver.load_workspaces(root);
        resolver
    }

    pub fn resolve(&self, from_file: &str, source: &str, language: Language) -> ModuleTarget {
        match language {
            Language::JavaScript | Language::TypeScript | Language::Tsx => {
                self.resolve_js(from_file, source)
            }
            Language::Java => self.resolve_java(source),
            Language::Python => self.resolve_python(from_file, source),
            _ => ModuleTarget::Unresolved,
        }
    }

    /// The file declaring `class` in `package`, for wildcard imports and
    /// same-package references.
    pub fn java_class(&self, package: &str, class: &str) -> Option<i64> {
        self.java_classes
            .get(&(package.to_string(), class.to_string()))
            .copied()
    }

    /// Whether any indexed Java file declares `package`.
    pub fn java_package_exists(&self, package: &str) -> bool {
        !package.is_empty() && self.java_classes.keys().any(|(p, _)| p == package)
    }

    pub fn file_id(&self, path: &str) -> Option<i64> {
        self.files.get(path).copied()
    }

    fn resolve_js(&self, from_file: &str, source: &str) -> ModuleTarget {
        if source.starts_with('.') || source.starts_with('/') {
            let base = parent_dir(from_file);
            let target = if source.starts_with('/') {
                normalize(source.trim_start_matches('/'))
            } else {
                normalize(&join(&base, source))
            };
            return target
                .and_then(|path| self.probe_js(&path))
                .map_or(ModuleTarget::Unresolved, ModuleTarget::File);
        }
        if let Some(id) = self.resolve_tsconfig(from_file, source) {
            return ModuleTarget::File(id);
        }
        if let Some(id) = self.resolve_workspace(source) {
            return ModuleTarget::File(id);
        }
        ModuleTarget::External(package_name(source).to_string())
    }

    fn probe_js(&self, path: &str) -> Option<i64> {
        if let Some(id) = self.files.get(path) {
            return Some(*id);
        }
        if let Some((stem, ext)) = path.rsplit_once('.') {
            if let Some((_, sources)) = JS_SOURCE_FOR_EMITTED.iter().find(|(e, _)| *e == ext) {
                for source in *sources {
                    if let Some(id) = self.files.get(&format!("{stem}.{source}")) {
                        return Some(*id);
                    }
                }
            }
        }
        for ext in JS_EXTENSIONS {
            if let Some(id) = self.files.get(&format!("{path}.{ext}")) {
                return Some(*id);
            }
        }
        for ext in JS_EXTENSIONS {
            let index = if path.is_empty() {
                format!("index.{ext}")
            } else {
                format!("{path}/index.{ext}")
            };
            if let Some(id) = self.files.get(&index) {
                return Some(*id);
            }
        }
        None
    }

    fn resolve_tsconfig(&self, from_file: &str, source: &str) -> Option<i64> {
        let config = self.nearest_tsconfig(from_file)?;
        let base = config.base.clone().unwrap_or_default();
        for (pattern, targets) in &config.paths {
            let captured = match pattern.split_once('*') {
                Some((prefix, suffix)) => source
                    .strip_prefix(prefix)
                    .and_then(|rest| rest.strip_suffix(suffix)),
                None => (source == pattern).then_some(""),
            };
            let Some(captured) = captured else {
                continue;
            };
            for target in targets {
                let target = target.replacen('*', captured, 1);
                if let Some(id) = normalize(&join(&base, &target)).and_then(|p| self.probe_js(&p)) {
                    return Some(id);
                }
            }
        }
        config
            .base
            .as_ref()
            .and_then(|base| normalize(&join(base, source)))
            .and_then(|p| self.probe_js(&p))
    }

    fn nearest_tsconfig(&self, from_file: &str) -> Option<&TsConfig> {
        let mut dir = parent_dir(from_file);
        loop {
            if let Some(config) = self.tsconfigs.get(&dir) {
                return Some(config);
            }
            if dir.is_empty() {
                return None;
            }
            dir = parent_dir(&dir);
        }
    }

    fn resolve_workspace(&self, source: &str) -> Option<i64> {
        let name = package_name(source);
        let package = self.packages.get(name)?;
        let rest = source[name.len()..].trim_start_matches('/');
        if !rest.is_empty() {
            return normalize(&join(&package.dir, rest)).and_then(|p| self.probe_js(&p));
        }
        let entry = package
            .entry
            .as_deref()
            .and_then(|entry| normalize(&join(&package.dir, entry)))
            .and_then(|p| self.probe_js(&p));
        entry
            .or_else(|| self.probe_js(&join(&package.dir, "src/index")))
            .or_else(|| self.probe_js(&package.dir))
    }

    fn resolve_java(&self, source: &str) -> ModuleTarget {
        // `a.b.Outer.Inner` lives in a/b/Outer.java: try ever shorter prefixes.
        let parts: Vec<&str> = source.split('.').collect();
        for split in (1..=parts.len()).rev() {
            let package = parts[..split - 1].join(".");
            if let Some(id) = self.java_class(&package, parts[split - 1]) {
                return ModuleTarget::File(id);
            }
        }
        ModuleTarget::External(source.to_string())
    }

    fn resolve_python(&self, from_file: &str, source: &str) -> ModuleTarget {
        let dots = source.chars().take_while(|c| *c == '.').count();
        let module = &source[dots..];
        let mut base = if dots == 0 {
            String::new()
        } else {
            let mut dir = parent_dir(from_file);
            for _ in 1..dots {
                dir = parent_dir(&dir);
            }
            dir
        };
        if !module.is_empty() {
            base = join(&base, &module.replace('.', "/"));
        }
        for candidate in [format!("{base}.py"), join(&base, "__init__.py")] {
            if let Some(id) = self.files.get(&candidate) {
                return ModuleTarget::File(*id);
            }
        }
        if dots > 0 {
            ModuleTarget::Unresolved
        } else {
            ModuleTarget::External(module.split('.').next().unwrap_or(module).to_string())
        }
    }

    fn load_tsconfigs(&mut self, root: &Path) {
        let mut dirs: Vec<String> =
            self.files
                .keys()
                .filter(|path| {
                    Language::from_extension(path.rsplit('.').next().unwrap_or_default())
                        .is_some_and(|l| {
                            matches!(
                                l,
                                Language::JavaScript | Language::TypeScript | Language::Tsx
                            )
                        })
                })
                .flat_map(|path| ancestors(path))
                .collect();
        dirs.sort();
        dirs.dedup();
        for dir in dirs {
            for name in ["tsconfig.json", "jsconfig.json"] {
                let rel = join(&dir, name);
                if let Some(config) = read_tsconfig(root, &rel, 0) {
                    self.tsconfigs.insert(dir.clone(), config);
                    break;
                }
            }
        }
    }

    fn load_workspaces(&mut self, root: &Path) {
        let mut patterns = Vec::new();
        if let Some(manifest) = read_json(&root.join("package.json")) {
            match &manifest["workspaces"] {
                Value::Array(list) => patterns.extend(strings(list)),
                Value::Object(map) => {
                    if let Some(Value::Array(list)) = map.get("packages") {
                        patterns.extend(strings(list));
                    }
                }
                _ => {}
            }
        }
        if let Ok(yaml) = fs::read_to_string(root.join("pnpm-workspace.yaml")) {
            patterns.extend(
                yaml.lines()
                    .filter_map(|line| line.trim().strip_prefix("- "))
                    .map(|item| item.trim().trim_matches(['\'', '"']).to_string()),
            );
        }
        for pattern in patterns {
            for dir in expand_workspace_pattern(root, &pattern) {
                let Some(manifest) = read_json(&root.join(&dir).join("package.json")) else {
                    continue;
                };
                let Some(name) = manifest["name"].as_str() else {
                    continue;
                };
                let entry = package_entry(&manifest);
                self.packages
                    .insert(name.to_string(), WorkspacePackage { dir, entry });
            }
        }
    }
}

fn read_tsconfig(root: &Path, rel: &str, depth: usize) -> Option<TsConfig> {
    let json = read_json(&root.join(rel))?;
    let dir = parent_dir(rel);
    let mut config = TsConfig::default();
    if depth == 0 {
        if let Some(parent) = json["extends"].as_str().filter(|p| p.starts_with('.')) {
            let parent = if parent.ends_with(".json") {
                parent.to_string()
            } else {
                format!("{parent}.json")
            };
            if let Some(inherited) =
                normalize(&join(&dir, &parent)).and_then(|p| read_tsconfig(root, &p, 1))
            {
                config = inherited;
            }
        }
    }
    let options = &json["compilerOptions"];
    if let Some(base_url) = options["baseUrl"].as_str() {
        config.base = normalize(&join(&dir, base_url));
    }
    if let Value::Object(paths) = &options["paths"] {
        // Without baseUrl, paths resolve against the config's own directory.
        let base = config.base.clone().unwrap_or_else(|| dir.clone());
        config.base = Some(base);
        config.paths = paths
            .iter()
            .map(|(pattern, targets)| {
                let targets = match targets {
                    Value::Array(list) => strings(list),
                    _ => Vec::new(),
                };
                (pattern.clone(), targets)
            })
            .collect();
        // Longest prefix first, as TypeScript matches the most specific pattern.
        config.paths.sort_by_key(|(pattern, _)| {
            std::cmp::Reverse(pattern.split('*').next().map_or(0, str::len))
        });
    }
    Some(config)
}

fn package_entry(manifest: &Value) -> Option<String> {
    let exports = &manifest["exports"];
    let dot = match exports {
        Value::String(entry) => Some(entry.clone()),
        Value::Object(map) => {
            let target = map.get(".").unwrap_or(exports);
            match target {
                Value::String(entry) => Some(entry.clone()),
                Value::Object(conditions) => ["types", "import", "default", "require"]
                    .iter()
                    .find_map(|key| conditions.get(*key).and_then(Value::as_str))
                    .map(ToOwned::to_owned),
                _ => None,
            }
        }
        _ => None,
    };
    dot.or_else(|| {
        ["types", "module", "main"]
            .iter()
            .find_map(|key| manifest[*key].as_str())
            .map(ToOwned::to_owned)
    })
    .map(|entry| entry.trim_start_matches("./").to_string())
}

fn expand_workspace_pattern(root: &Path, pattern: &str) -> Vec<String> {
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    match pattern
        .strip_suffix("/*")
        .or_else(|| pattern.strip_suffix("/**"))
    {
        Some(parent) => {
            let Ok(entries) = fs::read_dir(root.join(parent)) else {
                return Vec::new();
            };
            let mut dirs: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| join(parent, &e.file_name().to_string_lossy()))
                .collect();
            dirs.sort();
            dirs
        }
        None if !pattern.contains('*') => vec![pattern.to_string()],
        None => Vec::new(),
    }
}

/// `@scope/pkg/sub` → `@scope/pkg`; `pkg/sub` → `pkg`.
fn package_name(source: &str) -> &str {
    let mut parts = source.splitn(3, '/');
    let first = parts.next().unwrap_or(source);
    if first.starts_with('@') {
        match parts.next() {
            Some(second) => &source[..first.len() + 1 + second.len()],
            None => source,
        }
    } else {
        first
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc(&text)).ok()
}

/// tsconfig files allow comments and trailing commas.
fn strip_jsonc(text: &str) -> String {
    drop_trailing_commas(&drop_comments(text))
}

fn drop_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            _ => {
                in_string = c == '"';
                out.push(c);
            }
        }
    }
    out
}

fn drop_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.get(i + 1) {
                    out.push(*next);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            in_string = c == '"';
            out.push(c);
        }
        i += 1;
    }
    out
}

fn strings(list: &[Value]) -> Vec<String> {
    list.iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

fn ancestors(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut dir = parent_dir(path);
    loop {
        out.push(dir.clone());
        if dir.is_empty() {
            return out;
        }
        dir = parent_dir(&dir);
    }
}

fn join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

/// Resolves `.` and `..` lexically; `None` when the path escapes the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in PathBuf::from(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn resolver(root: &Path, paths: &[&str]) -> ModuleResolver {
        let files = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.to_string(), i as i64 + 1))
            .collect();
        ModuleResolver::from_files(root, files)
    }

    fn id(paths: &[&str], path: &str) -> ModuleTarget {
        ModuleTarget::File(paths.iter().position(|p| *p == path).unwrap() as i64 + 1)
    }

    #[test]
    fn relative_specifiers_probe_extensions_index_and_emitted_js() {
        let temp = TempDir::new().unwrap();
        let paths = [
            "src/a.ts",
            "src/b.tsx",
            "src/lib/index.ts",
            "src/esm.ts",
            "src/plain.js",
        ];
        let r = resolver(temp.path(), &paths);
        let ts = Language::TypeScript;
        assert_eq!(r.resolve("src/a.ts", "./b", ts), id(&paths, "src/b.tsx"));
        assert_eq!(
            r.resolve("src/a.ts", "./lib", ts),
            id(&paths, "src/lib/index.ts")
        );
        assert_eq!(
            r.resolve("src/lib/index.ts", "../a", ts),
            id(&paths, "src/a.ts")
        );
        assert_eq!(
            r.resolve("src/a.ts", "./esm.js", ts),
            id(&paths, "src/esm.ts")
        );
        assert_eq!(
            r.resolve("src/a.ts", "./plain.js", ts),
            id(&paths, "src/plain.js")
        );
        assert_eq!(
            r.resolve("src/a.ts", "./missing", ts),
            ModuleTarget::Unresolved
        );
        assert_eq!(
            r.resolve("src/a.ts", "../../../x", ts),
            ModuleTarget::Unresolved
        );
        assert_eq!(
            r.resolve("src/a.ts", "@scope/pkg/deep", ts),
            ModuleTarget::External("@scope/pkg".to_string())
        );
    }

    #[test]
    fn tsconfig_paths_base_url_and_extends() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("tsconfig.base.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@app/*": ["src/*"], "@lib": ["libs/core/main.ts"], } } }"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("tsconfig.json"),
            "// project config\n{ \"extends\": \"./tsconfig.base\", /* inherits paths */ }\n",
        )
        .unwrap();
        let paths = [
            "src/data/repo.ts",
            "libs/core/main.ts",
            "src/app.ts",
            "utils/x.ts",
        ];
        let r = resolver(temp.path(), &paths);
        let ts = Language::TypeScript;
        assert_eq!(
            r.resolve("src/app.ts", "@app/data/repo", ts),
            id(&paths, "src/data/repo.ts")
        );
        assert_eq!(
            r.resolve("src/app.ts", "@lib", ts),
            id(&paths, "libs/core/main.ts")
        );
        assert_eq!(
            r.resolve("src/app.ts", "utils/x", ts),
            id(&paths, "utils/x.ts")
        );
        assert_eq!(
            r.resolve("src/app.ts", "react", ts),
            ModuleTarget::External("react".to_string())
        );
    }

    #[test]
    fn workspace_packages_resolve_to_their_entry() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("package.json"),
            r#"{ "workspaces": ["packages/*"] }"#,
        )
        .unwrap();
        for (dir, manifest) in [
            (
                "packages/ui",
                r#"{ "name": "@acme/ui", "exports": { ".": { "import": "./src/index.ts" } } }"#,
            ),
            (
                "packages/util",
                r#"{ "name": "util", "main": "dist/index.js" }"#,
            ),
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
            fs::write(root.join(dir).join("package.json"), manifest).unwrap();
        }
        let paths = [
            "packages/ui/src/index.ts",
            "packages/ui/src/button.tsx",
            "packages/util/src/index.ts",
            "apps/web/main.ts",
        ];
        let r = resolver(root, &paths);
        let ts = Language::TypeScript;
        assert_eq!(
            r.resolve("apps/web/main.ts", "@acme/ui", ts),
            id(&paths, "packages/ui/src/index.ts")
        );
        assert_eq!(
            r.resolve("apps/web/main.ts", "@acme/ui/src/button", ts),
            id(&paths, "packages/ui/src/button.tsx")
        );
        assert_eq!(
            r.resolve("apps/web/main.ts", "util", ts),
            id(&paths, "packages/util/src/index.ts")
        );
    }

    #[test]
    fn java_classes_resolve_through_their_package() {
        let temp = TempDir::new().unwrap();
        let mut r = resolver(temp.path(), &[]);
        r.java_classes
            .insert(("com.acme.user".to_string(), "UserService".to_string()), 7);
        r.java_classes
            .insert(("com.acme".to_string(), "Outer".to_string()), 8);
        assert_eq!(
            r.resolve("x.java", "com.acme.user.UserService", Language::Java),
            ModuleTarget::File(7)
        );
        assert_eq!(
            r.resolve("x.java", "com.acme.Outer.Inner", Language::Java),
            ModuleTarget::File(8)
        );
        assert_eq!(
            r.resolve("x.java", "java.util.List", Language::Java),
            ModuleTarget::External("java.util.List".to_string())
        );
        assert_eq!(r.java_class("com.acme.user", "UserService"), Some(7));
    }

    #[test]
    fn python_modules_and_relative_imports() {
        let temp = TempDir::new().unwrap();
        let paths = [
            "app/models.py",
            "app/services/__init__.py",
            "app/services/users.py",
        ];
        let r = resolver(temp.path(), &paths);
        let py = Language::Python;
        assert_eq!(
            r.resolve("main.py", "app.models", py),
            id(&paths, "app/models.py")
        );
        assert_eq!(
            r.resolve("main.py", "app.services", py),
            id(&paths, "app/services/__init__.py")
        );
        assert_eq!(
            r.resolve("app/services/users.py", "..models", py),
            id(&paths, "app/models.py")
        );
        assert_eq!(
            r.resolve("app/services/__init__.py", ".users", py),
            id(&paths, "app/services/users.py")
        );
        assert_eq!(
            r.resolve("main.py", "requests.adapters", py),
            ModuleTarget::External("requests".to_string())
        );
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_stripped() {
        let parsed: Value = serde_json::from_str(&strip_jsonc(
            "{\"a\": \"x//y\", /* c */ \"b\": [1, 2,], // t\n}",
        ))
        .unwrap();
        assert_eq!(parsed["a"], "x//y");
        assert_eq!(parsed["b"], serde_json::json!([1, 2]));
    }
}
