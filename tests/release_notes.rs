//! The release workflow takes the GitHub release body from the tag's
//! CHANGELOG.md section instead of generating it, and stops when there is
//! none. The extraction is the workflow's own shell, run here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workflow() -> yaml_rust2::Yaml {
    let text = fs::read_to_string(manifest().join(".github/workflows/release.yml")).unwrap();
    yaml_rust2::YamlLoader::load_from_str(&text)
        .unwrap()
        .remove(0)
}

/// The shell Actions runs `run:` steps in (Git's, on Windows).
fn bash() -> Command {
    if !cfg!(windows) {
        return Command::new("bash");
    }
    let out = Command::new("git").arg("--exec-path").output().unwrap();
    let exec = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    let bash = exec
        .ancestors()
        .nth(3)
        .map(|git| git.join("bin").join("bash.exe"))
        .filter(|bash| bash.is_file())
        .unwrap_or_else(|| panic!("no Git Bash next to {}", exec.display()));
    Command::new(bash)
}

struct Notes {
    ok: bool,
    output: String,
    /// What the step wrote to the file it names in `GITHUB_OUTPUT`.
    body: Option<String>,
}

/// Runs the `notes` step for `tag` in `dir`, which holds a CHANGELOG.md.
fn extract(dir: &Path, tag: &str) -> Notes {
    let docs = workflow();
    let steps = docs["jobs"]["release"]["steps"].as_vec().unwrap();
    let script = steps
        .iter()
        .find(|s| s["id"].as_str() == Some("notes"))
        .and_then(|s| s["run"].as_str())
        .expect("a step with id `notes` in the release job");
    let temp = tempfile::TempDir::new().unwrap();
    let github_output = temp.path().join("output");
    let out = bash()
        .args(["-c", script])
        .current_dir(dir)
        .env("TAG", tag)
        .env(
            "PRERELEASE",
            if tag.contains('-') { "true" } else { "false" },
        )
        .env("RUNNER_TEMP", temp.path())
        .env("GITHUB_OUTPUT", &github_output)
        .output()
        .unwrap();
    let body = fs::read_to_string(&github_output)
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("file=").map(str::to_string))
        })
        .and_then(|file| fs::read_to_string(file).ok());
    Notes {
        ok: out.status.success(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        body,
    }
}

fn changelog_dir(text: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("CHANGELOG.md"), text).unwrap();
    dir
}

#[test]
fn the_release_body_is_the_changelog_section_of_the_tag() {
    let notes = extract(&manifest(), "v1.2.0");
    assert!(notes.ok, "{}", notes.output);
    let body = notes.body.expect("a notes file");

    // Exactly the lines between `## 1.2.0 - …` and the next `## ` heading.
    let changelog = fs::read_to_string(manifest().join("CHANGELOG.md")).unwrap();
    let section = changelog
        .split("\n## 1.2.0 - ")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .and_then(|rest| rest.split_once('\n'))
        .map(|(_, rest)| rest.trim())
        .unwrap();
    assert!(!section.is_empty());
    assert_eq!(body.trim_end(), section);
    assert!(body.contains("### Added"), "{body}");
    assert!(
        !body.contains("## 1.1.0") && !body.contains("## Unreleased"),
        "only this release"
    );
}

#[test]
fn the_current_version_has_a_changelog_section_or_an_unreleased_one() {
    let cargo = fs::read_to_string(manifest().join("Cargo.toml")).unwrap();
    let changelog = fs::read_to_string(manifest().join("CHANGELOG.md")).unwrap();
    let version = env!("CARGO_PKG_VERSION");
    assert!(cargo.contains(&format!("version = \"{version}\"")));
    let has_section = changelog
        .lines()
        .any(|l| l == format!("## {version}") || l.starts_with(&format!("## {version} ")));
    assert!(
        has_section || changelog.lines().any(|l| l == "## Unreleased"),
        "CHANGELOG.md needs a `## {version}` or `## Unreleased` section"
    );
}

#[test]
fn a_tag_without_a_section_fails_the_job_and_says_why() {
    let dir = changelog_dir(
        "# Changelog\n\n## Unreleased\n\n- Something.\n\n## 1.0.0 - 2026-01-01\n\n- Old.\n",
    );
    let notes = extract(dir.path(), "v1.1.0");
    assert!(!notes.ok, "{}", notes.output);
    assert!(
        notes.output.contains("no non-empty '## 1.1.0' section"),
        "{}",
        notes.output
    );
    // A heading with nothing under it is as missing as no heading.
    let dir = changelog_dir(
        "# Changelog\n\n## 1.1.0 - 2026-01-02\n\n\n## 1.0.0 - 2026-01-01\n\n- Old.\n",
    );
    assert!(!extract(dir.path(), "v1.1.0").ok);
}

#[test]
fn version_prefixes_do_not_match_each_other() {
    let dir = changelog_dir(
        "# Changelog\n\n## 1.2.10 - 2026-03-01\n\n- Ten.\n\n## 1.2.1 - 2026-02-01\n\n- One.\n\n### Fixed\n\n- A fix.\n\n## 1.2.0\n\n- Zero.\n",
    );
    assert_eq!(
        extract(dir.path(), "v1.2.1").body.unwrap(),
        "- One.\n\n### Fixed\n\n- A fix.\n"
    );
    assert_eq!(extract(dir.path(), "v1.2.0").body.unwrap(), "- Zero.\n");
    assert_eq!(extract(dir.path(), "v1.2.10").body.unwrap(), "- Ten.\n");
    assert!(!extract(dir.path(), "v1.2.2").ok);
}

#[test]
fn a_release_candidate_uses_its_own_section_or_unreleased() {
    let dir = changelog_dir(
        "# Changelog\n\n## Unreleased\n\n- Coming.\n\n## 1.3.0-rc.1 - 2026-04-01\n\n- Candidate.\n\n## 1.2.0\n\n- Zero.\n",
    );
    assert_eq!(
        extract(dir.path(), "v1.3.0-rc.1").body.unwrap(),
        "- Candidate.\n"
    );
    assert_eq!(
        extract(dir.path(), "v1.3.0-rc.2").body.unwrap(),
        "- Coming.\n"
    );
    // A stable tag never falls back to Unreleased.
    assert!(!extract(dir.path(), "v1.3.0").ok);
}

#[test]
fn a_release_candidate_can_use_the_changelog_fragments() {
    let dir = changelog_dir("# Changelog\n\n## 1.2.0\n\n- Zero.\n");
    fs::create_dir_all(dir.path().join("scripts")).unwrap();
    fs::copy(
        manifest().join("scripts/changelog.js"),
        dir.path().join("scripts/changelog.js"),
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("changes")).unwrap();
    fs::write(dir.path().join("changes/a.fixed.md"), "Fix A.\n").unwrap();
    fs::write(dir.path().join("changes/b.added.md"), "Add B.\n").unwrap();
    let notes = extract(dir.path(), "v1.3.0-rc.1").body.unwrap();
    assert_eq!(notes, "### Added\n\n- Add B.\n\n### Fixed\n\n- Fix A.\n");
    // A stable tag still needs its own section.
    assert!(!extract(dir.path(), "v1.3.0").ok);
}

#[test]
fn the_release_is_created_from_the_notes_file() {
    let workflow = fs::read_to_string(manifest().join(".github/workflows/release.yml")).unwrap();
    assert!(!workflow.contains("--generate-notes"));
    assert!(workflow.contains("--notes-file \"$NOTES_FILE\""));
    assert!(workflow.contains("NOTES_FILE: ${{ steps.notes.outputs.file }}"));
}
