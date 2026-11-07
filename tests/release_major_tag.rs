//! The release workflow's `major-tag` job: `@v1` follows the newest stable 1.x
//! release, never a pre-release, and never moves backwards. The job's shell is
//! run here against a local repository and a bare `origin`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workflow() -> yaml_rust2::Yaml {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
    )
    .unwrap();
    yaml_rust2::YamlLoader::load_from_str(&text)
        .unwrap()
        .remove(0)
}

/// The shell Actions runs `run:` steps in. On Windows a bare `bash` is the WSL
/// launcher, so use Git's.
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

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// A working copy with a bare `origin`, like the job's checkout.
struct Repo {
    _temp: tempfile::TempDir,
    origin: PathBuf,
    work: PathBuf,
}

impl Repo {
    fn new() -> Repo {
        let temp = tempfile::TempDir::new().unwrap();
        let origin = temp.path().join("origin.git");
        let work = temp.path().join("work");
        fs::create_dir_all(&origin).unwrap();
        fs::create_dir_all(&work).unwrap();
        git(&origin, &["init", "-q", "--bare", "-b", "main"]);
        git(&work, &["init", "-q", "-b", "main"]);
        git(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        Repo {
            _temp: temp,
            origin,
            work,
        }
    }

    /// A new commit, tagged `tag` when given; returns its id.
    fn release(&self, tag: Option<&str>) -> String {
        let n = git(&self.work, &["rev-list", "--count", "--all"]);
        fs::write(self.work.join("file"), format!("{n}\n")).unwrap();
        git(&self.work, &["add", "."]);
        git(&self.work, &["commit", "-q", "-m", &format!("commit {n}")]);
        let id = git(&self.work, &["rev-parse", "HEAD"]);
        if let Some(tag) = tag {
            git(&self.work, &["tag", tag]);
        }
        git(&self.work, &["push", "-q", "origin", "main", "--tags"]);
        id
    }

    fn origin_tag(&self, tag: &str) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("refs/tags/{tag}^{{commit}}"))
            .current_dir(&self.origin)
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_string())
    }

    /// Runs the step for `tag`, as the job does; returns the combined output.
    fn run(&self, tag: &str) -> (bool, String) {
        let docs = workflow();
        let steps = docs["jobs"]["major-tag"]["steps"].as_vec().unwrap();
        let script = steps
            .iter()
            .find(|s| s["id"].as_str() == Some("move"))
            .and_then(|s| s["run"].as_str())
            .expect("a step with id `move`");
        let out = bash()
            .args(["-c", script])
            .current_dir(&self.work)
            .env("TAG", tag)
            .output()
            .unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }
}

#[test]
fn the_major_tag_job_runs_last_with_the_only_write_permission_it_needs() {
    let docs = workflow();
    let job = &docs["jobs"]["major-tag"];
    assert!(!job.is_badvalue(), "a `major-tag` job");
    assert_eq!(job["needs"].as_str(), Some("smoke"));
    let condition = job["if"].as_str().unwrap();
    assert!(
        condition.contains("needs.smoke.result == 'success'"),
        "{condition}"
    );
    assert!(condition.contains("contains("), "pre-releases skip it");
    assert_eq!(job["permissions"]["contents"].as_str(), Some("write"));
    // Nothing else is needed to move a tag.
    assert_eq!(job["permissions"].as_hash().unwrap().len(), 1);
    let checkout = &job["steps"][0];
    assert_eq!(checkout["with"]["fetch-depth"].as_i64(), Some(0));
}

#[test]
fn a_stable_release_creates_and_then_moves_the_major_tag() {
    let repo = Repo::new();
    let first = repo.release(Some("v1.2.0"));
    let (ok, out) = repo.run("v1.2.0");
    assert!(ok, "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(first));

    let second = repo.release(Some("v1.3.0"));
    let (ok, out) = repo.run("v1.3.0");
    assert!(ok, "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(second.clone()));

    // A second run for the same tag changes nothing.
    let (ok, out) = repo.run("v1.3.0");
    assert!(ok, "{out}");
    assert!(out.contains("already points"), "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(second));
}

#[test]
fn a_pre_release_leaves_the_major_tag_alone() {
    let repo = Repo::new();
    let stable = repo.release(Some("v1.2.0"));
    assert!(repo.run("v1.2.0").0);
    repo.release(Some("v1.3.0-rc.1"));
    let (ok, out) = repo.run("v1.3.0-rc.1");
    assert!(ok, "{out}");
    assert!(out.contains("pre-release"), "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(stable));

    // Not even the first stable release of a major version is made from one.
    let repo = Repo::new();
    repo.release(Some("v2.0.0-rc.1"));
    assert!(repo.run("v2.0.0-rc.1").0);
    assert_eq!(repo.origin_tag("v2"), None);
}

#[test]
fn re_running_an_old_release_never_moves_the_tag_backwards() {
    let repo = Repo::new();
    repo.release(Some("v1.2.0"));
    let newest = repo.release(Some("v1.3.0"));
    assert!(repo.run("v1.3.0").0);
    let (ok, out) = repo.run("v1.2.0");
    assert!(ok, "{out}");
    assert!(out.contains("newer"), "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(newest.clone()));

    // A patch release cut from older code than the tag's commit is skipped too.
    git(&repo.work, &["checkout", "-q", "-b", "hotfix", "v1.2.0"]);
    repo.release(None);
    git(&repo.work, &["tag", "v1.2.1"]);
    git(&repo.work, &["push", "-q", "origin", "--tags"]);
    let (ok, out) = repo.run("v1.2.1");
    assert!(ok, "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(newest));
}

#[test]
fn another_major_version_gets_its_own_tag() {
    let repo = Repo::new();
    let one = repo.release(Some("v1.9.0"));
    assert!(repo.run("v1.9.0").0);
    let two = repo.release(Some("v2.0.0"));
    assert!(repo.run("v2.0.0").0);
    assert_eq!(repo.origin_tag("v1"), Some(one));
    assert_eq!(repo.origin_tag("v2"), Some(two));
}

#[test]
fn a_tag_that_is_not_a_version_fails_the_job() {
    let repo = Repo::new();
    repo.release(Some("nightly"));
    let (ok, out) = repo.run("nightly");
    assert!(!ok, "{out}");
    assert!(out.contains("not a vMAJOR.MINOR.PATCH tag"), "{out}");
}

#[test]
fn a_release_that_does_not_descend_from_the_tag_leaves_it_alone() {
    let repo = Repo::new();
    repo.release(Some("v1.2.0"));
    let current = repo.release(Some("v1.3.0"));
    assert!(repo.run("v1.3.0").0);
    // The higher version, but cut from code that lacks what v1 already has.
    git(&repo.work, &["checkout", "-q", "-b", "other", "v1.2.0"]);
    repo.release(None);
    git(&repo.work, &["tag", "v1.4.0"]);
    git(&repo.work, &["push", "-q", "origin", "--tags"]);
    let (ok, out) = repo.run("v1.4.0");
    assert!(ok, "{out}");
    assert!(out.contains("not an ancestor"), "{out}");
    assert_eq!(repo.origin_tag("v1"), Some(current));
}
