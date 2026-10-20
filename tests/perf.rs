mod support;

use std::time::{Duration, Instant};

use support::synth_repo;
use tempfile::TempDir;
use tessera_codegraph::query::{BlastArgs, ContextArgs};
use tessera_codegraph::{Index, IndexOptions};

fn synth_index(files: usize) -> (TempDir, Index) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    synth_repo(&root, files);
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();
    let index = Index::open(&db).unwrap();
    (temp, index)
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let out = f();
    (out, started.elapsed())
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored"]
fn context_and_blast_under_budget() {
    let (_temp, index) = synth_index(2_000);
    let (_, context) = timed(|| {
        index
            .context(&ContextArgs {
                symbol: "sharedHelper".to_string(),
                budget_tokens: 2_000,
            })
            .unwrap()
    });
    let (_, blast) = timed(|| {
        index
            .blast(&BlastArgs {
                symbol: Some("sharedHelper".to_string()),
                ..BlastArgs::default()
            })
            .unwrap()
    });
    assert!(
        context <= Duration::from_millis(500),
        "context took {context:?}"
    );
    assert!(blast <= Duration::from_millis(500), "blast took {blast:?}");
}
