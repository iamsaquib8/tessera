#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn host_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        other => panic!("unsupported test host {other:?}"),
    }
}

fn fake_release(dir: &Path, version: &str) {
    let release = dir.join(version);
    let stage = dir.join("stage");
    fs::create_dir_all(&release).unwrap();
    fs::create_dir_all(&stage).unwrap();
    let bin = stage.join("tessera");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\necho \"tessera {}\"\n",
            version.trim_start_matches('v')
        ),
    )
    .unwrap();
    Command::new("chmod").arg("755").arg(&bin).status().unwrap();
    let archive = format!("tessera-{}.tar.gz", host_target());
    let status = Command::new("tar")
        .args(["-czf"])
        .arg(release.join(&archive))
        .args(["-C"])
        .arg(&stage)
        .arg("tessera")
        .status()
        .unwrap();
    assert!(status.success());
    let sums = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cd '{}' && (shasum -a 256 {archive} 2>/dev/null || sha256sum {archive})",
            release.display()
        ))
        .output()
        .unwrap();
    fs::write(release.join("SHA256SUMS"), sums.stdout).unwrap();
}

fn run_installer(base: &Path, install_dir: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env("TESSERA_VERSION", "v9.9.9")
        .env("TESSERA_BASE_URL", format!("file://{}", base.display()))
        .env("TESSERA_INSTALL_DIR", install_dir)
        .output()
        .unwrap()
}

#[test]
fn installs_and_verifies_checksum() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let install_dir = temp.path().join("bin");
    let output = run_installer(&base, &install_dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = Command::new(install_dir.join("tessera"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "tessera 9.9.9"
    );
}

#[test]
fn refuses_a_tampered_archive() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let sums = base.join("v9.9.9/SHA256SUMS");
    let tampered =
        fs::read_to_string(&sums)
            .unwrap()
            .replacen(|c: char| c.is_ascii_hexdigit(), "0", 8);
    fs::write(&sums, tampered).unwrap();
    let install_dir = temp.path().join("bin");
    let output = run_installer(&base, &install_dir);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"));
    assert!(!install_dir.join("tessera").exists());
}
