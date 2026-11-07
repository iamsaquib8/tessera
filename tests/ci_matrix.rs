//! What the CI workflow really exercises: the declared minimum Rust and Node,
//! and Linux on arm64.

use std::fs;
use std::path::PathBuf;

use yaml_rust2::{Yaml, YamlLoader};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ci() -> Yaml {
    let text = fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
    YamlLoader::load_from_str(&text).unwrap().remove(0)
}

fn steps(job: &Yaml) -> Vec<&Yaml> {
    job["steps"].as_vec().unwrap().iter().collect()
}

#[test]
fn the_msrv_job_runs_the_declared_minimum_rust_whatever_rust_toolchain_toml_says() {
    let cargo = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let declared = cargo
        .lines()
        .find_map(|l| l.strip_prefix("rust-version = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("rust-version in Cargo.toml");
    let toolchain_file = fs::read_to_string(root().join("rust-toolchain.toml")).unwrap();
    assert!(
        toolchain_file.contains("channel = \"stable\""),
        "this test explains the override below"
    );

    let docs = ci();
    let job = &docs["jobs"]["msrv"];
    // rustup gives rust-toolchain.toml priority over the default an action
    // sets, but not over RUSTUP_TOOLCHAIN.
    assert_eq!(
        job["env"]["RUSTUP_TOOLCHAIN"].as_str(),
        Some(declared.as_str()),
        "the job must set RUSTUP_TOOLCHAIN to the declared rust-version"
    );
    let runs: Vec<&str> = steps(job)
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect();
    let version = runs
        .iter()
        .position(|r| r.contains("rustc --version"))
        .expect("a step that prints the rustc version");
    let check = runs
        .iter()
        .position(|r| {
            r.contains("cargo check") && r.contains("--locked") && r.contains("--all-targets")
        })
        .expect("cargo check --locked --all-targets");
    assert!(version < check, "the version is printed before the check");
}

#[test]
fn the_npm_launcher_is_tested_on_the_declared_node_floor_and_current_lts_lines() {
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root().join("npm/package.json")).unwrap())
            .unwrap();
    let floor: i64 = package["engines"]["node"]
        .as_str()
        .unwrap()
        .trim_start_matches(">=")
        .split('.')
        .next()
        .unwrap()
        .parse()
        .unwrap();

    let docs = ci();
    let job = &docs["jobs"]["npm"];
    let versions: Vec<i64> = job["strategy"]["matrix"]["node"]
        .as_vec()
        .expect("a node matrix")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(versions.iter().min(), Some(&floor), "{versions:?}");
    assert!(versions.contains(&20), "{versions:?}");
    assert!(versions.iter().any(|v| *v >= 22), "{versions:?}");
    let setup = steps(job)
        .into_iter()
        .find(|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("actions/setup-node@"))
        })
        .unwrap();
    assert_eq!(
        setup["with"]["node-version"].as_str(),
        Some("${{ matrix.node }}")
    );
    // Node 21 stopped treating a directory argument as "every test inside it".
    let test = steps(job)
        .into_iter()
        .filter_map(|s| s["run"].as_str())
        .find(|r| r.contains("node --test"))
        .unwrap();
    assert!(test.contains("npm/test/*.test.js"), "{test}");
}

#[test]
fn linux_arm64_runs_the_tests_and_the_musl_binary() {
    let docs = ci();
    let os = docs["jobs"]["test"]["strategy"]["matrix"]["os"]
        .as_vec()
        .unwrap();
    assert!(
        os.iter().any(|o| o.as_str() == Some("ubuntu-24.04-arm")),
        "cargo test runs on a Linux arm64 runner"
    );

    let musl = &docs["jobs"]["musl"];
    let include = musl["strategy"]["matrix"]["include"].as_vec().unwrap();
    let arm = include
        .iter()
        .find(|e| e["target"].as_str() == Some("aarch64-unknown-linux-musl"))
        .expect("an aarch64 musl entry");
    assert_eq!(arm["os"].as_str(), Some("ubuntu-24.04-arm"));
    assert!(include
        .iter()
        .any(|e| e["target"].as_str() == Some("x86_64-unknown-linux-musl")));
    let runs: String = steps(musl)
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(runs.contains("--target ${{ matrix.target }}"), "{runs}");
    assert!(
        runs.contains("--version") && runs.contains("tests/fixtures/typescript"),
        "the binary runs a real query, not only --version:\n{runs}"
    );
}
