mod support;

use std::thread;

use support::index_fixture;
use tessera_codegraph::indexer::IndexOptions;
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
        let result = Index::open(&db).and_then(|index| index.find_references("loadUser"));
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
            serde_json::to_string(&index.impact("sharedHelper", 4).unwrap()).unwrap()
        })
        .collect();
    assert!(runs.iter().all(|run| run == &runs[0]), "{runs:#?}");
}

#[test]
fn fuzzy_definitions_are_labelled() {
    let (_temp, db) = index_fixture("typescript");
    let index = Index::open(&db).unwrap();
    let result = index.find_definition("findByIdd").unwrap();
    assert!(!result.matches.is_empty());
    assert!(matches!(
        result.match_kind,
        tessera_codegraph::output::MatchKind::Fuzzy { .. }
    ));
    assert!(result
        .to_string()
        .starts_with("No exact definition; closest matches:"));
    let exact = index.find_definition("findById").unwrap();
    assert_eq!(
        exact.match_kind,
        tessera_codegraph::output::MatchKind::Exact
    );
}
