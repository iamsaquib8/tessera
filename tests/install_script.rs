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

fn installer(base: &Path, install_dir: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .env_remove("TESSERA_VERSION")
        .env_remove("TESSERA_LATEST_URL")
        .env("TESSERA_BASE_URL", format!("file://{}", base.display()))
        .env("TESSERA_INSTALL_DIR", install_dir);
    command
}

fn run_installer(base: &Path, install_dir: &Path) -> std::process::Output {
    installer(base, install_dir)
        .env("TESSERA_VERSION", "v9.9.9")
        .output()
        .unwrap()
}

fn installed_version(install_dir: &Path) -> String {
    let out = Command::new(install_dir.join("tessera"))
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A server that answers every request with `302 Location: <location>` and
/// records each request line. Returns its address and the recorded lines.
fn redirect_server(
    location: impl Fn(u16) -> String + Send + 'static,
) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            log.lock().unwrap().push(line.trim().to_string());
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 || header.trim().is_empty() {
                    break;
                }
            }
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                location(port)
            );
        }
    });
    (port, seen)
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

#[test]
fn replaces_an_existing_binary_by_rename() {
    use std::os::unix::fs::MetadataExt;
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let install_dir = temp.path().join("bin");
    fs::create_dir_all(&install_dir).unwrap();
    let target = install_dir.join("tessera");
    fs::write(&target, "old").unwrap();
    let before = fs::metadata(&target).unwrap().ino();
    let output = run_installer(&base, &install_dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        fs::metadata(&target).unwrap().ino(),
        before,
        "the running binary must be replaced, not overwritten in place"
    );
    let leftovers: Vec<_> = fs::read_dir(&install_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "tessera")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn accepts_a_version_written_without_the_leading_v() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let install_dir = temp.path().join("bin");
    let output = installer(&base, &install_dir)
        .env("TESSERA_VERSION", "9.9.9")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(installed_version(&install_dir), "tessera 9.9.9");
}

#[test]
fn a_missing_version_names_the_tag_it_asked_for() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let output = installer(&base, &temp.path().join("bin"))
        .env("TESSERA_VERSION", "1.0.1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("v1.0.1"), "{stderr}");
}

#[test]
fn latest_follows_the_releases_redirect_and_never_calls_the_api() {
    let script =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh")).unwrap();
    assert!(
        !script.contains("api.github.com"),
        "the installer must not use the rate-limited API"
    );
    assert!(script.contains("releases/latest"));

    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let install_dir = temp.path().join("bin");
    let (port, seen) =
        redirect_server(|port| format!("http://127.0.0.1:{port}/o/r/releases/tag/v9.9.9"));
    let output = installer(&base, &install_dir)
        .env(
            "TESSERA_LATEST_URL",
            format!("http://127.0.0.1:{port}/o/r/releases/latest"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(installed_version(&install_dir), "tessera 9.9.9");
    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].contains("/o/r/releases/latest"), "{requests:?}");
}

#[test]
fn latest_fails_clearly_when_the_redirect_is_not_a_release() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let (port, _) = redirect_server(|port| format!("http://127.0.0.1:{port}/login"));
    let install_dir = temp.path().join("bin");
    let output = installer(&base, &install_dir)
        .env("TESSERA_LATEST_URL", format!("http://127.0.0.1:{port}/x"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("TESSERA_VERSION"), "{stderr}");
    assert!(!install_dir.join("tessera").exists());
}

#[test]
fn latest_with_a_mirror_needs_a_version_or_a_latest_url() {
    let temp = TempDir::new().unwrap();
    let base = temp.path().join("releases");
    fake_release(&base, "v9.9.9");
    let output = installer(&base, &temp.path().join("bin")).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TESSERA_VERSION") && stderr.contains("TESSERA_BASE_URL"),
        "{stderr}"
    );
}
