//! `.github/dependabot.yml` watches every place the repository takes a
//! dependency from, and keeps grammar bumps apart from the rest.

use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn config() -> Yaml {
    let text = fs::read_to_string(root().join(".github/dependabot.yml"))
        .expect(".github/dependabot.yml exists");
    YamlLoader::load_from_str(&text).unwrap().remove(0)
}

/// The update entries for `ecosystem`, as `(directory, entry)`.
fn updates(ecosystem: &str) -> Vec<(String, Yaml)> {
    config()["updates"]
        .as_vec()
        .unwrap()
        .iter()
        .filter(|u| u["package-ecosystem"].as_str() == Some(ecosystem))
        .map(|u| (u["directory"].as_str().unwrap().to_string(), u.clone()))
        .collect()
}

fn directories(ecosystem: &str) -> Vec<String> {
    let mut found: Vec<String> = updates(ecosystem).into_iter().map(|(d, _)| d).collect();
    found.sort();
    found
}

#[test]
fn every_ecosystem_is_watched_where_its_manifest_is() {
    assert_eq!(config()["version"].as_i64(), Some(2));
    assert_eq!(directories("cargo"), ["/"]);
    assert_eq!(directories("npm"), ["/npm", "/website"]);
    // `/` covers the workflows; a composite action's action.yml needs its own entry.
    assert_eq!(directories("github-actions"), ["/", "/action"]);
    assert_eq!(directories("docker"), ["/"]);
    for (ecosystem, manifests) in [
        ("cargo", &["Cargo.toml"][..]),
        ("npm", &["package.json"]),
        ("github-actions", &[]),
        ("docker", &["Dockerfile"]),
    ] {
        for (directory, _) in updates(ecosystem) {
            let dir = root().join(directory.trim_start_matches('/'));
            assert!(dir.is_dir(), "{ecosystem}: {directory}");
            for manifest in manifests {
                assert!(
                    dir.join(manifest).is_file(),
                    "{ecosystem}: {directory}/{manifest}"
                );
            }
        }
    }
    assert!(root().join("action/action.yml").is_file());
    assert!(root().join(".github/workflows").is_dir());
}

#[test]
fn updates_run_weekly_and_come_in_a_few_grouped_pull_requests() {
    let config = config();
    for update in config["updates"].as_vec().unwrap() {
        let name = update["package-ecosystem"].as_str().unwrap();
        assert_eq!(
            update["schedule"]["interval"].as_str(),
            Some("weekly"),
            "{name}"
        );
        assert!(
            update["open-pull-requests-limit"]
                .as_i64()
                .is_none_or(|n| n <= 10),
            "{name}"
        );
        // Every ecosystem has a group that collects its minor and patch updates.
        let groups = update["groups"]
            .as_hash()
            .unwrap_or_else(|| panic!("{name}: groups"));
        let collects = groups.values().any(|group| {
            group["update-types"].as_vec().is_some_and(|types| {
                types.iter().any(|t| t.as_str() == Some("minor"))
                    && types.iter().any(|t| t.as_str() == Some("patch"))
            })
        });
        assert!(collects, "{name}: a minor-and-patch group");
    }
}

#[test]
fn grammar_crates_get_their_own_group_ahead_of_the_catch_all() {
    let (_, cargo) = updates("cargo").remove(0);
    let groups: Vec<(String, &Yaml)> = cargo["groups"]
        .as_hash()
        .unwrap()
        .iter()
        .map(|(name, group)| (name.as_str().unwrap().to_string(), group))
        .collect();
    // A dependency joins the first group that matches it.
    let grammar = groups
        .iter()
        .position(|(_, g)| {
            g["patterns"]
                .as_vec()
                .is_some_and(|p| p.iter().any(|p| p.as_str() == Some("tree-sitter*")))
        })
        .expect("a group for tree-sitter*");
    let catch_all = groups
        .iter()
        .position(|(_, g)| {
            g["patterns"]
                .as_vec()
                .is_some_and(|p| p.iter().any(|p| p.as_str() == Some("*")))
        })
        .expect("a catch-all group");
    assert!(grammar < catch_all);
    // The grammar group takes every update type: major bumps change the snapshots.
    assert!(groups[grammar].1["update-types"].is_badvalue());

    let cargo_toml = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let grammars = cargo_toml
        .lines()
        .filter(|l| l.starts_with("tree-sitter"))
        .count();
    assert!(
        grammars >= 10,
        "the pattern matches the grammar crates in Cargo.toml"
    );
}
