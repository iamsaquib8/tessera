mod support;

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use support::{graph_dump, index_fixture};
use tessera_codegraph::indexer::{self, IndexOptions};
use tessera_codegraph::query::{BlastArgs, FindArgs, RefsArgs};
use tessera_codegraph::{fresh, Index, Workspace};

#[test]
fn concurrent_index_and_query() {
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo");
    for i in 0..400 {
        std::fs::write(
            root.join(format!("src/gen_{i}.ts")),
            format!("import {{ loadUser }} from \"./users/repo\";\nexport function gen{i}() {{ return loadUser(\"{i}\"); }}\n"),
        )
        .unwrap();
    }
    let writer_db = db.clone();
    let writer = thread::spawn(move || {
        for _ in 0..10 {
            Index::build(
                &root,
                &writer_db,
                IndexOptions {
                    full: true,
                    ..IndexOptions::default()
                },
            )
            .map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    });
    let mut errors = Vec::new();
    for _ in 0..200 {
        let result = Index::open(&db).and_then(|index| {
            index.refs(&RefsArgs {
                target: "loadUser".to_string(),
                ..RefsArgs::default()
            })
        });
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    let writer_result = writer.join().unwrap();
    let locked: Vec<_> = errors
        .iter()
        .chain(writer_result.as_ref().err())
        .filter(|e| e.contains("locked"))
        .collect();
    assert!(locked.is_empty(), "lock errors: {locked:?}");
}

#[test]
fn impact_output_is_deterministic() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("util.ts"),
        "export function sharedHelper() { return 1; }\n",
    )
    .unwrap();
    let callers: String = (0..40)
        .map(|i| format!("export function caller{i}() {{ return sharedHelper(); }}\n"))
        .collect();
    std::fs::write(root.join("callers.ts"), callers).unwrap();
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();
    let runs: Vec<String> = (0..5)
        .map(|_| {
            let index = Index::open(&db).unwrap();
            let blast = index
                .blast(&BlastArgs {
                    symbol: Some("sharedHelper".to_string()),
                    ..BlastArgs::default()
                })
                .unwrap();
            serde_json::to_string(&blast).unwrap()
        })
        .collect();
    assert!(runs.iter().all(|run| run == &runs[0]), "{runs:#?}");
}

#[test]
fn fuzzy_definitions_are_labelled() {
    let (_temp, db) = index_fixture("typescript");
    let index = Index::open(&db).unwrap();
    let find = |name: &str| {
        index
            .find(&FindArgs {
                symbol: Some(name.to_string()),
                ..FindArgs::default()
            })
            .unwrap()
    };
    let result = find("findByIdd");
    assert!(!result.matches.is_empty());
    assert!(matches!(
        result.matches[0].match_kind,
        tessera_codegraph::output::MatchKind::Fuzzy { .. }
    ));
    assert!(result
        .to_string()
        .starts_with("No exact definition of findByIdd; closest matches:"));
    assert_eq!(
        find("findById").matches[0].match_kind,
        tessera_codegraph::output::MatchKind::Exact
    );
}

#[test]
fn adversarial_inputs_never_abort_indexing() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let write = |name: &str, bytes: &[u8]| std::fs::write(root.join(name), bytes).unwrap();
    write("huge_line.js", &vec![b'a'; 1_000_000]);
    write(
        "latin1.py",
        b"def caf\xe9():\n    return 1\n\ndef plain():\n    return caf\xe9()\n",
    );
    write(
        "bom.ts",
        "\u{feff}export function withBom() { return 1; }\n".as_bytes(),
    );
    write(
        "Crlf.java",
        b"public class Crlf {\r\n  public void run() {}\r\n}\r\n",
    );
    write("empty.go", b"");
    write(
        "nested.js",
        format!("const x = {}{};\n", "[".repeat(50_000), "]".repeat(50_000)).as_bytes(),
    );
    write("my file é.ts", b"export function spaced() { return 1; }\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        write("locked.ts", b"export function locked() {}\n");
        std::fs::set_permissions(
            root.join("locked.ts"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }

    let db = temp.path().join("t.db");
    let report = Index::build(&root, &db, IndexOptions::default()).unwrap();
    assert!(
        report.files_skipped >= 1,
        "huge single-line file should be skipped"
    );
    let index = Index::open(&db).unwrap();
    let find = |name: &str| {
        index
            .find(&FindArgs {
                symbol: Some(name.to_string()),
                ..FindArgs::default()
            })
            .unwrap()
    };
    for name in ["plain", "withBom", "Crlf", "spaced"] {
        let result = find(name);
        assert!(
            matches!(
                result.matches.first().map(|m| m.match_kind),
                Some(tessera_codegraph::output::MatchKind::Exact)
            ),
            "{name} not indexed: {}",
            result
        );
    }
    let bom = find("withBom");
    assert_eq!(bom.matches[0].symbol.start_line, 1);
    #[cfg(unix)]
    {
        assert!(report.warnings.iter().any(|w| w.path == "locked.ts"));
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join("locked.ts"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
}

#[test]
fn graph_is_identical_across_rebuilds() {
    for lang in [
        "typescript",
        "tsx",
        "javascript",
        "python",
        "go",
        "rust",
        "java",
        "c",
        "cpp",
        "csharp",
        "ruby",
        "php",
    ] {
        let (_a, first) = index_fixture(lang);
        let (_b, second) = index_fixture(lang);
        assert_eq!(
            support::graph_dump(&first),
            support::graph_dump(&second),
            "{lang}"
        );
    }
}

#[test]
fn cli_blast_json_is_byte_identical_across_runs() {
    let temp = tempfile::TempDir::new().unwrap();
    support::copy_dir(&support::fixture_dir("typescript"), temp.path());
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_tessera"))
            .args(["--quiet", "--json", "blast", "formatName"])
            .current_dir(temp.path())
            .output()
            .unwrap()
            .stdout
    };
    let first = run();
    for _ in 0..4 {
        assert_eq!(run(), first);
    }
}

#[cfg(unix)]
#[test]
fn unchanged_size_and_mtime_skip_rereading() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("a.ts");
    std::fs::write(&file, "export function alpha() { return 1; }\n").unwrap();
    std::fs::write(root.join("stamp"), "").unwrap();
    std::process::Command::new("touch")
        .arg("-r")
        .arg(&file)
        .arg(root.join("stamp"))
        .status()
        .unwrap();
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();
    std::fs::write(&file, "export function omega() { return 1; }\n").unwrap();
    std::process::Command::new("touch")
        .arg("-r")
        .arg(root.join("stamp"))
        .arg(&file)
        .status()
        .unwrap();
    let report = Index::build(&root, &db, IndexOptions::default()).unwrap();
    assert_eq!((report.files_indexed, report.files_reused), (0, 1));
}

fn lock_path(db: &Path) -> std::path::PathBuf {
    db.with_file_name(format!(
        "{}.lock",
        db.file_name().unwrap().to_string_lossy()
    ))
}

#[test]
fn query_serves_the_existing_index_while_another_writer_holds_the_lock() {
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo");
    fs::write(
        root.join("src/extra.ts"),
        "export function extra() { return 1; }\n",
    )
    .unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path(&db))
        .unwrap();
    lock.lock().unwrap();
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let started = Instant::now();
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .arg("--root")
        .arg(&root)
        .arg("--db")
        .arg(&db)
        .args(["find", "findById"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("UserRepo.findById"));
    assert!(
        stderr.contains("another tessera process"),
        "stderr: {stderr}"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn concurrent_refreshes_do_not_fail() {
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo").canonicalize().unwrap();
    for i in 0..300 {
        fs::write(
            root.join(format!("src/gen_{i}.ts")),
            format!("export function gen{i}() {{ return {i}; }}\n"),
        )
        .unwrap();
    }
    let workers: Vec<_> = (0..3)
        .map(|worker| {
            let ws = Workspace {
                root: root.clone(),
                db_path: db.clone(),
            };
            thread::spawn(move || {
                for round in 0..5 {
                    fs::write(
                        ws.root.join(format!("src/w{worker}.ts")),
                        format!("export function w{worker}() {{ return {round}; }}\n"),
                    )
                    .map_err(|e| e.to_string())?;
                    fresh::ensure(&ws).map_err(|e| format!("{e:#}"))?;
                }
                Ok::<_, String>(())
            })
        })
        .collect();
    let errors: Vec<String> = workers
        .into_iter()
        .filter_map(|w| w.join().unwrap().err())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn foreign_sqlite_file_is_never_overwritten() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("repo");
    support::copy_dir(&support::fixture_dir("typescript"), &root);
    let app_db = temp.path().join("app.db");
    rusqlite::Connection::open(&app_db)
        .unwrap()
        .execute_batch("CREATE TABLE users(id INTEGER); INSERT INTO users VALUES (1);")
        .unwrap();
    let output = Command::cargo_bin("tessera")
        .unwrap()
        .arg("--root")
        .arg(&root)
        .arg("--db")
        .arg(&app_db)
        .args(["find", "findById"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a Tessera index"));
    let users: i64 = rusqlite::Connection::open(&app_db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .unwrap();
    assert_eq!(users, 1);
}

#[cfg(unix)]
#[test]
fn unreadable_file_does_not_keep_the_index_stale() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo");
    let locked = root.join("src/locked.ts");
    fs::write(&locked, "export function locked() {}\n").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&locked).is_ok() {
        return;
    }
    let ws = Workspace {
        root: root.canonicalize().unwrap(),
        db_path: db.clone(),
    };
    fresh::ensure(&ws).unwrap();
    assert_eq!(fresh::check(&ws).unwrap(), fresh::Freshness::Fresh);
    let status = Index::open(&db).unwrap().status().unwrap();
    assert!(
        status
            .hints
            .iter()
            .any(|h| h.contains("1 file could not be read")),
        "{status:?}"
    );
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn explicit_paths_obey_the_same_rules_as_the_walk() {
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo");
    fs::write(root.join(".gitignore"), "generated/\n").unwrap();
    for dir in ["generated", ".next/server", "dist", "node_modules/pkg"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "generated/out.ts",
        ".next/server/page.ts",
        "dist/bundle.ts",
        "node_modules/pkg/index.ts",
    ] {
        fs::write(root.join(file), "export function leaked() { return 1; }\n").unwrap();
    }
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join("secret.ts"),
        "export function secret() { return 1; }\n",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.join("secret.ts"), root.join("src/link.ts")).unwrap();

    let ws = Workspace {
        root: root.canonicalize().unwrap(),
        db_path: db.clone(),
    };
    indexer::index_files(
        &ws,
        &[
            "generated/out.ts".to_string(),
            ".next/server/page.ts".to_string(),
            "dist/bundle.ts".to_string(),
            "node_modules/pkg/index.ts".to_string(),
            "src/link.ts".to_string(),
        ],
    )
    .unwrap();
    let dump = graph_dump(&db).to_string();
    assert!(!dump.contains("leaked"), "{dump}");
    assert!(!dump.contains("secret"), "{dump}");
    assert_eq!(fresh::check(&ws).unwrap(), fresh::Freshness::Fresh);
}
