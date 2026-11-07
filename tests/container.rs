//! The container image is reproducible and runs as a numeric non-root user,
//! and pull requests that could break it build it.

use std::fs;

use yaml_rust2::YamlLoader;

#[test]
fn every_base_image_is_pinned_by_digest() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    let froms: Vec<&str> = dockerfile
        .lines()
        .filter(|line| line.starts_with("FROM "))
        .collect();
    assert_eq!(froms.len(), 2, "{froms:?}");
    for line in froms {
        let image = line.split_whitespace().nth(1).unwrap();
        let (_, digest) = image
            .split_once("@sha256:")
            .unwrap_or_else(|| panic!("`{line}` has no digest"));
        assert_eq!(digest.len(), 64, "{line}");
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()), "{line}");
    }
}

#[test]
fn the_image_runs_as_a_numeric_user() {
    let dockerfile = fs::read_to_string("Dockerfile").unwrap();
    assert!(dockerfile.contains("useradd -m -u 10001 tessera"));
    let user = dockerfile
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("USER "))
        .expect("a USER line");
    assert_eq!(user.trim(), "10001:10001");
}

#[test]
fn dependabot_keeps_the_digests_current() {
    let config = fs::read_to_string(".github/dependabot.yml").unwrap();
    let docs = YamlLoader::load_from_str(&config).unwrap();
    let docker = docs[0]["updates"]
        .as_vec()
        .unwrap()
        .iter()
        .find(|update| update["package-ecosystem"].as_str() == Some("docker"))
        .expect("a docker entry");
    assert_eq!(docker["directory"].as_str(), Some("/"));
}

#[test]
fn a_pull_request_that_touches_the_image_builds_it_without_pushing() {
    let text = fs::read_to_string(".github/workflows/image.yml").unwrap();
    let docs = YamlLoader::load_from_str(&text).unwrap();
    let paths: Vec<&str> = docs[0]["on"]["pull_request"]["paths"]
        .as_vec()
        .expect("a paths filter")
        .iter()
        .map(|path| path.as_str().unwrap())
        .collect();
    for path in [
        "Dockerfile",
        ".dockerignore",
        "src/**",
        "packs/**",
        "Cargo.toml",
        "Cargo.lock",
    ] {
        assert!(paths.contains(&path), "{path} should trigger the build");
    }
    assert!(text.contains("push: false"), "a pull request never pushes");
    assert!(text.contains("--version"), "the image is run once");
    assert!(text.contains("10001"), "the user is checked");
    assert!(!text.contains("docker/login-action"), "no registry login");
}
