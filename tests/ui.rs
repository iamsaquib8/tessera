mod support;

use std::fs;
use std::process::Command as StdCommand;

use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;
use tessera_codegraph::ui;
use tessera_codegraph::Workspace;

fn repo(fixture: &str) -> (TempDir, Workspace) {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(fixture), temp.path());
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        assert!(StdCommand::new("git")
            .args(args)
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
    }
    let ws = Workspace::resolve(None, None, temp.path());
    (temp, ws)
}

#[test]
fn api_answers_each_view() {
    let (temp, ws) = repo("typescript");
    let content = fs::read_to_string(temp.path().join("src/users/repo.ts")).unwrap();
    fs::write(
        temp.path().join("src/users/repo.ts"),
        content.replace("repo.findById(id)", "repo.findByIdd(id)"),
    )
    .unwrap();
    let get = |path: &str| -> Value {
        let (status, _, body) = ui::respond(&ws, path, Some("127.0.0.1:4747"));
        assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    };
    assert!(get("/api/status")["files"].as_u64().unwrap() > 0);
    let impact = get("/api/impact");
    assert_eq!(impact["verify"]["findings"][0]["id"], "V1", "{impact}");
    assert!(!get("/api/search?q=find*")["hits"]
        .as_array()
        .unwrap()
        .is_empty());
    let symbol = get("/api/symbol?name=formatName");
    assert_eq!(symbol["find"]["matches"][0]["symbol"]["name"], "formatName");
    assert!(!symbol["refs"]["references"].as_array().unwrap().is_empty());
    assert!(get("/api/path?from=handler&to=loadUser")["found"]
        .as_bool()
        .unwrap());
    assert!(get("/api/map").get("contracts").is_some());
}

#[test]
fn the_page_is_self_contained_and_hosts_are_checked() {
    let (_temp, ws) = repo("typescript");
    let (status, kind, body) = ui::respond(&ws, "/", Some("localhost:4747"));
    assert_eq!(status, 200);
    assert!(kind.starts_with("text/html"));
    let page = String::from_utf8(body).unwrap();
    assert!(
        !page.contains("http://") && !page.contains("https://") && !page.contains("//cdn"),
        "external URL in page"
    );
    let (status, _, _) = ui::respond(&ws, "/api/status", Some("evil.example:4747"));
    assert_eq!(status, 403, "DNS rebinding guard");
    let (status, _, _) = ui::respond(&ws, "/api/nope", Some("127.0.0.1:4747"));
    assert_eq!(status, 404);
}

#[test]
fn export_writes_a_static_site_with_relative_links() {
    let (temp, ws) = repo("contracts-grpc");
    let out = temp.path().join("site");
    ui::export(&ws, &out).unwrap();
    let page = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(page.contains("TESSERA_STATIC"));
    assert!(
        !page.contains("\"/api") && !page.contains("'/api"),
        "absolute API paths"
    );
    for file in ["status", "map", "services", "search"] {
        let value: Value = serde_json::from_str(
            &fs::read_to_string(out.join(format!("data/{file}.json"))).unwrap(),
        )
        .unwrap();
        assert!(value.is_object(), "{file}");
    }
    let services: Value =
        serde_json::from_str(&fs::read_to_string(out.join("data/services.json")).unwrap()).unwrap();
    assert!(services["services"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "frontend"));
    let key = ui::symbol_key("CartService.GetCart");
    assert!(
        out.join(format!("data/symbols/{key}.json")).is_file(),
        "symbol pages for busy symbols"
    );
}
