use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn indexes_and_queries_typescript_and_python() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("users.ts"),
        r#"
export function findById(id: string) {
  return loadUser(id);
}

function loadUser(id: string) {
  return { id };
}

export function handler(id: string) {
  return findById(id);
}
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("service.py"),
        r#"
def normalize_user(user):
    return user.strip()

def handler(user):
    return normalize_user(user)
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/test.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 2 files"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["find-definition", "findById", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("findById"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "find-references",
            "normalize_user",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("normalize_user(user)"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "findById", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("handler"));
}
