use std::fs;

#[test]
fn release_package_versions_stay_aligned() {
    let cargo_toml = fs::read_to_string("Cargo.toml").unwrap();
    let cargo_version = find_manifest_value(&cargo_toml, "version").unwrap();

    let cargo_lock = fs::read_to_string("Cargo.lock").unwrap();
    let lock_version = cargo_lock
        .split("[[package]]")
        .find(|section| section.contains("name = \"tessera-codegraph\""))
        .and_then(|section| find_manifest_value(section, "version"))
        .unwrap();

    let npm_package = fs::read_to_string("npm/package.json").unwrap();
    let npm_json: serde_json::Value = serde_json::from_str(&npm_package).unwrap();
    let npm_version = npm_json["version"].as_str().unwrap();

    assert_eq!(cargo_version, lock_version);
    assert_eq!(cargo_version, npm_version);
}

#[test]
fn release_workflow_creates_release_before_publishing_assets() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();

    assert!(
        workflow.contains("release:"),
        "missing release creation job"
    );
    assert!(
        workflow.contains("gh release create \"$TAG\""),
        "release workflow must create the GitHub Release before uploading assets"
    );
    assert!(
        workflow.contains("needs: release"),
        "binary matrix must wait for the release creation job"
    );
    assert!(
        workflow.contains("needs: binaries"),
        "npm publish must wait for release binaries"
    );
    assert!(
        workflow.contains("macos-15-intel"),
        "x86_64 macOS builds should use a current Intel runner label"
    );
    assert!(
        !workflow.contains("macos-13"),
        "macos-13 runners can stay queued and block npm publishing"
    );
}

fn find_manifest_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key} = \"");
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|rest| rest.split('"').next())
    })
}
