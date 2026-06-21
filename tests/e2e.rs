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

    fs::write(
        temp.path().join("store.c"),
        r#"#include <stdio.h>

int c_load_user(int id) { return id; }

int c_find_by_id(int id) { return c_load_user(id); }

int c_render_user(int id) { return c_find_by_id(id); }
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("store.cpp"),
        r#"#include <string>

class CppStore {
public:
    int cppLoad(int id) { return id; }
    int cppFind(int id) { return cppLoad(id); }
    int cppRender(int id) { return cppFind(id); }
};
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("UserService.cs"),
        r#"using System;

public class CsService {
    public int CsLoad(int id) { return id; }
    public int CsFind(int id) { return CsLoad(id); }
    public int CsRender(int id) { return CsFind(id); }
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("user_service.rb"),
        r#"require 'set'

class RbService
  def rb_load_user(id)
    id
  end

  def rb_find_by_id(id)
    rb_load_user(id)
  end

  def rb_render_user(id)
    rb_find_by_id(id)
  end
end
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("UserService.php"),
        r#"<?php

function php_load_user($id) { return $id; }

function php_find_by_id($id) { return php_load_user($id); }

function php_render_user($id) { return php_find_by_id($id); }
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

    // C: c_find_by_id is called by c_render_user.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "c_find_by_id", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("c_render_user"));

    // C++: cppFind (a method on CppStore) is called by cppRender.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "cppFind", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("cppRender"));

    // C#: CsFind is called by CsRender.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "CsFind", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("CsRender"));

    // Ruby: rb_find_by_id is called by rb_render_user.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "rb_find_by_id", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("rb_render_user"));

    // PHP: php_find_by_id is called by php_render_user.
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["impact", "php_find_by_id", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("php_render_user"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["stats", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("java")
                .and(predicate::str::contains("typescript"))
                .and(predicate::str::contains("csharp"))
                .and(predicate::str::contains("ruby"))
                .and(predicate::str::contains("php")),
        );
}

#[test]
fn connect_finds_call_paths_and_export_renders_graph() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/c.db");
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

    // handler -> findById -> loadUser : a path exists.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "connect",
            "handler",
            "loadUser",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Call path")
                .and(predicate::str::contains("findById"))
                .and(predicate::str::contains("loadUser")),
        );

    // No reverse path: loadUser does not call handler.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "connect",
            "loadUser",
            "handler",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No call path"));

    // Mermaid export of the whole graph.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "export",
            "--format",
            "mermaid",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("graph TD").and(predicate::str::contains("-->")));

    // DOT export rooted at a symbol.
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "export",
            "--format",
            "dot",
            "--from",
            "findById",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("digraph tessera")
                .and(predicate::str::contains("loadUser"))
                .and(predicate::str::contains("->")),
        );

    let html = temp.path().join("graph.html");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "export",
            "--format",
            "mermaid",
            "--from",
            "handler",
            "--group-by",
            "language",
            "--collapse-tests",
            "--html-out",
            html.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["group_by"] == "language"
                && v["html_path"].is_string()
                && v["diagram"].as_str().unwrap().contains("subgraph")
        }));
    let html_content = fs::read_to_string(html).unwrap();
    assert!(html_content.contains("Tessera Graph Preview"));
    assert!(html_content.contains("Copy Mermaid"));
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
fn plan_query_recommends_agent_workflow() {
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "plan-query",
            "edit findById safely",
            "--symbol",
            "findById",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["inferred_intent"] == "prepare_edit"
                && v["steps"].as_array().unwrap().iter().any(|step| {
                    step["tool"] == "edit-prep"
                        && step["command"]
                            .as_str()
                            .unwrap()
                            .contains("tessera edit-prep findById")
                })
        }));
}

#[test]
fn edit_prep_bundles_pre_edit_graph_context() {
    let temp = TempDir::new().unwrap();
    write_sample(&temp);
    let db = temp.path().join(".tessera/edit.db");
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
            "edit-prep",
            "findById",
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["validate"]["exists"] == true
                && v["signature"]["symbol"]["name"] == "findById"
                && v["context"]["body"].is_string()
                && v["tests"]["tests"].is_array()
                && v["next_steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|step| step["tool"] == "impact")
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
fn unused_reports_zero_inbound_symbols_with_filters() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::create_dir(temp.path().join("tests")).unwrap();
    fs::write(
        temp.path().join("src/app.ts"),
        r#"
export function entry() {
    return used();
}

function used() {
    return 1;
}

function orphan() {
    return 2;
}

export function publicUnused() {
    return 3;
}
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("tests/app.test.ts"),
        r#"
export function testOnlyHelper() {
    return 4;
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/unused.db");
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
        .args(["unused", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("orphan"))
        .stdout(predicate::str::contains("publicUnused"))
        .stdout(predicate::str::contains("testOnlyHelper").not());

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "unused",
            "--db",
            db.to_str().unwrap(),
            "--kind",
            "function",
            "--language",
            "typescript",
            "--path",
            "src/",
            "--exported=false",
            "--limit",
            "1",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["symbols"].as_array().unwrap().len() == 1
                && v["symbols"][0]["symbol"]["name"] == "orphan"
                && v["symbols"][0]["inbound_refs"] == 0
                && v["symbols"][0]["inbound_edges"] == 0
        }));
}

#[test]
fn watch_once_indexes_and_exits() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("app.ts"),
        r#"
export function start() {
    return helper();
}

function helper() {
    return 1;
}
"#,
    )
    .unwrap();

    let db = temp.path().join(".tessera/watch.db");
    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "watch",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--once",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[watch:incremental] indexed"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["find-definition", "start", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("start"));
}

#[test]
fn doctor_reports_index_health_and_missing_db_guidance() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("app.ts"),
        "export function start() { return 1; }\n",
    )
    .unwrap();
    let db = temp.path().join(".tessera/doctor.db");

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "doctor",
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("database"))
        .stdout(predicate::str::contains("tessera index . --db"));

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
            "doctor",
            "--root",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            v["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["name"] == "schema" && check["status"] == "ok")
                && v["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|check| check["name"] == "parsers" && check["status"] == "ok")
        }));
}

#[test]
fn init_creates_project_defaults_and_mcp_snippets() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join(".git")).unwrap();
    let db = temp.path().join(".tessera/init.db");

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "init",
            temp.path().to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--git-hooks",
            "--mcp-configs",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let v: serde_json::Value = serde_json::from_str(out).unwrap();
            let created = v["created"].as_array().unwrap();
            created.iter().any(|path| path == ".tessera/config.toml")
                && created.iter().any(|path| path == ".tessera/mcp/codex.toml")
                && created.iter().any(|path| path == ".git/hooks/post-merge")
        }));

    assert!(temp.path().join(".tessera/config.toml").exists());
    assert!(temp.path().join(".tessera/mcp/claude.json").exists());
    assert!(temp.path().join(".git/hooks/post-checkout").exists());
}

#[test]
fn completions_print_shell_scripts() {
    Command::cargo_bin("tessera")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -F _tessera tessera"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("init"));
}

#[test]
fn query_missing_database_is_actionable() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join(".tessera/missing.db");

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["stats", "--db", db.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Tessera database not found"))
        .stderr(predicate::str::contains("tessera index . --db"));
}

#[test]
fn explain_flags_and_empty_results_are_actionable() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("app.ts"),
        r#"
export function entry() {
    return helper();
}

function helper() {
    return 1;
}
"#,
    )
    .unwrap();
    let db = temp.path().join(".tessera/explain.db");
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
            "impact",
            "helper",
            "--db",
            db.to_str().unwrap(),
            "--explain",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Why:"))
        .stdout(predicate::str::contains("personalised PageRank"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args(["validate", "helpr", "--db", db.to_str().unwrap(), "--why"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Why:"))
        .stdout(predicate::str::contains("Bloom filter"));

    Command::cargo_bin("tessera")
        .unwrap()
        .args([
            "find-definition",
            "missingSymbol",
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "hint: run `tessera search <name>`",
        ));
}

#[test]
fn release_docs_and_schema_snapshot_exist() {
    let schema = fs::read_to_string("docs/json-schemas/tessera-cli-v0.5.schema.json").unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert_eq!(schema_json["title"], "Tessera CLI JSON responses v0.5");

    for path in [
        "docs/first-five-minutes.md",
        "docs/when-not-to-use.md",
        "docs/troubleshooting.md",
    ] {
        assert!(fs::metadata(path).unwrap().is_file(), "{path} missing");
    }
}

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
