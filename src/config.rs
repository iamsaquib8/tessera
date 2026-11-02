use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TesseraConfig {
    /// Only index paths with one of these relative path prefixes. Empty means all.
    pub include_paths: Vec<String>,
    /// Skip paths with one of these relative path prefixes.
    pub exclude_paths: Vec<String>,
    /// Extra directory or file names to skip during traversal.
    pub ignore_names: Vec<String>,
    /// Glob patterns (relative to the root) to skip.
    pub exclude_globs: Vec<String>,
    /// Files larger than this are skipped; `None` uses the walker default.
    pub max_file_bytes: Option<u64>,
    /// Path to the database file; `None` uses `.tessera/tessera.db`.
    pub db: Option<String>,
    /// Debounce delay for `watch` in milliseconds.
    pub watch_debounce_ms: Option<u64>,
    /// Poll interval for `watch` in milliseconds.
    pub watch_poll_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigFile {
    include: IncludeSection,
    exclude: ExcludeSection,
    ignore: IgnoreSection,
    index: IndexSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IncludeSection {
    paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExcludeSection {
    paths: Vec<String>,
    globs: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IgnoreSection {
    extra: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IndexSection {
    max_file_bytes: Option<u64>,
    db: Option<String>,
    watch_debounce_ms: Option<u64>,
    watch_poll_ms: Option<u64>,
}

impl TesseraConfig {
    pub fn load(root: &Path) -> Self {
        let path = root.join(".tessera/config.toml");
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse_lossy(&content)
    }

    pub fn parse_lossy(content: &str) -> Self {
        if let Ok(file) = toml::from_str::<ConfigFile>(content) {
            return Self {
                include_paths: normalize_prefixes(file.include.paths),
                exclude_paths: normalize_prefixes(file.exclude.paths),
                ignore_names: file
                    .ignore
                    .extra
                    .into_iter()
                    .map(|s| s.replace('\\', "/"))
                    .collect(),
                exclude_globs: file
                    .exclude
                    .globs
                    .into_iter()
                    .map(|s| s.replace('\\', "/"))
                    .collect(),
                max_file_bytes: file.index.max_file_bytes,
                db: file.index.db.map(|s| s.replace('\\', "/")),
                watch_debounce_ms: file.index.watch_debounce_ms,
                watch_poll_ms: file.index.watch_poll_ms,
            };
        }

        Self::parse_fallback(content)
    }

    fn parse_fallback(content: &str) -> Self {
        let mut config = Self::default();
        let mut section = String::new();
        let mut in_array = false;
        let mut array_buf = String::new();
        let mut array_section = String::new();
        let mut array_key = String::new();

        for raw_line in content.lines() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if in_array {
                array_buf.push(' ');
                array_buf.push_str(line);
                if line.contains(']') {
                    in_array = false;
                    let values = parse_string_array(&array_buf);
                    apply_key_value(&mut config, &array_section, &array_key, &array_buf, values);
                }
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line.trim_matches(['[', ']']).trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            if value.starts_with('[') && !value.contains(']') {
                in_array = true;
                array_section = section.clone();
                array_key = key.to_string();
                array_buf = value.to_string();
                continue;
            }
            let values = parse_string_array(value);
            apply_key_value(&mut config, &section, key, value, values);
        }

        config
    }

    pub fn includes_path(&self, rel_path: &str) -> bool {
        self.include_paths.is_empty()
            || self
                .include_paths
                .iter()
                .any(|prefix| rel_path.starts_with(prefix))
    }

    pub fn excludes_path(&self, rel_path: &str) -> bool {
        self.exclude_paths
            .iter()
            .any(|prefix| rel_path.starts_with(prefix))
    }

    pub fn ignores_name(&self, name: &str) -> bool {
        self.ignore_names.iter().any(|ignored| ignored == name)
    }
}

fn apply_key_value(
    config: &mut TesseraConfig,
    section: &str,
    key: &str,
    value: &str,
    values: Vec<String>,
) {
    match (section, key) {
        ("index", "max_file_bytes") => {
            config.max_file_bytes = value.trim().replace('_', "").parse().ok();
        }
        ("index", "db") => {
            config.db = Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .replace('\\', "/"),
            );
        }
        ("index", "watch_debounce_ms") => {
            config.watch_debounce_ms = value.trim().replace('_', "").parse().ok();
        }
        ("index", "watch_poll_ms") => {
            config.watch_poll_ms = value.trim().replace('_', "").parse().ok();
        }
        ("include", "paths") => config.include_paths = normalize_prefixes(values),
        ("exclude", "paths") => config.exclude_paths = normalize_prefixes(values),
        ("exclude", "globs") => config.exclude_globs = values,
        ("ignore", "extra") => config.ignore_names = values,
        _ => {}
    }
}

fn parse_string_array(value: &str) -> Vec<String> {
    let value = value.trim();
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return Vec::new();
    };
    inner
        .split(',')
        .filter_map(|part| {
            let trimmed = part.trim().trim_matches('"').trim_matches('\'').trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.replace('\\', "/"))
            }
        })
        .collect()
}

fn normalize_prefixes(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim_start_matches("./").replace('\\', "/"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_include_exclude_and_extra_ignores() {
        let config = TesseraConfig::parse_lossy(
            r#"
[include]
paths = ["src/", "lib"]

[exclude]
paths = ["src/generated/"]
globs = ["**/*.generated.ts"]

[ignore]
extra = ["fixtures", "tmp"]

[index]
max_file_bytes = 2_097_152
"#,
        );
        assert_eq!(config.max_file_bytes, Some(2_097_152));

        assert!(config.includes_path("src/app.ts"));
        assert!(!config.includes_path("tests/app.ts"));
        assert!(config.excludes_path("src/generated/app.ts"));
        assert!(config.ignores_name("fixtures"));
        assert!(config.ignores_name("tmp"));
        assert_eq!(config.exclude_globs, vec!["**/*.generated.ts"]);
    }

    #[test]
    fn parses_multiline_arrays_and_index_section() {
        let config = TesseraConfig::parse_lossy(
            r#"
[include]
paths = [
    "src/",
    "packages/api/", # internal API
]

[exclude]
paths = [
    "src/generated/",
    "tests/fixtures/",
]
globs = [
    "**/*.generated.ts",
    "**/*.min.js",
]

[ignore]
extra = [
    "fixtures",
    "tmp",
    "vendor",
]

[index]
max_file_bytes = 2_097_152
db = ".tessera/custom.db"
watch_debounce_ms = 350
watch_poll_ms = 500
"#,
        );

        assert_eq!(config.include_paths, vec!["src/", "packages/api/"]);
        assert_eq!(
            config.exclude_paths,
            vec!["src/generated/", "tests/fixtures/"]
        );
        assert_eq!(
            config.exclude_globs,
            vec!["**/*.generated.ts", "**/*.min.js"]
        );
        assert_eq!(config.ignore_names, vec!["fixtures", "tmp", "vendor"]);
        assert_eq!(config.max_file_bytes, Some(2_097_152));
        assert_eq!(config.db, Some(".tessera/custom.db".to_string()));
        assert_eq!(config.watch_debounce_ms, Some(350));
        assert_eq!(config.watch_poll_ms, Some(500));
    }

    #[test]
    fn parses_strings_with_hash_character() {
        let config = TesseraConfig::parse_lossy(
            r#"
[include]
paths = ["src/#special/"]

[ignore]
extra = ["tag#1"]
"#,
        );
        assert_eq!(config.include_paths, vec!["src/#special/"]);
        assert_eq!(config.ignore_names, vec!["tag#1"]);
    }

    #[test]
    fn parses_fallback_on_malformed_toml() {
        let config = TesseraConfig::parse_lossy(
            r#"
[broken_section
this is not valid toml = = =

[include]
paths = [
    "src/",
    "lib/"
]

[index]
db = "fallback.db"
watch_debounce_ms = 400
"#,
        );
        assert_eq!(config.include_paths, vec!["src/", "lib/"]);
        assert_eq!(config.db, Some("fallback.db".to_string()));
        assert_eq!(config.watch_debounce_ms, Some(400));
    }
}
