//! Supply-chain rules for the workflows and the composite Action: every
//! third-party action is pinned to a commit, and each job gets only the
//! permissions it needs.

use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workflows() -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir(root().join(".github/workflows"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "yml"))
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read_to_string(&path).unwrap(),
            )
        })
        .collect();
    found.sort();
    assert!(found.len() >= 6, "the workflows were found");
    found
}

fn yaml(text: &str) -> Yaml {
    YamlLoader::load_from_str(text).unwrap().remove(0)
}

#[test]
fn third_party_actions_are_pinned_to_a_commit_with_a_version_comment() {
    let mut files = workflows();
    files.push((
        "action/action.yml".to_string(),
        fs::read_to_string(root().join("action/action.yml")).unwrap(),
    ));
    let mut checked = 0;
    for (name, text) in files {
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start().trim_start_matches("- ");
            let Some(reference) = trimmed.strip_prefix("uses:") else {
                continue;
            };
            let (reference, comment) = match reference.split_once('#') {
                Some((reference, comment)) => (reference.trim(), Some(comment.trim())),
                None => (reference.trim(), None),
            };
            let place = format!("{name}:{}: {line}", number + 1);
            // This repository's own actions move with the repository.
            if reference.starts_with("./") || reference.starts_with("iamsaquib8/tessera/") {
                continue;
            }
            let (_, version) = reference
                .split_once('@')
                .unwrap_or_else(|| panic!("no version: {place}"));
            assert!(
                version.len() == 40 && version.chars().all(|c| c.is_ascii_hexdigit()),
                "not a full commit SHA: {place}"
            );
            assert!(
                comment.is_some_and(|c| !c.is_empty()),
                "no version comment: {place}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 40, "only {checked} pinned references found");
}

#[test]
fn rust_toolchain_steps_name_their_toolchain() {
    // A pinned commit has no branch name to read the toolchain from.
    for (name, text) in workflows() {
        let docs = yaml(&text);
        for (job, body) in docs["jobs"].as_hash().unwrap() {
            for step in body["steps"].as_vec().into_iter().flatten() {
                if step["uses"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("dtolnay/rust-toolchain@"))
                {
                    assert!(
                        step["with"]["toolchain"].as_str().is_some(),
                        "{name} {}: a `toolchain:` input",
                        job.as_str().unwrap()
                    );
                }
            }
        }
    }
}

fn scopes_with_write(permissions: &Yaml) -> Vec<String> {
    let mut scopes: Vec<String> = permissions
        .as_hash()
        .into_iter()
        .flatten()
        .filter(|(_, level)| level.as_str() == Some("write"))
        .map(|(scope, _)| scope.as_str().unwrap().to_string())
        .collect();
    scopes.sort();
    scopes
}

/// The write permissions each job may have: `(workflow, job, scopes)`. A job
/// not listed has none.
const WRITES: &[(&str, &str, &[&str])] = &[
    ("release.yml", "release", &["contents"]),
    ("release.yml", "binaries", &["contents"]),
    (
        "release.yml",
        "publish",
        &["attestations", "contents", "id-token"],
    ),
    ("release.yml", "npm", &["id-token"]),
    ("release.yml", "docker", &["packages"]),
    ("release.yml", "major-tag", &["contents"]),
    ("docker.yml", "image", &["packages"]),
    ("pages.yml", "deploy", &["id-token", "pages"]),
    ("tessera.yml", "report", &["pull-requests"]),
];

#[test]
fn every_workflow_reads_by_default_and_jobs_ask_for_their_own_writes() {
    for (name, text) in workflows() {
        let docs = yaml(&text);
        let top = &docs["permissions"];
        let hash = top
            .as_hash()
            .unwrap_or_else(|| panic!("{name}: a top-level `permissions:`"));
        assert_eq!(
            (hash.len(), top["contents"].as_str()),
            (1, Some("read")),
            "{name}: top level is `contents: read` and nothing else"
        );
        for (job, body) in docs["jobs"].as_hash().unwrap() {
            let job = job.as_str().unwrap();
            let expected: Vec<&str> = WRITES
                .iter()
                .find(|(file, j, _)| *file == name && *j == job)
                .map(|(_, _, scopes)| scopes.to_vec())
                .unwrap_or_default();
            assert_eq!(
                scopes_with_write(&body["permissions"]),
                expected,
                "{name} job `{job}`"
            );
        }
    }
}

#[test]
fn jobs_that_never_publish_cannot_write_to_the_repository() {
    let docs = yaml(&fs::read_to_string(root().join(".github/workflows/release.yml")).unwrap());
    for job in ["crates", "homebrew", "smoke"] {
        let permissions = &docs["jobs"][job]["permissions"];
        assert!(
            permissions.is_badvalue() || scopes_with_write(permissions).is_empty(),
            "{job}"
        );
    }
}

/// Jobs whose checkout keeps the token: `major-tag` pushes a tag, and `report`
/// runs the Action, which fetches the base revision of a shallow checkout.
const KEEPS_TOKEN: &[(&str, &str)] = &[("release.yml", "major-tag"), ("tessera.yml", "report")];

#[test]
fn checkouts_do_not_keep_the_token_unless_the_job_needs_it() {
    for (name, text) in workflows() {
        let docs = yaml(&text);
        for (job, body) in docs["jobs"].as_hash().unwrap() {
            let job = job.as_str().unwrap();
            for step in body["steps"].as_vec().into_iter().flatten() {
                let is_checkout = step["uses"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("actions/checkout@"));
                if is_checkout && !KEEPS_TOKEN.contains(&(name.as_str(), job)) {
                    assert_eq!(
                        step["with"]["persist-credentials"].as_bool(),
                        Some(false),
                        "{name} job `{job}`: persist-credentials: false"
                    );
                }
            }
        }
    }
}
