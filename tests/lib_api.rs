mod support;

use support::{index_fixture, synth_repo};
use tempfile::TempDir;
use tessera_codegraph::fresh::Freshness;
use tessera_codegraph::output::MatchKind;
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
        .any(|r| r.path == "src/api/handler.ts" && r.line == 4));
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
        .all(|r| r.path == "test/repo.test.ts"));
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
fn status_reports_a_fresh_index() {
    let (_temp, index) = open("go");
    let status = index.status().unwrap();
    assert_eq!(status.freshness, Freshness::Fresh);
    assert_eq!(status.schema, 4);
    assert!(status.files > 0);
}
