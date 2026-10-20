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
