use std::fs;

use tempfile::TempDir;

use tessera_codegraph::snapshot::MappedSnapshot;
use tessera_codegraph::{Index, IndexOptions};

#[test]
fn snapshot_round_trip_matches_db() {
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

    let db = temp.path().join("tessera.db");
    Index::build(
        temp.path(),
        &db,
        IndexOptions {
            full: true,
            build_snapshot: true,
        },
    )
    .unwrap();

    let snapshot_path = temp.path().join("snapshot.bin");
    assert!(snapshot_path.exists());

    let mapped = MappedSnapshot::open(&snapshot_path).unwrap();
    let graph = mapped.graph();
    assert!(graph.symbols.iter().any(|s| s.name == "findById"));
    assert!(graph.symbols.iter().any(|s| s.name == "handler"));
    assert!(!graph.edges.is_empty());
}
