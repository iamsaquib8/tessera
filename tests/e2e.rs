use std::fs;
use std::time::Instant;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn write_sample(temp: &TempDir) {
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

    fs::write(
        temp.path().join("users.go"),
        r#"
package sample

func FindByID(id string) string {
    return loadUser(id)
}

func loadUser(id string) string {
    return id
}

func RenderUser(id string) string {
    return FindByID(id)
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("store.rs"),
        r#"
pub fn find_by_id(id: &str) -> String {
    load_user(id)
}

fn load_user(id: &str) -> String {
    id.to_string()
}

pub fn render_user(id: &str) -> String {
    find_by_id(id)
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("UserService.java"),
        r#"
package sample;

public class UserService {
    public String findById(String id) {
        return loadUser(id);
    }

    private String loadUser(String id) {
        return id;
    }

    public String renderUser(String id) {
        return findById(id);
    }
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("UserList.tsx"),
        r#"
export function UserAvatar({ name }: { name: string }) {
    return <div>{name}</div>;
}

export function UserCard({ name }: { name: string }) {
    return (
        <article>
            <UserAvatar name={name} />
        </article>
    );
}

export function UserList({ names }: { names: string[] }) {
    return (
        <section>
            {names.map((n) => (
                <UserCard name={n} />
            ))}
        </section>
    );
}
"#,
    )
    .unwrap();
}

#[test]
fn indexes_and_queries_all_languages() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);

    let db = temp.path().join(".tessera/test.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("indexed"));

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

    // Go: FindByID is called by RenderUser
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "FindByID", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("RenderUser"));

    // Rust: find_by_id is called by render_user
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "find_by_id", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("render_user"));

    // Java: findById is called by renderUser
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "findById", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("renderUser"));

    // TSX/React: UserCard is rendered inside UserList — JSX element should
    // register as a reference, so the impact lookup returns UserList.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "UserCard", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("UserList"));

    // TSX/React: UserAvatar is rendered inside UserCard.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "UserAvatar", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("UserCard"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["stats", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("java").and(predicate::str::contains("typescript")));
}

#[test]
fn validates_and_suggests_near_misses() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/v.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    // Exact hit.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "validate",
            "findById",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"exists\": true"));

    // Near miss: should suggest findById.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "validate",
            "findByIdd",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"exists\": false").and(predicate::str::contains("findById")),
        );
}

#[test]
fn incremental_reindex_reuses_files() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/inc.db");

    // First (full) index.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let first_start = Instant::now();
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
        .stdout(predicate::str::contains("[incremental]").and(predicate::str::contains("reused")));
    let inc_duration = first_start.elapsed();

    // Pure incremental rerun on unchanged files should be reasonably fast.
    assert!(
        inc_duration.as_secs() < 30,
        "incremental rerun took {:?}",
        inc_duration
    );
}

#[test]
fn tests_for_finds_test_callers() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("lib.ts"),
        r#"
export function add(a: number, b: number) {
    return a + b;
}
"#,
    )
    .unwrap();
    fs::create_dir(temp.path().join("tests")).unwrap();
    fs::write(
        temp.path().join("tests/add.test.ts"),
        r#"
import { add } from "../lib";

export function testAdd() {
    return add(1, 2);
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/t.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["tests-for", "add", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("testAdd"));
}

#[test]
fn snapshot_command_writes_file() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/s.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
            "--no-snapshot",
        ])
        .assert()
        .success();
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["snapshot", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("snapshot.bin"));

    assert!(temp.path().join(".tessera/snapshot.bin").exists());
}

#[test]
fn search_filters_by_kind_language_path() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/search.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    // Glob: every `*User*` symbol across languages.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["search", "*User*", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("UserCard").and(predicate::str::contains("UserList")));

    // Kind filter: only methods.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "search",
            "User",
            "--kind",
            "method",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            // Every hit should report kind=method.
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            let hits = v["hits"].as_array().unwrap();
            !hits.is_empty()
                && hits
                    .iter()
                    .all(|h| h["symbol"]["kind"].as_str() == Some("method"))
        }));

    // Language filter: only java.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "search",
            "find",
            "--language",
            "java",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            let hits = v["hits"].as_array().unwrap();
            !hits.is_empty()
                && hits
                    .iter()
                    .all(|h| h["symbol"]["language"].as_str() == Some("java"))
        }));
}

#[test]
fn context_pack_bundles_body_deps_callers() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/ctx.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "context-pack",
            "findById",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["symbol"]["name"] == "findById"
                && v["body"].is_string()
                && v["caller_signatures"].is_array()
        }));
}

#[test]
fn imports_and_imported_by_track_module_graph() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("util.ts"),
        "export function help() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("app.ts"),
        r#"import { help } from "./util";

export function run() {
    return help();
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/imp.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["imports", "app.ts", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("./util"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["imported-by", "./util", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("app.ts"));
}

#[test]
fn signature_lists_class_members() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/sig.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "signature",
            "UserService",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            let members = v["members"].as_array().unwrap();
            !members.is_empty()
                && members
                    .iter()
                    .any(|m| m["qualified_name"].as_str() == Some("UserService.findById"))
        }));
}

#[test]
fn siblings_finds_shared_caller_cluster() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("lib.ts"),
        r#"export function a() { return 1; }
export function b() { return 2; }
export function c() { return 3; }

export function callerOne() {
    return a() + b();
}

export function callerTwo() {
    return a() + b() + c();
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/sib.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    // a and b share both callers → top sibling for a should be b.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["siblings", "a", "--db", db.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            let sibs = v["siblings"].as_array().unwrap();
            sibs.iter().any(|s| {
                s["qualified_name"].as_str() == Some("b") && s["shared_callers"].as_u64() == Some(2)
            })
        }));
}

#[test]
fn validate_snippet_detects_unresolved_calls() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/snip.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "index",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--full",
        ])
        .assert()
        .success();

    let snippet = "function x() { return findById(1) + findByIdd(2); }";
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "validate-snippet",
            "--language",
            "typescript",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .write_stdin(snippet)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"unresolved_calls\""));
}
