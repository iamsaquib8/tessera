mod support;

use support::{index_fixture, synth_repo};
use tempfile::TempDir;
use tessera_codegraph::fresh::Freshness;
use tessera_codegraph::output::{AgentText, Confidence, MatchKind};
use tessera_codegraph::query::{
    BlastArgs, ContextArgs, FindArgs, PathArgs, RefsArgs, SearchArgs, VerifyArgs,
};
use tessera_codegraph::{Index, IndexOptions, Language};

fn open(lang: &str) -> (TempDir, Index) {
    let (temp, db) = index_fixture(lang);
    (temp, Index::open(&db).unwrap())
}

fn symbol(name: &str) -> FindArgs {
    FindArgs {
        symbol: Some(name.to_string()),
        ..FindArgs::default()
    }
}

#[test]
fn find_returns_exact_matches_with_bodies() {
    let (_temp, index) = open("typescript");
    let result = index
        .find(&FindArgs {
            include_body: true,
            ..symbol("findById")
        })
        .unwrap();
    let first = &result.matches[0];
    assert_eq!(first.match_kind, MatchKind::Exact);
    assert_eq!(first.symbol.qualified_name, "UserRepo.findById");
    assert!(first
        .body
        .as_deref()
        .unwrap()
        .contains("formatName(this.lookup(id))"));
}

#[test]
fn index_open_on_moved_repo_finds_symbol_body() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    support::copy_dir(&support::fixture_dir("typescript"), &root);
    let db = root.join(".tessera").join("tessera.db");
    Index::build(
        &root,
        &db,
        IndexOptions {
            full: true,
            ..IndexOptions::default()
        },
    )
    .unwrap();

    let moved_repo = temp.path().join("repo_moved");
    std::fs::rename(&root, &moved_repo).unwrap();
    let moved_db = moved_repo.join(".tessera").join("tessera.db");

    let index = Index::open(&moved_db).unwrap();
    let result = index
        .find(&FindArgs {
            include_body: true,
            ..symbol("findById")
        })
        .unwrap();
    let first = &result.matches[0];
    assert_eq!(first.match_kind, MatchKind::Exact);
    assert!(first
        .body
        .as_deref()
        .unwrap()
        .contains("formatName(this.lookup(id))"));
}

#[test]
fn find_labels_fuzzy_matches() {
    let (_temp, index) = open("typescript");
    let result = index.find(&symbol("findByIdd")).unwrap();
    assert!(!result.matches.is_empty());
    assert!(result
        .matches
        .iter()
        .all(|m| matches!(m.match_kind, MatchKind::Fuzzy { .. })));
}

#[test]
fn find_path_gives_outline_and_imports() {
    let (_temp, index) = open("typescript");
    let result = index
        .find(&FindArgs {
            path: Some("src/users/repo.ts".to_string()),
            ..FindArgs::default()
        })
        .unwrap();
    let names: Vec<_> = result
        .outline
        .iter()
        .map(|s| s.qualified_name.as_str())
        .collect();
    assert!(names.contains(&"UserRepo"));
    assert!(names.contains(&"loadUser"));
    assert!(result.imports.iter().any(|i| i.source == "../util/format"));
}

#[test]
fn refs_lists_cross_file_calls_and_tests() {
    let (_temp, index) = open("typescript");
    let all = index
        .refs(&RefsArgs {
            target: "loadUser".to_string(),
            ..RefsArgs::default()
        })
        .unwrap();
    assert_eq!(all.definitions, 2);
    assert!(all
        .references
        .iter()
        .any(|r| r.reference.path == "src/api/handler.ts" && r.reference.line == 4));
    let tests = index
        .refs(&RefsArgs {
            target: "loadUser".to_string(),
            tests_only: true,
            ..RefsArgs::default()
        })
        .unwrap();
    assert!(!tests.references.is_empty());
    assert!(tests
        .references
        .iter()
        .all(|r| r.reference.path == "test/repo.test.ts"));
}

#[test]
fn refs_on_a_file_lists_importers() {
    let (_temp, index) = open("javascript");
    let result = index
        .refs(&RefsArgs {
            target: "lib/util.js".to_string(),
            ..RefsArgs::default()
        })
        .unwrap();
    assert_eq!(
        result.importers,
        vec!["lib/email_worker.js", "specs/util.spec.js"]
    );
}

#[test]
fn refs_paginate_without_silent_caps() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    synth_repo(&root, 300);
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();
    let index = Index::open(&db).unwrap();
    let first = index
        .refs(&RefsArgs {
            target: "sharedHelper".to_string(),
            limit: 250,
            ..RefsArgs::default()
        })
        .unwrap();
    assert_eq!(first.page.total, 600);
    assert_eq!(first.references.len(), 250);
    assert!(first.page.truncated);
    assert_eq!(first.page.next_cursor.as_deref(), Some("o:250"));
}

#[test]
fn blast_ranks_callers_of_a_symbol() {
    let (_temp, index) = open("typescript");
    let result = index
        .blast(&BlastArgs {
            symbol: Some("formatName".to_string()),
            ..BlastArgs::default()
        })
        .unwrap();
    let callers: Vec<_> = result
        .callers
        .iter()
        .map(|c| c.symbol.qualified_name.as_str())
        .collect();
    assert!(callers.contains(&"UserRepo.findById"));
    assert!(callers.contains(&"loadUser"));
}

#[test]
fn path_connects_two_symbols() {
    let (_temp, index) = open("typescript");
    let result = index
        .path(&PathArgs {
            from: "handler".to_string(),
            to: "formatName".to_string(),
            depth: 6,
        })
        .unwrap();
    assert!(result.found);
}

#[test]
fn context_bundles_body_and_callers_within_budget() {
    let (_temp, index) = open("typescript");
    let result = index
        .context(&ContextArgs {
            symbol: "findById".to_string(),
            budget_tokens: 2_000,
        })
        .unwrap();
    assert!(result.symbol.is_some());
    assert!(result.body.as_deref().unwrap().contains("formatName"));
    assert!(result.callers.iter().any(|c| c.contains("loadUser")));
    assert!(result.tokens <= 2_000);
}

#[test]
fn verify_flags_only_invented_calls() {
    let (_temp, index) = open("typescript");
    let result = index
        .verify(&VerifyArgs {
            snippet: Some("JSON.parse(x); findByIdd(1);".to_string()),
            language: Some(Language::TypeScript),
            ..VerifyArgs::default()
        })
        .unwrap();
    assert!(!result.ok);
    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    assert_eq!(result.findings[0].id, "V1");
    assert_eq!(result.findings[0].suggestions[0].name, "findById");
    let known = index
        .verify(&VerifyArgs {
            symbol: Some("findById".to_string()),
            ..VerifyArgs::default()
        })
        .unwrap();
    assert!(known.ok);
}

#[test]
fn search_unused_skips_type_like_kinds() {
    let (_temp, index) = open("javascript");
    let result = index
        .search(&SearchArgs {
            pattern: "*".to_string(),
            unused: true,
            ..SearchArgs::default()
        })
        .unwrap();
    assert!(result
        .hits
        .iter()
        .all(|h| !matches!(h.symbol.kind.as_str(), "class" | "struct" | "interface")));
    assert!(result.note.is_some());
}

#[test]
fn status_counts_match_the_index_tables() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    for i in 0..3 {
        std::fs::write(
            root.join(format!("m{i}.ts")),
            format!(
                "export function f{i}() {{ return g{i}(); }}\nfunction g{i}() {{ return {i}; }}\n"
            ),
        )
        .unwrap();
    }
    for i in 0..2 {
        std::fs::write(
            root.join(format!("m{i}.py")),
            format!("def f{i}():\n    return g{i}()\n\ndef g{i}():\n    return {i}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("m.go"),
        "package m\n\nfunc F() int { return G() }\n\nfunc G() int { return 1 }\n",
    )
    .unwrap();
    let db = temp.path().join("t.db");
    Index::build(&root, &db, IndexOptions::default()).unwrap();

    let conn = rusqlite::Connection::open(&db).unwrap();
    let count = |table: &str| -> usize {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as usize
    };
    let status = Index::open(&db).unwrap().status().unwrap();
    assert_eq!(status.files, count("files"));
    assert_eq!(status.symbols, count("symbols"));
    assert_eq!(status.refs, count("refs"));
    let languages: Vec<(&str, usize)> = status
        .languages
        .iter()
        .map(|l| (l.language.as_str(), l.count))
        .collect();
    assert_eq!(
        languages,
        [("typescript", 3), ("python", 2), ("go", 1)],
        "languages are ordered by file count"
    );
}

#[test]
fn status_reports_a_fresh_index() {
    let (_temp, index) = open("go");
    let status = index.status().unwrap();
    assert_eq!(status.freshness, Freshness::Fresh);
    assert_eq!(status.schema, tessera_codegraph::db::SCHEMA_VERSION);
    assert!(status.files > 0);
}

#[test]
fn blast_rejects_diff_revisions_that_look_like_git_options() {
    let (temp, db) = index_fixture("typescript");
    let root = temp.path().join("repo");
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "one",
        ],
    ] {
        assert!(std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
    }
    let target = root.join("PWNED");
    let index = Index::open(&db).unwrap();
    let result = index.blast(&BlastArgs {
        diff_from: Some(format!("--output={}", target.display())),
        diff_to: Some("txt".to_string()),
        ..BlastArgs::default()
    });
    assert!(result.is_err());
    let leaked: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("PWNED"))
        .collect();
    assert!(leaked.is_empty(), "git wrote {leaked:?}");
}

fn ref_lines(result: &tessera_codegraph::RefsResult) -> Vec<String> {
    result
        .references
        .iter()
        .map(|r| {
            format!(
                "{}:{} {:?}",
                r.reference.path, r.reference.line, r.confidence
            )
        })
        .collect()
}

#[test]
fn refs_follow_resolved_edges_and_report_confidence() {
    let (_temp, index) = open("ts-resolution");
    let result = index
        .refs(&RefsArgs {
            target: "UserRepo.get".to_string(),
            ..RefsArgs::default()
        })
        .unwrap();
    let lines = ref_lines(&result);
    assert_eq!(
        lines,
        vec![
            "src/data/repo.ts:15 High",
            "src/services/users.ts:17 High",
            "src/services/users.ts:28 High"
        ],
        "`this.rows.get` on a Map is not a reference to UserRepo.get"
    );
    assert!(!result.agent_text().contains("name-level matching"));
    let definition = result.references[0].definition.as_ref().unwrap();
    assert_eq!(
        (
            definition.path.as_str(),
            definition.start_line,
            definition.end_line
        ),
        ("src/data/repo.ts", 10, 12)
    );

    let display = index
        .refs(&RefsArgs {
            target: "display".to_string(),
            ..RefsArgs::default()
        })
        .unwrap();
    assert!(display
        .references
        .iter()
        .all(|r| r.confidence == Confidence::Medium));
    let high_only = index
        .refs(&RefsArgs {
            target: "display".to_string(),
            min_confidence: Some(Confidence::High),
            ..RefsArgs::default()
        })
        .unwrap();
    assert!(high_only.references.is_empty());
}

#[test]
fn blast_walks_resolved_callers_with_the_weakest_confidence_on_the_path() {
    let (_temp, index) = open("ts-resolution");
    let blast = index
        .blast(&BlastArgs {
            symbol: Some("UserRepo.require".to_string()),
            ..BlastArgs::default()
        })
        .unwrap();
    let callers: Vec<(String, usize, Confidence)> = blast
        .callers
        .iter()
        .map(|c| (c.symbol.qualified_name.clone(), c.depth, c.confidence))
        .collect();
    assert!(callers.contains(&("UserService.name".to_string(), 1, Confidence::High)));
    // UserService.name is reached by the test only through a name-matched call.
    assert!(callers.contains(&(
        "describe(\"UserService\").it(\"names a user\")".to_string(),
        2,
        Confidence::Medium
    )));
    assert!(!callers.iter().any(|(name, _, _)| name == "Badge"));
    assert_eq!(blast.tests, vec!["test/users.test.ts".to_string()]);
}

#[test]
fn context_lists_resolved_callers_and_callees() {
    let (_temp, index) = open("ts-resolution");
    let context = index
        .context(&ContextArgs {
            symbol: "UserRepo.require".to_string(),
            ..ContextArgs::default()
        })
        .unwrap();
    assert!(context
        .callers
        .iter()
        .any(|c| c.contains("UserService.name")));
    assert!(context
        .dependencies
        .iter()
        .any(|d| d.contains("UserRepo.get") && d.contains("src/data/repo.ts")));
    assert!(context
        .dependencies
        .iter()
        .any(|d| d.contains("notFound") && d.contains("src/domain/errors.ts")));
}

#[test]
fn unused_checks_types_through_type_references() {
    let (_temp, index) = open("ts-resolution");
    let result = index
        .search(&SearchArgs {
            pattern: "*".to_string(),
            unused: true,
            ..SearchArgs::default()
        })
        .unwrap();
    let names: Vec<&str> = result
        .hits
        .iter()
        .map(|h| h.symbol.qualified_name.as_str())
        .collect();
    assert!(names.contains(&"isUser"), "{names:?}");
    for used in ["Entity", "UserRepo", "UserRepo.get", "Repo", "Repo.get"] {
        assert!(!names.contains(&used), "{used} is used: {names:?}");
    }
}
