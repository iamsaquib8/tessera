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
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
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
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
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
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
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
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
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

/// Full index of `synth_repo(10_000)` before resolution existed (v0.10.0,
/// release build, M-series laptop). Resolution may cost at most half again.
const M0_FULL_INDEX_10K: Duration = Duration::from_millis(2_432);

#[test]
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
fn full_index_with_resolution_stays_within_half_again_of_m0() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    synth_repo(&root, 10_000);
    let (report, took) =
        timed(|| Index::build(&root, temp.path().join("t.db"), IndexOptions::default()).unwrap());
    let resolve = report.resolve.expect("a full index resolves");
    assert!(resolve.high > 0);
    let budget = M0_FULL_INDEX_10K.mul_f64(1.5);
    assert!(
        took <= budget,
        "full index took {took:?}, budget {budget:?}"
    );
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
fn single_file_edit_re_resolves_dependents_quickly() {
    use tessera_codegraph::indexer;

    let (_temp, index) = synth_index(5_000);
    let ws = index.workspace();
    drop(index);
    let mut samples = Vec::new();
    for round in 0..20 {
        let path = format!("module_{:05}.ts", 100 + round * 37);
        let content = std::fs::read_to_string(ws.root.join(&path)).unwrap();
        std::fs::write(ws.root.join(&path), format!("{content}// edit {round}\n")).unwrap();
        let (report, took) =
            timed(|| indexer::index_files(&ws, std::slice::from_ref(&path)).unwrap());
        let resolve = report.resolve.expect("an edit re-resolves");
        assert!(!resolve.fallback, "{resolve:?}");
        samples.push(took);
    }
    let p95 = p95(samples);
    assert!(p95 <= Duration::from_millis(300), "edit p95 {p95:?}");
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
fn editing_a_hub_file_stays_within_the_incremental_budget() {
    use tessera_codegraph::fresh::{self, Freshness};
    use tessera_codegraph::indexer;

    let (_temp, index) = synth_index(5_000);
    let ws = index.workspace();
    drop(index);
    let util = std::fs::read_to_string(ws.root.join("util.ts")).unwrap();
    let edit = |content: String| {
        std::fs::write(ws.root.join("util.ts"), content).unwrap();
        let (report, took) = timed(|| indexer::index_files(&ws, &["util.ts".to_string()]).unwrap());
        (report.resolve.expect("an edit re-resolves"), took)
    };
    // A body edit keeps util's exports, so its 5,000 importers keep their edges.
    let (resolve, took) = edit(util.replace("x * 13", "x * 17"));
    assert!(!resolve.fallback && resolve.files == 1, "{resolve:?}");
    assert!(
        took <= Duration::from_millis(300),
        "body edit took {took:?}"
    );
    // A new parameter reaches every importer: the edit resolves util alone
    // and leaves the importers to the next full pass.
    let (resolve, took) =
        edit(util.replace("sharedHelper(x: number)", "sharedHelper(x: number, y = 0)"));
    assert!(resolve.fallback && resolve.files == 1, "{resolve:?}");
    assert!(
        took <= Duration::from_millis(300),
        "signature edit took {took:?}"
    );
    assert_eq!(fresh::check(&ws).unwrap(), Freshness::Pending);
}

#[test]
#[ignore = "perf budget; run with --release -- --ignored --test-threads=1"]
fn post_edit_hook_stays_within_budget_on_five_thousand_files() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    synth_repo(&root, 5_000);
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
        assert!(Command::new("git")
            .args(args)
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
    }
    Index::build(
        &root,
        root.join(".tessera/tessera.db"),
        IndexOptions::default(),
    )
    .unwrap();
    let bin = assert_cmd::cargo::cargo_bin("tessera");
    let file = root.join("module_00042.ts");
    let original = std::fs::read_to_string(&file).unwrap();
    let hook = |event: &str, payload: serde_json::Value| {
        let started = Instant::now();
        let mut child = Command::new(&bin)
            .args(["hook", "claude", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        assert!(child.wait_with_output().unwrap().status.success());
        started.elapsed()
    };
    let cwd = root.to_string_lossy().into_owned();
    hook(
        "SessionStart",
        serde_json::json!({ "session_id": "p", "cwd": cwd }),
    );
    let mut samples = Vec::new();
    for i in 0..20 {
        std::fs::write(&file, format!("{original}// edit {i}\n")).unwrap();
        samples.push(hook(
            "PostToolUse",
            serde_json::json!({
                "session_id": "p",
                "cwd": cwd,
                "tool_name": "Edit",
                "tool_input": { "file_path": file.to_string_lossy(), "old_string": "", "new_string": "" },
            }),
        ));
    }
    let p95 = p95(samples);
    assert!(p95 <= Duration::from_millis(300), "post-edit p95 {p95:?}");
}
