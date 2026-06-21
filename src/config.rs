use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TesseraConfig {
    /// Only index paths with one of these relative path prefixes. Empty means all.
    pub include_paths: Vec<String>,
    /// Skip paths with one of these relative path prefixes.
    pub exclude_paths: Vec<String>,
    /// Extra directory or file names to skip during traversal.
    pub ignore_names: Vec<String>,
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
        let mut config = Self::default();
        let mut section = String::new();

        for raw_line in content.lines() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
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
            let values = parse_string_array(value.trim());
            match (section.as_str(), key) {
                ("include", "paths") => config.include_paths = normalize_prefixes(values),
                ("exclude", "paths") => config.exclude_paths = normalize_prefixes(values),
                ("ignore", "extra") => config.ignore_names = values,
                _ => {}
            }
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

[ignore]
extra = ["fixtures", "tmp"]
"#,
        );

        assert!(config.includes_path("src/app.ts"));
        assert!(!config.includes_path("tests/app.ts"));
        assert!(config.excludes_path("src/generated/app.ts"));
        assert!(config.ignores_name("fixtures"));
        assert!(config.ignores_name("tmp"));
    }
}
