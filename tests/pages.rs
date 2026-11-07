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
    let mut stack = vec![out.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
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
    }
    assert!(checked >= 26, "only {checked} pages");
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

#[test]
fn the_docs_site_job_builds_the_site_and_checks_its_links() {
    let ci = workflow("ci.yml");
    let steps = ci["jobs"]["docs-site"]["steps"].as_vec().unwrap();
    let run = steps
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(run.contains("npm run build"), "{run}");
    assert!(run.contains("npm run check-links"), "{run}");
    let scripts = fs::read_to_string(root().join("website/package.json")).unwrap();
    assert!(scripts.contains("scripts/check-links.mjs"));
}

#[test]
fn a_dead_relative_link_fails_the_link_check() {
    // A built site with one good page and one page linking to nothing.
    let dist = tempfile::TempDir::new().unwrap();
    let page = |links: &str| {
        format!(
            "<!doctype html><html><body><main><h1 id=\"top\">T</h1>{links}</main></body></html>"
        )
    };
    fs::create_dir_all(dist.path().join("a")).unwrap();
    fs::create_dir_all(dist.path().join("b")).unwrap();
    fs::write(
        dist.path().join("a/index.html"),
        page("<a href=\"/tessera/b/#top\">b</a>"),
    )
    .unwrap();
    fs::write(dist.path().join("b/index.html"), page("")).unwrap();
    let check = |dist: &std::path::Path| {
        Command::new("node")
            .arg(root().join("website/scripts/check-links.mjs"))
            .env("TESSERA_DOCS_DIST", dist)
            .output()
            .unwrap()
    };
    let good = check(dist.path());
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    fs::write(
        dist.path().join("b/index.html"),
        page("<a href=\"../missing/\">x</a><a href=\"../a/#nope\">y</a>"),
    )
    .unwrap();
    let bad = check(dist.path());
    assert_eq!(bad.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(stderr.contains("../missing/"), "{stderr}");
    assert!(stderr.contains("#nope"), "{stderr}");
}
