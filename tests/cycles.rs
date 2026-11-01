mod support;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use support::{copy_dir, fixture_dir};
use tempfile::TempDir;
use tessera_codegraph::mcp::{handle, Outcome, ServerState};
use tessera_codegraph::{CyclesArgs, Index, Workspace};

fn repo(lang: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    copy_dir(&fixture_dir(lang), temp.path());
    temp
}

fn tessera(cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("tessera").unwrap();
    cmd.current_dir(cwd).arg("--quiet");
    cmd
}

#[test]
fn clean_repo_has_no_cycles() {
    let temp = repo("typescript");
    tessera(temp.path())
        .args(["cycles"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No circular import cycles found."));

    let out = tessera(temp.path())
        .args(["cycles", "--json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["total_cycles"], 0);
    assert_eq!(json["cycles"], serde_json::json!([]));
}

#[test]
fn typescript_three_module_cycle() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("auth.ts"),
        "import { getUser } from './user';\nexport const auth = () => getUser();\n",
    )
    .unwrap();
    fs::write(
        src.join("user.ts"),
        "import { checkPerm } from './permissions';\nexport const getUser = () => checkPerm();\n",
    )
    .unwrap();
    fs::write(
        src.join("permissions.ts"),
        "import { auth } from './auth';\nexport const checkPerm = () => auth();\n",
    )
    .unwrap();

    // 1. `tessera cycles` command
    tessera(temp.path())
        .args(["cycles"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 circular import cycle:"))
        .stdout(predicate::str::contains("1. (3 modules):"))
        .stdout(predicate::str::contains("src/auth.ts -> src/user.ts:1"))
        .stdout(predicate::str::contains(
            "src/user.ts -> src/permissions.ts:1",
        ))
        .stdout(predicate::str::contains(
            "src/permissions.ts -> src/auth.ts:1",
        ));

    // 2. `tessera imports --cycles` alias command
    tessera(temp.path())
        .args(["imports", "--cycles"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 circular import cycle:"))
        .stdout(predicate::str::contains("1. (3 modules):"));

    // 3. JSON format
    let out = tessera(temp.path())
        .args(["cycles", "--json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["total_cycles"], 1);
    assert_eq!(json["cycles"][0]["modules"], 3);
    let links = json["cycles"][0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);
    assert_eq!(links[0]["from"], "src/auth.ts");
    assert_eq!(links[0]["to"], "src/user.ts");
    assert_eq!(links[0]["line"], 1);
}

#[test]
fn python_two_module_cycle_and_path_filter() {
    let temp = TempDir::new().unwrap();
    let app = temp.path().join("app");
    let other = temp.path().join("other");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&other).unwrap();

    // Python circular import
    fs::write(
        app.join("order.py"),
        "from app.item import Item\nclass Order:\n    pass\n",
    )
    .unwrap();
    fs::write(
        app.join("item.py"),
        "from app.order import Order\nclass Item:\n    pass\n",
    )
    .unwrap();

    // Disjoint TypeScript circular import in other/
    fs::write(other.join("a.ts"), "import './b';\nexport const a = 1;\n").unwrap();
    fs::write(other.join("b.ts"), "import './a';\nexport const b = 2;\n").unwrap();

    // Both cycles found without filter
    let out = tessera(temp.path())
        .args(["cycles", "--json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["total_cycles"], 2);

    // Filter by path: app
    let out_app = tessera(temp.path())
        .args(["cycles", "--path", "app", "--json"])
        .output()
        .unwrap();
    let json_app: Value = serde_json::from_slice(&out_app.stdout).unwrap();
    assert_eq!(json_app["total_cycles"], 1);
    assert_eq!(json_app["cycles"][0]["links"][0]["from"], "app/item.py");
    assert_eq!(json_app["cycles"][0]["links"][0]["to"], "app/order.py");

    // Filter by path: other
    let out_other = tessera(temp.path())
        .args(["cycles", "--path", "other", "--json"])
        .output()
        .unwrap();
    let json_other: Value = serde_json::from_slice(&out_other.stdout).unwrap();
    assert_eq!(json_other["total_cycles"], 1);
    assert_eq!(json_other["cycles"][0]["links"][0]["from"], "other/a.ts");
    assert_eq!(json_other["cycles"][0]["links"][0]["to"], "other/b.ts");
}

#[test]
fn mcp_import_cycles_tool() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("a.ts"), "import './b';\n").unwrap();
    fs::write(src.join("b.ts"), "import './a';\n").unwrap();

    tessera(temp.path()).args(["index"]).assert().success();

    let db = temp.path().join(".tessera/tessera.db");
    let ws = Workspace {
        root: temp.path().to_path_buf(),
        db_path: db,
    };
    let state = ServerState::new(ws);

    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "import_cycles",
            "arguments": {}
        }
    });

    match handle(&state, &message.to_string()) {
        Outcome::Reply(reply) => {
            let content = &reply["result"]["content"];
            let text = content[0]["text"].as_str().unwrap();
            assert!(text.contains("Found 1 circular import cycle:"));
            assert!(text.contains("src/a.ts -> src/b.ts"));
            assert!(text.contains("src/b.ts -> src/a.ts"));
        }
        Outcome::Silent => panic!("expected reply from import_cycles"),
    }
}

#[test]
fn library_cycles_query() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("x.ts"), "import './y';\n").unwrap();
    fs::write(src.join("y.ts"), "import './x';\n").unwrap();

    let db_path = temp.path().join(".tessera/tessera.db");
    Index::build(temp.path(), &db_path, Default::default()).unwrap();
    let index = Index::open(&db_path).unwrap();

    let result = index.cycles(&CyclesArgs::default()).unwrap();
    assert_eq!(result.total_cycles, 1);
    assert_eq!(result.cycles[0].modules, 2);
    assert_eq!(result.cycles[0].links[0].from, "src/x.ts");
    assert_eq!(result.cycles[0].links[0].to, "src/y.ts");
    assert_eq!(result.cycles[0].links[1].from, "src/y.ts");
    assert_eq!(result.cycles[0].links[1].to, "src/x.ts");
}
