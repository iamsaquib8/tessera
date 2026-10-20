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
fn release_workflow_publishes_only_after_every_binary_succeeds() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    for (needle, why) in [
        ("gh release create \"$TAG\"", "release must be created before assets upload"),
        ("--draft", "release starts as a draft so releases/latest never points at an empty release"),
        ("needs: release", "binaries wait for the draft release"),
        ("needs: binaries", "publish waits for every binary job"),
        ("--draft=false", "publish job flips the draft only at the end"),
        ("SHA256SUMS", "checksums are published for installers"),
        ("attest-build-provenance", "artifacts carry provenance"),
        ("timeout-minutes: 30", "a hung runner cannot block a release for 24h"),
        ("x86_64-unknown-linux-musl", "static Linux binaries are built"),
        ("aarch64-unknown-linux-musl", "static arm64 Linux binaries are built"),
        ("macos-15-intel", "Intel macOS builds use a current runner label"),
        ("needs: publish", "package registries wait for a published release"),
    ] {
        assert!(workflow.contains(needle), "{why} (missing `{needle}`)");
    }
    assert!(!workflow.contains("macos-13"), "macos-13 runners can stay queued");
}

#[test]
fn npm_launcher_uses_platform_packages_without_install_scripts() {
    let npm: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("npm/package.json").unwrap()).unwrap();
    assert!(npm.get("scripts").is_none(), "no postinstall download");
    let optional = npm["optionalDependencies"].as_object().unwrap();
    assert_eq!(optional.len(), 7);
    for (name, version) in optional {
        let platform = name.trim_start_matches("tessera-codegraph-");
        let manifest = format!("npm/platforms/{platform}/package.json");
        let pkg: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
        assert_eq!(&pkg["version"], version, "{manifest}");
        assert_eq!(pkg["version"], npm["version"], "{manifest}");
    }
}

fn find_manifest_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key} = \"");
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|rest| rest.split('"').next())
    })
}
