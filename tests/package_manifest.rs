//! What `Cargo.toml` promises: no unsafe code, the lints CI enforces, a
//! size-tuned release profile, and a crates.io package that holds only what
//! `cargo install` needs.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> String {
    fs::read_to_string(root().join("Cargo.toml")).unwrap()
}

#[test]
fn the_crate_forbids_unsafe_code_and_denies_clippy_findings() {
    let text = manifest();
    assert!(text.contains("[lints.rust]\nunsafe_code = \"forbid\""));
    assert!(text.contains("[lints.clippy]\nall = { level = \"deny\", priority = -1 }"));
}

#[test]
fn the_release_profile_is_tuned_for_size() {
    assert!(manifest()
        .contains("[profile.release]\nlto = \"fat\"\ncodegen-units = 1\nstrip = \"symbols\"\n"));
}

#[test]
fn the_crates_io_package_ships_the_source_and_nothing_else() {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["package", "--list", "--allow-dirty", "--offline"])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let files: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        // Windows lists paths with backslashes.
        .map(|line| line.replace('\\', "/"))
        .collect();
    for file in &files {
        let top = file.split('/').next().unwrap();
        assert!(
            ["src", "packs"].contains(&top)
                || [
                    "Cargo.toml",
                    "Cargo.toml.orig",
                    "Cargo.lock",
                    "README.md",
                    "LICENSE",
                    "CHANGELOG.md",
                    ".cargo_vcs_info.json",
                ]
                .contains(&file.as_str()),
            "the package ships {file}"
        );
    }
    // Every file the source embeds is in the package.
    for needed in ["src/ui/page.html", "packs/express.scm", "src/lib.rs"] {
        assert!(files.iter().any(|f| f == needed), "missing {needed}");
    }
}
