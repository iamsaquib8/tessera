//! The repository's own contributor-facing files agree with each other and
//! with what CI runs.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn yaml(path: &str) -> Yaml {
    YamlLoader::load_from_str(&read(path)).unwrap().remove(0)
}

#[test]
fn issue_forms_only_apply_labels_the_repository_defines() {
    let labels = yaml(".github/labels.yml");
    let labels = labels.as_vec().expect("labels.yml is a list");
    let entries = read(".github/labels.yml")
        .lines()
        .filter(|line| line.starts_with("- "))
        .count();
    assert_eq!(
        labels.len(),
        entries,
        "an entry of labels.yml did not parse"
    );
    for label in labels {
        let color = label["color"].as_str().expect("a color");
        assert!(
            color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit()),
            "{label:?}"
        );
        assert!(!label["description"].as_str().unwrap_or("").is_empty());
    }
    let defined: BTreeSet<String> = labels
        .iter()
        .map(|label| label["name"].as_str().expect("a label name").to_string())
        .collect();
    let mut checked = 0;
    for entry in fs::read_dir(root().join(".github/ISSUE_TEMPLATE")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "config.yml" || !name.ends_with(".yml") {
            continue;
        }
        let form = yaml(&format!(".github/ISSUE_TEMPLATE/{name}"));
        let labels = form["labels"].as_vec().expect("a labels list");
        assert!(!labels.is_empty(), "{name} applies no label");
        for label in labels {
            let label = label.as_str().unwrap();
            assert!(
                defined.contains(label),
                "{name} applies `{label}`, which .github/labels.yml does not define"
            );
            checked += 1;
        }
    }
    assert!(checked >= 4, "found {checked} template labels");
}

#[test]
fn contributing_lists_every_test_file_and_ci_job() {
    let contributing = read("CONTRIBUTING.md");
    for entry in fs::read_dir(root().join("tests")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        if name.ends_with(".rs") {
            assert!(
                contributing.contains(&format!("`tests/{name}`")),
                "CONTRIBUTING.md does not list tests/{name}"
            );
        }
    }
    let ci = yaml(".github/workflows/ci.yml");
    for (job, _) in ci["jobs"].as_hash().unwrap() {
        let job = job.as_str().unwrap();
        assert!(
            contributing.contains(&format!("`{job}`")),
            "CONTRIBUTING.md does not describe the `{job}` CI job"
        );
    }
}

#[test]
fn the_pull_request_template_asks_for_what_ci_runs() {
    let template = read(".github/PULL_REQUEST_TEMPLATE.md");
    let ci = read(".github/workflows/ci.yml");
    for command in [
        "cargo fmt --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test --all-targets --all-features",
    ] {
        assert!(ci.contains(command), "ci.yml no longer runs `{command}`");
        assert!(template.contains(command), "template lacks `{command}`");
    }
}
