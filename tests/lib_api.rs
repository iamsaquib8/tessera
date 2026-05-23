use std::fs;

use tempfile::TempDir;

use tessera_codegraph::{Index, IndexOptions, Language};

fn write_repo(temp: &TempDir) {
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
}

#[test]
fn library_round_trip() {
    let temp = TempDir::new().unwrap();
    write_repo(&temp);
    let db = temp.path().join("tessera.db");

    let report = Index::build(temp.path(), &db, IndexOptions::default()).unwrap();
    assert!(report.symbols_indexed >= 3);

    let idx = Index::open(&db).unwrap();
    let definition = idx.find_definition("findById").unwrap();
    assert!(definition.matches.iter().any(|m| m.name == "findById"));

    let impact = idx.impact("findById", 4).unwrap();
    assert!(impact.callers.iter().any(|c| c.symbol.name == "handler"));
    assert!(impact
        .callers
        .iter()
        .all(|c| c.criticality <= 100.0 && c.criticality >= 0.0));

    let validate = idx.validate("findByIdd").unwrap();
    assert!(!validate.exists);
    assert!(!validate.candidates.is_empty());

    let snippet_result = idx
        .validate_snippet("findById(1); findByIdd(2);", Language::TypeScript)
        .unwrap();
    assert_eq!(snippet_result.total_calls, 2);
    assert!(snippet_result.unresolved_calls >= 1);

    let stats = idx.stats().unwrap();
    assert!(stats.symbols > 0);
}

#[test]
fn incremental_then_full_match() {
    let temp = TempDir::new().unwrap();
    write_repo(&temp);
    let db = temp.path().join("tessera.db");

    Index::build(temp.path(), &db, IndexOptions::default()).unwrap();
    let initial = Index::open(&db).unwrap().stats().unwrap();

    // Rerun incrementally — counts should be stable.
    let again = Index::build(temp.path(), &db, IndexOptions::default()).unwrap();
    assert_eq!(again.files_removed, 0);

    let after = Index::open(&db).unwrap().stats().unwrap();
    assert_eq!(initial.files, after.files);
    assert_eq!(initial.symbols, after.symbols);
}
