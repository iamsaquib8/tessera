//! The docs site describes the latest release, not `main`, and says which
//! version a page describes.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use yaml_rust2::YamlLoader;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workflow(name: &str) -> yaml_rust2::Yaml {
    let text = fs::read_to_string(root().join(".github/workflows").join(name)).unwrap();
    YamlLoader::load_from_str(&text).unwrap().remove(0)
}

#[test]
fn pushing_to_main_does_not_publish_the_docs() {
    let pages = workflow("pages.yml");
    // `on:` is read as a boolean key by YAML 1.1 parsers; yaml-rust2 keeps the string.
    let triggers = &pages["on"];
    assert!(triggers["push"].is_badvalue(), "no push trigger");
    assert!(!triggers["workflow_call"].is_badvalue());
    assert!(!triggers["workflow_dispatch"].is_badvalue());
}

#[test]
fn a_stable_release_publishes_the_docs_after_every_channel_works() {
    let release = workflow("release.yml");
    let docs = &release["jobs"]["docs"];
    assert_eq!(docs["uses"].as_str(), Some("./.github/workflows/pages.yml"));
    let condition = docs["if"].as_str().unwrap();
    assert!(condition.contains("needs.smoke.result == 'success'"));
    assert!(condition.contains("!contains("), "pre-releases stay off");
    assert_eq!(docs["needs"].as_str(), Some("smoke"));
    assert_eq!(
        docs["with"]["tag"].as_str(),
        Some("${{ inputs.tag || github.ref_name }}")
    );
}

#[test]
fn the_site_is_built_from_a_release_tag_by_default() {
    let text = fs::read_to_string(root().join(".github/workflows/pages.yml")).unwrap();
    assert!(
        text.contains("gh release view"),
        "a manual run builds the latest release"
    );
    let pages = workflow("pages.yml");
    let checkout = pages["jobs"]["build"]["steps"]
        .as_vec()
        .unwrap()
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/checkout@"))
        })
        .and_then(|step| step["with"]["ref"].as_str())
        .expect("the checkout names a ref");
    assert!(checkout.contains("steps.ref.outputs.ref"), "{checkout}");
}

#[test]
fn every_page_says_which_version_it_describes() {
    let out = tempfile::TempDir::new().unwrap();
    let run = Command::new("node")
        .arg(root().join("website/scripts/sync-docs.mjs"))
        .env("TESSERA_DOCS_VERSION", "9.8.7")
        .env("TESSERA_DOCS_OUT", out.path())
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let mut checked = 0;
    for entry in fs::read_dir(out.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "md") {
            let page = fs::read_to_string(&path).unwrap();
            let frontmatter = page.split("---").nth(1).unwrap();
            assert!(
                frontmatter.contains("banner:") && frontmatter.contains("Tessera 9.8.7"),
                "{} has no version banner",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked >= 16, "only {checked} pages");
}

#[test]
fn without_a_version_the_banner_reads_the_crate_version() {
    let out = tempfile::TempDir::new().unwrap();
    let run = Command::new("node")
        .arg(root().join("website/scripts/sync-docs.mjs"))
        .env_remove("TESSERA_DOCS_VERSION")
        .env("TESSERA_DOCS_OUT", out.path())
        .output()
        .unwrap();
    assert!(run.status.success());
    let page = fs::read_to_string(out.path().join("quickstart.md")).unwrap();
    assert!(page.contains(&format!("Tessera {}", env!("CARGO_PKG_VERSION"))));
}
