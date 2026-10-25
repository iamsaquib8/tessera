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
        (
            "gh release create \"$TAG\"",
            "release must be created before assets upload",
        ),
        (
            "--draft",
            "release starts as a draft so releases/latest never points at an empty release",
        ),
        ("needs: release", "binaries wait for the draft release"),
        ("needs: binaries", "publish waits for every binary job"),
        (
            "--draft=false",
            "publish job flips the draft only at the end",
        ),
        ("SHA256SUMS", "checksums are published for installers"),
        ("attest-build-provenance", "artifacts carry provenance"),
        (
            "timeout-minutes: 30",
            "a hung runner cannot block a release for 24h",
        ),
        (
            "x86_64-unknown-linux-musl",
            "static Linux binaries are built",
        ),
        (
            "aarch64-unknown-linux-musl",
            "static arm64 Linux binaries are built",
        ),
        (
            "macos-15-intel",
            "Intel macOS builds use a current runner label",
        ),
        (
            "needs: publish",
            "package registries wait for a published release",
        ),
    ] {
        assert!(workflow.contains(needle), "{why} (missing `{needle}`)");
    }
    assert!(
        !workflow.contains("macos-13"),
        "macos-13 runners can stay queued"
    );
}

#[test]
fn npm_launcher_uses_platform_packages_without_install_scripts() {
    let npm: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("npm/package.json").unwrap()).unwrap();
    assert!(npm.get("scripts").is_none(), "no postinstall download");
    let optional = npm["optionalDependencies"].as_object().unwrap();
    assert_eq!(optional.len(), 5, "one static musl package per Linux arch");
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

#[cfg(unix)]
#[test]
fn homebrew_formula_renders_every_checksum() {
    let temp = tempfile::TempDir::new().unwrap();
    let sums = temp.path().join("SHA256SUMS");
    let targets = [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
    ];
    let lines: String = targets
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}  tessera-{t}.tar.gz\n", format!("{i}").repeat(64)))
        .collect();
    fs::write(&sums, lines).unwrap();
    let output = std::process::Command::new("sh")
        .arg("packaging/homebrew/render.sh")
        .arg("1.2.3")
        .arg(&sums)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let formula = String::from_utf8(output.stdout).unwrap();
    assert!(!formula.contains("{{"), "{formula}");
    assert!(formula.contains("version \"1.2.3\""));
    for i in 0..targets.len() {
        assert!(formula.contains(&format!("sha256 \"{}\"", format!("{i}").repeat(64))));
    }
}

#[test]
fn release_workflow_builds_the_tag_and_keeps_prereleases_off_latest() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    let checkouts = workflow.matches("uses: actions/checkout@v4").count();
    let pinned = workflow.matches("ref: ${{ env.TAG }}").count();
    assert_eq!(checkouts, pinned, "every checkout builds the tag, not main");
    for (needle, why) in [
        (
            "--prerelease",
            "an rc tag is marked as a prerelease on GitHub",
        ),
        ("dist_tag=next", "an rc goes to the npm `next` dist-tag"),
        (
            "uses: ./.github/workflows/docker.yml",
            "the image is built by the release run (GITHUB_TOKEN releases trigger nothing)",
        ),
        (
            "uses: ./.github/workflows/smoke.yml",
            "install smoke tests run after every channel is published",
        ),
    ] {
        assert!(workflow.contains(needle), "{why} (missing `{needle}`)");
    }
    for name in ["docker", "smoke"] {
        let called = fs::read_to_string(format!(".github/workflows/{name}.yml")).unwrap();
        assert!(called.contains("workflow_call"), "{name}.yml is reusable");
        assert!(
            !called.contains("types: [published]"),
            "{name}.yml must not rely on a release event that never fires"
        );
    }
}

#[test]
fn binstall_uses_static_linux_binaries() {
    let cargo_toml = fs::read_to_string("Cargo.toml").unwrap();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        let section = format!("[package.metadata.binstall.overrides.{target}]");
        let musl = target.replace("-gnu", "-musl");
        let override_block = cargo_toml.split(&section).nth(1).unwrap_or_default();
        assert!(
            override_block
                .split("\n[")
                .next()
                .unwrap_or_default()
                .contains(&format!("tessera-{musl}")),
            "{target} must download the {musl} build"
        );
    }
}

/// The image builds from the Dockerfile's copies alone, so every directory
/// whose files the source embeds (`include_str!`) must be copied in.
#[test]
fn the_docker_build_copies_every_embedded_directory() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dockerfile = std::fs::read_to_string(root.join("Dockerfile")).unwrap();
    let mut embedded = std::collections::BTreeSet::new();
    let mut stack = vec![root.join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for part in text.split("include_str!(\"").skip(1) {
                let rel = part.split('"').next().unwrap();
                let file = path.parent().unwrap().join(rel).canonicalize().unwrap();
                let top = file.strip_prefix(root.canonicalize().unwrap()).unwrap();
                embedded.insert(
                    top.components()
                        .next()
                        .unwrap()
                        .as_os_str()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    for top in embedded {
        assert!(
            dockerfile.contains(&format!("COPY {top} ./{top}")),
            "the Dockerfile does not copy {top}/"
        );
    }
}

#[test]
fn npm_publishes_with_trusted_publishing_and_has_a_local_fallback() {
    let workflow = fs::read_to_string(".github/workflows/release.yml").unwrap();
    let npm_job = workflow
        .split("  npm:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  homebrew:").next())
        .unwrap();
    assert!(
        npm_job.contains("npm install -g npm@"),
        "trusted publishing (OIDC) needs npm 11.5.1 or later"
    );
    assert!(
        !npm_job.contains("skipping npm"),
        "a missing token falls through to trusted publishing, not a silent skip"
    );
    let script = fs::read_to_string("npm/scripts/publish-local.sh").unwrap();
    assert!(script.contains("gh release download"), "{script}");
    assert!(script.contains("build-platform-packages.js"), "{script}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata("npm/scripts/publish-local.sh")
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "publish-local.sh is executable");
    }
}
