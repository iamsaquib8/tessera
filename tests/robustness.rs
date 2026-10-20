mod support;

use std::thread;

use support::index_fixture;
use tessera_codegraph::indexer::IndexOptions;
use tessera_codegraph::query::{BlastArgs, FindArgs, RefsArgs};
use tessera_codegraph::Index;

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
