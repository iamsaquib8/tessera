//! `deny.toml` and the jobs that run it: the supply-chain checks stay on, and
//! the dependency tree cannot gain an HTTP client without a failing build.

use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn workflow(path: &str) -> Yaml {
    YamlLoader::load_from_str(&read(path)).unwrap().remove(0)
}

fn deny() -> toml::Table {
    read("deny.toml").parse().unwrap()
}

#[test]
fn advisories_fail_on_vulnerabilities_and_yanked_crates() {
    let deny = deny();
    let advisories = deny["advisories"].as_table().unwrap();
    assert_eq!(advisories["yanked"].as_str(), Some("deny"));
    // A vulnerability fails by default; an ignore list needs a reason, so
    // it starts empty.
    assert!(advisories["ignore"].as_array().unwrap().is_empty());
}

#[test]
fn only_crates_io_is_a_source() {
    let deny = deny();
    let sources = deny["sources"].as_table().unwrap();
    assert_eq!(sources["unknown-registry"].as_str(), Some("deny"));
    assert_eq!(sources["unknown-git"].as_str(), Some("deny"));
    assert!(sources["allow-git"].as_array().unwrap().is_empty());
    let registries: Vec<_> = sources["allow-registry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(registries, ["https://github.com/rust-lang/crates.io-index"]);
}

#[test]
fn http_clients_are_banned_to_keep_the_no_network_promise() {
    let deny = deny();
    let banned: Vec<_> = deny["bans"]["deny"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["crate"].as_str().unwrap().to_string())
        .collect();
    for client in [
        "reqwest",
        "ureq",
        "hyper",
        "isahc",
        "curl",
        "attohttpc",
        "surf",
    ] {
        assert!(banned.iter().any(|b| b == client), "{client} is not banned");
    }
    assert_eq!(deny["bans"]["multiple-versions"].as_str(), Some("warn"));
    // The dependency tree today has none of them.
    let lock = read("Cargo.lock");
    for client in banned {
        assert!(
            !lock.contains(&format!("name = \"{client}\"")),
            "{client} is in Cargo.lock"
        );
    }
}

#[test]
fn every_license_in_the_tree_is_allowed() {
    let deny = deny();
    let allowed: Vec<&str> = deny["licenses"]["allow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    for license in ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "ISC"] {
        assert!(allowed.contains(&license), "{license}");
    }
    // Copyleft needs a review before it is allowed.
    assert!(!allowed.iter().any(|l| l.contains("GPL")));
}

#[test]
fn ci_runs_cargo_deny_and_a_weekly_run_checks_advisories() {
    let ci = workflow(".github/workflows/ci.yml");
    let job = &ci["jobs"]["deny"];
    let uses = job["steps"]
        .as_vec()
        .unwrap()
        .iter()
        .filter_map(|s| s["uses"].as_str())
        .any(|u| u.starts_with("EmbarkStudios/cargo-deny-action@"));
    assert!(uses, "the deny job runs cargo-deny-action");

    let weekly = workflow(".github/workflows/advisories.yml");
    assert!(
        weekly["on"]["schedule"][0]["cron"].as_str().is_some(),
        "a schedule"
    );
    assert!(!weekly["on"]["workflow_dispatch"].is_badvalue());
    let text = read(".github/workflows/advisories.yml");
    assert!(text.contains("advisories"));
}
