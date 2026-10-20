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

fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[(samples.len() * 95 / 100).min(samples.len() - 1)]
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored"]
fn freshness_and_lookups_on_ten_thousand_files() {
    use tessera_codegraph::fresh;
    use tessera_codegraph::query::{FindArgs, RefsArgs};

    let (_temp, index) = synth_index(10_000);
    let ws = index.workspace();
    let (_, check) = timed(|| fresh::check(&ws).unwrap());
    assert!(
        check <= Duration::from_millis(100),
        "fresh::check took {check:?}"
    );

    let mut finds = Vec::new();
    let mut refs = Vec::new();
    for i in 0..100 {
        let name = format!("bridge{}", i * 97 % 10_000);
        finds.push(
            timed(|| {
                index
                    .find(&FindArgs {
                        symbol: Some(name.clone()),
                        ..FindArgs::default()
                    })
                    .unwrap()
            })
            .1,
        );
        refs.push(
            timed(|| {
                index
                    .refs(&RefsArgs {
                        target: name.clone(),
                        ..RefsArgs::default()
                    })
                    .unwrap()
            })
            .1,
        );
    }
    let (find_p95, refs_p95) = (p95(finds), p95(refs));
    assert!(
        find_p95 <= Duration::from_millis(50),
        "find p95 {find_p95:?}"
    );
    assert!(
        refs_p95 <= Duration::from_millis(50),
        "refs p95 {refs_p95:?}"
    );
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored"]
fn incremental_index_on_forty_thousand_files() {
    let (temp, index) = synth_index(40_000);
    let root = index.root().to_path_buf();
    drop(index);
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(
        root.join("module_00007.ts"),
        "export function changed() { return 1; }\n",
    )
    .unwrap();
    let report = Index::build(&root, temp.path().join("t.db"), IndexOptions::default()).unwrap();
    assert_eq!(report.files_indexed, 1);
    assert_eq!(report.files_removed, 0);
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored"]
fn no_op_reindex_skips_unchanged_files_quickly() {
    let (temp, index) = synth_index(10_000);
    let root = index.root().to_path_buf();
    drop(index);
    let (report, took) =
        timed(|| Index::build(&root, temp.path().join("t.db"), IndexOptions::default()).unwrap());
    assert_eq!(report.files_indexed, 0);
    assert!(
        took <= Duration::from_millis(400),
        "no-op re-index took {took:?}"
    );
}
