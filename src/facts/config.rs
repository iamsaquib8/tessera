//! Configuration keys: the files that set them and the code that reads
//! them. Keys are kept, values never are.

use std::sync::LazyLock;

use regex::Regex;

use super::{kind, IndexedFact};

/// `config spring.datasource.url` for `SPRING_DATASOURCE_URL`,
/// `spring.datasource.url` and `spring.data-source.url` alike: Spring's
/// relaxed binding, which also fits plain environment variables.
pub fn key(name: &str) -> String {
    let normal: String = name
        .chars()
        .filter(|c| *c != '-')
        .map(|c| {
            if c == '_' {
                '.'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    format!("config {normal}")
}

fn set(name: &str, line: usize, rule: &'static str) -> IndexedFact {
    // The evidence names the key only: values can be secrets.
    IndexedFact::new(
        kind::CONFIG_SET,
        key(name),
        name.to_string(),
        line,
        rule,
        name,
    )
}

fn read(name: &str, line: usize, rule: &'static str, evidence: &str) -> IndexedFact {
    IndexedFact::new(
        kind::CONFIG_READ,
        key(name),
        name.to_string(),
        line,
        rule,
        evidence,
    )
}

static ENV_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_.]*)\s*=").expect("valid regex")
});
static DOCKER_ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*ENV\s+(.*)$").expect("valid regex"));
static DOCKER_PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([A-Za-z_][A-Za-z0-9_]*)=(?:"[^"]*"|'[^']*'|\S*)"#).expect("valid regex")
});
static COMPOSE_LIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*-\s*['"]?([A-Za-z_][A-Za-z0-9_.]*)(?:=|['"]?\s*$)"#).expect("valid regex")
});
static YAML_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(\s*)['"]?([A-Za-z0-9_.\-]+)['"]?\s*:(?:\s+(.*?))?\s*$"#).expect("valid regex")
});
static PROPERTY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([A-Za-z0-9_.\-\[\]]+)\s*[=:]\s*(.*?)\s*$").expect("valid regex")
});
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z0-9_.\-]+)(?::[^}]*)?\}").expect("valid regex"));

/// Keys a configuration file sets, and the placeholders it reads
/// (`url: ${DATABASE_URL}`).
pub fn file(path: &str, content: &str) -> Vec<IndexedFact> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let mut out = Vec::new();
    if name.contains("dockerfile") {
        for (index, line) in content.lines().enumerate() {
            let Some(rest) = DOCKER_ENV.captures(line) else {
                continue;
            };
            let rest = &rest[1];
            if rest.contains('=') {
                for pair in DOCKER_PAIR.captures_iter(rest) {
                    out.push(set(&pair[1], index + 1, "config.dockerfile"));
                }
            } else if let Some(first) = rest.split_whitespace().next() {
                out.push(set(first, index + 1, "config.dockerfile"));
            }
        }
    } else if name.ends_with(".yml") || name.ends_with(".yaml") {
        if name.starts_with("docker-compose") || name.starts_with("compose") {
            compose(content, &mut out);
        } else {
            spring_yaml(content, &mut out);
        }
    } else if name.ends_with(".properties") {
        for (index, line) in content.lines().enumerate() {
            if line.trim_start().starts_with(['#', '!']) {
                continue;
            }
            if let Some(c) = PROPERTY.captures(line) {
                out.push(set(&c[1], index + 1, "config.properties"));
                placeholders(&c[2], index + 1, &mut out);
            }
        }
    } else {
        for (index, line) in content.lines().enumerate() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if let Some(c) = ENV_LINE.captures(line) {
                out.push(set(&c[1], index + 1, "config.env"));
            }
        }
    }
    out
}

fn placeholders(value: &str, line: usize, out: &mut Vec<IndexedFact>) {
    for c in PLACEHOLDER.captures_iter(value) {
        // Only the placeholder: the rest of the line may hold a value.
        let shown = format!("${{{}}}", &c[1]);
        out.push(read(&c[1], line, "config.placeholder", &shown));
    }
}

/// `environment:` under a Compose service, as a list (`- KEY=value`) or a
/// map (`KEY: value`).
fn compose(content: &str, out: &mut Vec<IndexedFact>) {
    let mut block: Option<usize> = None;
    for (index, line) in content.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(start) = block {
            if indent <= start {
                block = None;
            } else if let Some(c) = COMPOSE_LIST.captures(line) {
                out.push(set(&c[1], index + 1, "config.compose"));
                continue;
            } else if let Some(c) = YAML_KEY.captures(line) {
                out.push(set(&c[2], index + 1, "config.compose"));
                continue;
            }
        }
        if trimmed == "environment:" {
            block = Some(indent);
        }
    }
}

/// Spring `application.yml`: nested keys flattened (`spring.datasource.url`),
/// and `${…}` placeholders in values read.
fn spring_yaml(content: &str, out: &mut Vec<IndexedFact>) {
    let mut path: Vec<(usize, String)> = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            if trimmed == "---" {
                path.clear();
            }
            continue;
        }
        let Some(c) = YAML_KEY.captures(line) else {
            continue;
        };
        let indent = c[1].len();
        while path.last().is_some_and(|(depth, _)| *depth >= indent) {
            path.pop();
        }
        path.push((indent, c[2].to_string()));
        let value = c.get(3).map_or("", |m| m.as_str());
        if !value.is_empty() && !value.starts_with('#') {
            let full: Vec<&str> = path.iter().map(|(_, k)| k.as_str()).collect();
            out.push(set(&full.join("."), index + 1, "config.spring"));
            placeholders(value, index + 1, out);
        }
    }
}

static READS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\bprocess\.env\.([A-Za-z_][A-Za-z0-9_]*)", "config.js.env"),
        (
            r#"\bprocess\.env\[\s*['"`]([^'"`]+)['"`]\s*\]"#,
            "config.js.env",
        ),
        (
            r"\bimport\.meta\.env\.([A-Za-z_][A-Za-z0-9_]*)",
            "config.js.env",
        ),
        (r#"\bDeno\.env\.get\(\s*['"]([^'"]+)['"]"#, "config.js.env"),
        (
            r#"\bos\.(?:environ\.get|getenv)\(\s*['"]([^'"]+)['"]"#,
            "config.python.env",
        ),
        (
            r#"\b(?:os\.)?environ\[\s*['"]([^'"]+)['"]\s*\]"#,
            "config.python.env",
        ),
        (
            r#"\bos\.(?:Getenv|LookupEnv)\(\s*"([^"]+)""#,
            "config.go.env",
        ),
        (r#"\bviper\.Get\w*\(\s*"([^"]+)""#, "config.go.viper"),
        (r#"\bSystem\.getenv\(\s*"([^"]+)""#, "config.java.env"),
        (r#"@Value\(\s*"\$\{([^}:]+)"#, "config.java.value"),
        (
            r#"\b(?:env|environment)\.getProperty\(\s*"([^"]+)""#,
            "config.java.property",
        ),
        (r#"\bENV\[\s*['"]([^'"]+)['"]\s*\]"#, "config.ruby.env"),
        (r#"\bENV\.fetch\(\s*['"]([^'"]+)['"]"#, "config.ruby.env"),
        (r#"\benv::var(?:_os)?\(\s*"([^"]+)""#, "config.rust.env"),
        (r#"\benv!\(\s*"([^"]+)""#, "config.rust.env"),
        (
            r#"\bGetEnvironmentVariable\(\s*"([^"]+)""#,
            "config.csharp.env",
        ),
    ]
    .into_iter()
    .map(|(pattern, rule)| (Regex::new(pattern).expect("valid regex"), rule))
    .collect()
});

static ANY_READ: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"env[.\[]|\benv!|env::|environ|Getenv|LookupEnv|viper\.Get|getenv|@Value|getProperty|\bENV\b|GetEnvironmentVariable")
        .expect("valid regex")
});

/// Keys source code reads from the environment or a config service.
pub fn reads(content: &str) -> Vec<IndexedFact> {
    // A cheap gate: every pattern below names one of these.
    if ![
        "env",
        "ENV",
        "@Value",
        "viper",
        "getProperty",
        "Environment",
    ]
    .iter()
    .any(|word| content.contains(word))
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if !ANY_READ.is_match(line) {
            continue;
        }
        for (regex, rule) in READS.iter() {
            for c in regex.captures_iter(line) {
                out.push(read(&c[1], index + 1, rule, line));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_bind_relaxed_and_values_stay_out() {
        assert_eq!(key("SPRING_DATASOURCE_URL"), key("spring.datasource.url"));
        assert_eq!(key("spring.data-source.url"), key("spring.datasource.url"));
        let facts = file(".env.example", "export API_KEY=sk-1\n# OFF=1\nA=b=c\n");
        let names: Vec<&str> = facts.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["API_KEY", "A"]);
        assert!(facts.iter().all(|f| !f.evidence.contains("sk-1")));
    }
}
