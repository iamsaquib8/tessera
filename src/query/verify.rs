use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Result};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::common::{is_test_path, known_global};
use super::waivers::Waivers;
use super::{graph, legacy};
use crate::extract::parse_file;
use crate::facts::kind;
use crate::output::{AgentText, Confidence};
use crate::types::Receiver;
use crate::types::{Language, SymbolSuggestion};

/// Symbols reached from the changed code are walked at most this far to find
/// tests, visiting at most this many symbols each.
const TEST_DEPTH: usize = 4;
const TEST_SCAN_CAP: usize = 2_000;
/// The test walk starts from at most this many changed symbols.
const TEST_SEEDS: usize = 300;
/// Agent output lists at most this many findings; the rest are counted.
const AGENT_MAX_FINDINGS: usize = 8;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerifyArgs {
    pub symbol: Option<String>,
    pub snippet: Option<String>,
    #[serde(deserialize_with = "language_by_name")]
    pub language: Option<Language>,
    /// Check the working tree against `HEAD`, including untracked files.
    pub changed: bool,
    /// Check the staged changes.
    pub staged: bool,
    /// Check the working tree against this revision.
    pub diff_from: Option<String>,
    /// Count baselined and accepted findings as if they were new.
    pub no_baseline: bool,
}

fn language_by_name<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Language>, D::Error> {
    let name: Option<String> = Option::deserialize(deserializer)?;
    name.map(|n| {
        Language::from_name(&n)
            .ok_or_else(|| serde::de::Error::custom(format!("unsupported language `{n}`")))
    })
    .transpose()
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: String,
    pub confidence: Confidence,
    /// Only blocking findings fail `verify` (exit 1) and hooks.
    pub blocking: bool,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub suggestions: Vec<SymbolSuggestion>,
    /// Stable across edits that only move the code: what a baseline, an
    /// accepted break, or a code-scanning alert is keyed by.
    pub fingerprint: String,
    /// Present when a baseline or an accepted break covers the finding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waived: Option<super::waivers::Waiver>,
}

/// 16 hex digits of FNV-1a over `text`.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

impl Finding {
    /// The fingerprint of a finding with no semantic key: its check, file
    /// and message, which name the symbols involved but not the line.
    fn default_fingerprint(&self) -> String {
        fingerprint(&format!(
            "{} {} {}",
            self.id,
            self.path.as_deref().unwrap_or_default(),
            self.message
        ))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// Test files that reach the changed code (V6).
    pub tests: Vec<String>,
}

impl VerifyResult {
    /// Sorts `findings`, blocking first, and drops duplicates.
    pub fn new(mut findings: Vec<Finding>, tests: Vec<String>) -> Self {
        findings.sort_by(|a, b| {
            b.blocking
                .cmp(&a.blocking)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });
        findings.dedup_by(|a, b| a.id == b.id && a.path == b.path && a.line == b.line);
        for finding in findings.iter_mut().filter(|f| f.fingerprint.is_empty()) {
            finding.fingerprint = finding.default_fingerprint();
        }
        Self {
            ok: !findings.iter().any(|f| f.blocking),
            findings,
            tests,
        }
    }

    pub fn blocking(&self) -> usize {
        self.findings.iter().filter(|f| f.blocking).count()
    }
}

pub fn run(conn: &Connection, root: &Path, args: &VerifyArgs) -> Result<VerifyResult> {
    if args.changed || args.staged || args.diff_from.is_some() {
        let changes = if args.staged {
            Changes::Staged
        } else if let Some(rev) = &args.diff_from {
            Changes::Since(rev.clone())
        } else {
            Changes::WorkingTree
        };
        let waivers = if args.no_baseline {
            Waivers::none()
        } else {
            Waivers::load(root)
        };
        return verify_changes_with(conn, root, &changes, &waivers);
    }
    let mut findings = Vec::new();
    if let Some(symbol) = &args.symbol {
        let check = legacy::validate_conn(conn, symbol)?;
        if !check.exists {
            findings.push(unresolved(symbol, None, check.candidates));
        }
    }
    if let Some(snippet) = &args.snippet {
        let Some(language) = args.language else {
            bail!("--language is required with --snippet");
        };
        let parsed = parse_file(language, snippet)?;
        for reference in parsed.references {
            let name = &reference.symbol_name;
            let checkable = match reference.receiver {
                Receiver::None => !known_global(language, name),
                Receiver::Name => {
                    let receiver = reference.receiver_name.as_deref().unwrap_or_default();
                    // Only a static call on a class the index knows is checkable;
                    // a variable's type is unknown in a snippet.
                    !receiver.contains('.')
                        && !known_global(language, receiver)
                        && receiver.chars().next().is_some_and(char::is_uppercase)
                        && legacy::validate_conn(conn, receiver)?.exists
                }
                Receiver::This | Receiver::Super | Receiver::Other => false,
            };
            if !checkable {
                continue;
            }
            let check = legacy::validate_conn(conn, name)?;
            if !check.exists {
                findings.push(unresolved(name, Some(reference.line), check.candidates));
            }
        }
    }
    if args.symbol.is_none() && args.snippet.is_none() {
        bail!("pass a symbol, --snippet with --language, or --changed");
    }
    Ok(VerifyResult::new(findings, Vec::new()))
}

fn unresolved(name: &str, line: Option<usize>, suggestions: Vec<SymbolSuggestion>) -> Finding {
    let hint = suggestions
        .first()
        .map(|s| format!("; did you mean {}?", s.qualified_name))
        .unwrap_or_default();
    Finding {
        id: "V1".to_string(),
        confidence: Confidence::Name,
        blocking: true,
        message: format!("`{name}` is not defined anywhere in the index{hint}"),
        path: None,
        line,
        suggestions,
        fingerprint: fingerprint(&format!("V1 {name}")),
        waived: None,
    }
}

/// Which change to check.
#[derive(Debug, Clone)]
pub enum Changes {
    WorkingTree,
    Staged,
    Since(String),
}

/// V1–V4 and V6 over the files a change touches and the code that depends on
/// them. The index must already reflect the working tree.
pub fn verify_changes(conn: &Connection, root: &Path, changes: &Changes) -> Result<VerifyResult> {
    verify_changes_with(conn, root, changes, &Waivers::load(root))
}

/// [`verify_changes`] with these waivers.
pub fn verify_changes_with(
    conn: &Connection,
    root: &Path,
    changes: &Changes,
    waivers: &Waivers,
) -> Result<VerifyResult> {
    let changed = changed_paths(root, changes)?;
    verify_paths_with(conn, root, &changed, changes, waivers)
}

/// The same checks for an explicit set of changed files (hooks pass the
/// files an agent edited).
pub fn verify_paths(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
) -> Result<VerifyResult> {
    verify_paths_with(conn, root, changed, changes, &Waivers::load(root))
}

/// [`verify_paths`] with these waivers.
pub fn verify_paths_with(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    waivers: &Waivers,
) -> Result<VerifyResult> {
    let findings = problems_with(conn, root, changed, changes, waivers)?
        .into_iter()
        .map(|p| p.finding)
        .collect();
    with_tests(conn, root, changed, changes, findings)
}

/// `findings` plus V6, the tests that reach the change.
pub fn with_tests(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    findings: Vec<Finding>,
) -> Result<VerifyResult> {
    with_tests_until(conn, root, changed, changes, findings, None)
}

/// [`with_tests`], with the test walk cut short at `deadline`: the tests
/// found by then are listed.
pub fn with_tests_until(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    mut findings: Vec<Finding>,
    deadline: Option<std::time::Instant>,
) -> Result<VerifyResult> {
    let tests = tests_to_run(conn, root, changed, changes, deadline)?;
    for test in &tests {
        findings.push(Finding {
            id: "V6".to_string(),
            confidence: Confidence::High,
            blocking: false,
            message: "reaches the changed code; run it".to_string(),
            path: Some(test.clone()),
            line: None,
            suggestions: Vec::new(),
            fingerprint: String::new(),
            waived: None,
        });
    }
    Ok(VerifyResult::new(findings, tests))
}

/// V1–V5 without the test walk: what hooks run after every edit, against
/// `HEAD`.
pub fn verify_problems(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
) -> Result<VerifyResult> {
    let findings = problems(conn, root, changed, &Changes::WorkingTree)?
        .into_iter()
        .map(|p| p.finding)
        .collect();
    Ok(VerifyResult::new(findings, Vec::new()))
}

/// A finding with a key that names the problem without its line, and
/// without whether the change touched the file it is in: the same broken
/// call has one key as a V1 or a V3, before and after lines shift.
#[derive(Debug, Clone)]
pub struct Problem {
    pub finding: Finding,
    pub key: String,
}

/// V1–V5 over `changed`, keyed, with the repository's baseline and
/// accepted breaks applied.
pub fn problems(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
) -> Result<Vec<Problem>> {
    problems_with(conn, root, changed, changes, &Waivers::load(root))
}

/// [`problems`] with these waivers.
pub fn problems_with(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    waivers: &Waivers,
) -> Result<Vec<Problem>> {
    let rev = match changes {
        Changes::Since(rev) => rev.as_str(),
        _ => "HEAD",
    };
    let barrels = barrels_over(conn, changed)?;
    let mut out = Vec::new();
    missing_names(conn, changed, &barrels, &mut out)?;
    lost_imports(conn, root, changed, rev, &mut out)?;
    arity_mismatches(conn, changed, &mut out)?;
    broken_imports(conn, changed, &barrels, &mut out)?;
    // V5 reads every changed file at `rev`; only a client can lose what a
    // change removed, so an index without clients skips it.
    let clients: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM facts WHERE kind IN (?1, ?2, ?3))",
        params![kind::HTTP_CALL, kind::RPC_CALL, kind::CONSUMER],
        |row| row.get(0),
    )?;
    if clients {
        let mut contracts = Vec::new();
        contract_breaks(conn, root, changed, rev, &mut contracts)?;
        out.extend(contracts.into_iter().map(contract_problem));
    }
    for problem in &mut out {
        problem.finding.fingerprint = fingerprint(&problem.key);
        waivers.apply(&mut problem.finding);
    }
    Ok(out)
}

fn contract_problem(finding: Finding) -> Problem {
    let key = format!(
        "contract {} {}",
        finding.path.as_deref().unwrap_or("-"),
        finding.message
    );
    Problem { finding, key }
}

/// The keys of every problem the index holds now, whichever files a change
/// touches, leaving out rows that involve `skip`; V5 is checked over
/// `contract_paths` against `HEAD`. Hooks take this when a session begins,
/// so problems that were already there never block the agent.
pub fn problem_keys(
    conn: &Connection,
    root: &Path,
    skip: &HashSet<String>,
    contract_paths: &HashSet<String>,
) -> Result<Vec<String>> {
    let clear = |paths: &[&str]| !paths.iter().any(|p| skip.contains(*p));
    let skip_stems: HashSet<String> = skip.iter().map(|p| strip_extension(p)).collect();
    let mut keys = Vec::new();
    for row in missing_rows(conn)? {
        if clear(&[&row.path, row.missing_in.as_deref().unwrap_or_default()]) {
            keys.push(row.key());
        }
    }
    for row in arity_rows(conn)? {
        if clear(&[&row.path, &row.target_path]) {
            keys.push(row.key());
        }
    }
    for row in binding_rows(conn)? {
        let lexical = lexical_target(&row.path, &row.source).is_some_and(|candidate| {
            skip_stems.contains(&candidate) || skip_stems.contains(&format!("{candidate}/index"))
        });
        if !lexical && clear(&[&row.path, row.target.as_deref().unwrap_or_default()]) {
            keys.push(row.key());
        }
    }
    let mut lost = Vec::new();
    lost_imports(conn, root, contract_paths, "HEAD", &mut lost)?;
    keys.extend(lost.into_iter().map(|p| p.key));
    let mut contracts = Vec::new();
    contract_breaks(conn, root, contract_paths, "HEAD", &mut contracts)?;
    keys.extend(contracts.into_iter().map(|f| contract_problem(f).key));
    Ok(keys)
}

/// Provider fact kinds: what a server, a handler or a producer offers.
const PROVIDERS: &[&str] = &[kind::ROUTE, kind::RPC_IMPL, kind::RPC_DEF, kind::PRODUCER];

#[derive(Debug)]
struct Contracted {
    key: String,
    path: String,
    line: usize,
    confidence: String,
    /// The source line the fact came from.
    evidence: String,
    /// The link is exact, not a path parameter standing in for a literal
    /// segment (`/users/{}` reached by `/users/me`).
    exact: bool,
}

fn facts_where(
    conn: &Connection,
    sql: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Contracted>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT f.key, fl.path, f.line, f.confidence, f.evidence FROM facts f
         JOIN files fl ON fl.id = f.file_id WHERE {sql}"
    ))?;
    let rows = stmt.query_map(args, |row| {
        Ok(Contracted {
            key: row.get(0)?,
            path: row.get(1)?,
            line: row.get::<_, i64>(2)? as usize,
            confidence: row.get(3)?,
            evidence: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            exact: true,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// V5: the change removed a route, a gRPC method or a topic's producer
/// (present in a changed file at `rev`, gone from it now, and offered by no
/// other file) while a client still uses it.
fn contract_breaks(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    rev: &str,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    // Nothing can lose a client in an index without clients.
    let clients: i64 = conn.query_row(
        "SELECT COUNT(*) FROM facts WHERE kind IN (?1, ?2, ?3)",
        params![kind::HTTP_CALL, kind::RPC_CALL, kind::CONSUMER],
        |row| row.get(0),
    )?;
    if clients == 0 {
        return Ok(());
    }
    let candidates: Vec<&String> = changed
        .iter()
        .filter(|path| Language::from_path(Path::new(path.as_str())).is_some())
        .collect();
    let before = files_at(root, rev, &candidates);
    let mut removed: Vec<(crate::facts::IndexedFact, String)> = Vec::new();
    for path in candidates {
        let (Some(language), Some(before)) =
            (Language::from_path(Path::new(path)), before.get(path))
        else {
            continue;
        };
        let Ok(parsed) = crate::extract::parse_file_at(language, path, before) else {
            continue;
        };
        for fact in parsed
            .facts
            .into_iter()
            .filter(|f| PROVIDERS.contains(&f.kind))
        {
            if !still_offered(conn, &fact)? {
                removed.push((fact, path.clone()));
            }
        }
    }
    for (fact, from) in removed {
        let consumers = consumers_of(conn, &fact)?;
        let name = declared_name(conn, &fact)?;
        let (verb, user) = match fact.kind {
            kind::PRODUCER => ("produced", "listener"),
            _ => ("served", "call"),
        };
        // A route that now answers under a mounted router's prefix may
        // still be served; Tessera does not follow mounts yet.
        let mounted = fact.kind == kind::ROUTE
            && (served_under_a_prefix(conn, &fact)? || mounts_a_router(&root.join(&from)));
        for consumer in consumers {
            let high = fact.confidence == "high" && consumer.confidence == "high";
            let blocking = high && consumer.exact && !mounted && fact.kind != kind::PRODUCER;
            findings.push(Finding {
                id: "V5".to_string(),
                confidence: if high { Confidence::High } else { Confidence::Medium },
                blocking,
                message: format!(
                    "`{name}` is no longer {verb}: the change removed it from {from}, and this {user} still uses it"
                ),
                path: Some(consumer.path),
                line: Some(consumer.line),
                suggestions: Vec::new(),
                fingerprint: String::new(),
                waived: None,
            });
        }
    }
    Ok(())
}

/// A gRPC method whose server did not say which service it implements
/// (`*/GetQuote`) is named after the one `.proto` service that declares it.
fn declared_name(conn: &Connection, fact: &crate::facts::IndexedFact) -> Result<String> {
    let Some(method) = fact.key.strip_prefix("grpc */") else {
        return Ok(fact.name.clone());
    };
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT name FROM facts WHERE kind = ?1 AND key LIKE '%/' || ?2",
    )?;
    let names: Vec<String> = stmt
        .query_map(params![kind::RPC_DEF, method], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(match names.as_slice() {
        [name] => name.clone(),
        _ => fact.name.clone(),
    })
}

/// Whether something still offers what `fact` offered: the same key, or for
/// a route, the same path for its method or for any method.
fn still_offered(conn: &Connection, fact: &crate::facts::IndexedFact) -> Result<bool> {
    if !facts_where(conn, "f.key = ?1 AND f.kind = ?2", &[&fact.key, &fact.kind])?.is_empty() {
        return Ok(true);
    }
    let Some((method, path)) = route_parts(&fact.key).filter(|_| fact.kind == kind::ROUTE) else {
        return Ok(false);
    };
    let any = format!("http ANY {path}");
    let same = format!("http {method} {path}");
    let either = if method == "ANY" { "%" } else { "" };
    Ok(!facts_where(
        conn,
        "f.kind = ?1 AND (f.key = ?2 OR f.key = ?3 OR f.key LIKE 'http ' || ?4 || ' ' || ?5)",
        &[&kind::ROUTE, &any, &same, &either, &path],
    )?
    .is_empty())
}

/// `http GET /users/{}` → (`GET`, `/users/{}`).
fn route_parts(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix("http ")?.split_once(' ')
}

/// A route of the same method serves a trailing part of the removed path:
/// `GET /users` after `GET /api/users` moved into a router mounted at `/api`.
fn served_under_a_prefix(conn: &Connection, fact: &crate::facts::IndexedFact) -> Result<bool> {
    let Some((method, path)) = route_parts(&fact.key) else {
        return Ok(false);
    };
    let removed: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    Ok(facts_where(conn, "f.kind = ?1", &[&kind::ROUTE])?
        .iter()
        .filter_map(|route| route_parts(&route.key))
        .any(|(m, p)| {
            let segments: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
            (m == method || m == "ANY" || method == "ANY")
                && !segments.is_empty()
                && segments.len() < removed.len()
                && super::map::path_match(&removed[removed.len() - segments.len()..], &segments)
                    .is_some()
        }))
}

/// The file now mounts a router under a path (`app.use("/api", users)`),
/// so a route it no longer declares may live on under that prefix.
fn mounts_a_router(file: &Path) -> bool {
    static MOUNT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\.(use|register)\(\s*['"`]/"#).unwrap());
    std::fs::read_to_string(file).is_ok_and(|text| MOUNT.is_match(&text))
}

/// A request to another organisation's API: an absolute URL whose host has
/// a dot (`api.github.com`), unlike a service name (`http://cart:8080`).
fn external(evidence: &str) -> bool {
    let Some(at) = evidence.find("://") else {
        return false;
    };
    let host: String = evidence[at + 3..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    host.contains('.')
        && !host.starts_with("127.")
        && !host.ends_with(".local")
        && !host.ends_with(".internal")
        && host.chars().any(|c| c.is_ascii_alphabetic())
}

/// The clients of a removed provider fact.
fn consumers_of(conn: &Connection, fact: &crate::facts::IndexedFact) -> Result<Vec<Contracted>> {
    match fact.kind {
        kind::ROUTE => {
            let Some((method, path)) = fact
                .key
                .strip_prefix("http ")
                .and_then(|k| k.split_once(' '))
            else {
                return Ok(Vec::new());
            };
            let route: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let calls = facts_where(conn, "f.kind = ?1", &[&kind::HTTP_CALL])?;
            Ok(calls
                .into_iter()
                .filter(|call| !external(&call.evidence))
                .filter_map(|mut call| {
                    let (m, p) = call
                        .key
                        .strip_prefix("http ")
                        .and_then(|k| k.split_once(' '))?;
                    let segments: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
                    if !(m == method || m == "ANY" || method == "ANY") {
                        return None;
                    }
                    call.exact = super::map::path_match(&route, &segments)?;
                    Some(call)
                })
                .collect())
        }
        kind::PRODUCER => facts_where(
            conn,
            "f.kind = ?1 AND f.key = ?2",
            &[&kind::CONSUMER, &fact.key],
        ),
        _ => {
            let Some((service, method)) = fact
                .key
                .strip_prefix("grpc ")
                .and_then(|k| k.split_once('/'))
            else {
                return Ok(Vec::new());
            };
            let services: Vec<String> = if service == "*" {
                facts_where(
                    conn,
                    "f.kind = ?1 AND f.key LIKE '%/' || ?2",
                    &[&kind::RPC_DEF, &method],
                )?
                .into_iter()
                .filter_map(|d| {
                    d.key
                        .strip_prefix("grpc ")?
                        .split_once('/')
                        .map(|(s, _)| s.to_string())
                })
                .collect()
            } else {
                vec![service.to_string()]
            };
            let mut out = Vec::new();
            for service in services {
                let direct = format!("grpc {service}/{method}");
                let wildcard = format!("grpc */{method}");
                let client = format!("grpc {service}");
                out.extend(facts_where(
                    conn,
                    "f.kind = ?1 AND (f.key = ?2 OR (f.key = ?3 AND EXISTS (
                         SELECT 1 FROM facts c WHERE c.file_id = f.file_id AND c.kind = ?4 AND c.key = ?5)))",
                    &[&kind::RPC_CALL, &direct, &wildcard, &kind::RPC_CLIENT, &client],
                )?);
            }
            Ok(out)
        }
    }
}

/// `path` as it was at `rev`; `None` when it did not exist.
/// Each path's content at `rev`, read by one `git cat-file --batch`;
/// paths that did not exist then are left out.
pub(crate) fn files_at(root: &Path, rev: &str, paths: &[&String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if paths.is_empty() {
        return out;
    }
    let Ok(prefix) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-prefix"])
        .output()
    else {
        return out;
    };
    let prefix = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return out;
    };
    let request: String = paths
        .iter()
        .map(|path| format!("{rev}:{prefix}{path}\n"))
        .collect();
    let mut stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(request.as_bytes());
        }
    });
    let mut bytes = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut bytes);
    }
    let _ = writer.join();
    let _ = child.wait();
    let mut rest = bytes.as_slice();
    for path in paths {
        let Some(end) = rest.iter().position(|b| *b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        rest = &rest[end + 1..];
        let mut fields = header.split(' ');
        let (Some(_), Some(kind), Some(size)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(size) = size.parse::<usize>() else {
            continue;
        };
        if rest.len() < size {
            break;
        }
        if kind == "blob" {
            out.insert(
                path.to_string(),
                String::from_utf8_lossy(&rest[..size]).into_owned(),
            );
        }
        rest = rest.get(size + 1..).unwrap_or_default();
    }
    out
}

/// Deep-tier languages, whose high-confidence findings block.
fn blocks(path: &str) -> bool {
    let extension = path.rsplit('.').next().unwrap_or_default();
    Language::from_extension(extension).is_some_and(|l| {
        matches!(
            l,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Java
        )
    })
}

fn finding(id: &str, path: String, line: usize, message: String) -> Finding {
    let blocking = blocks(&path);
    let mut finding = Finding {
        id: id.to_string(),
        confidence: if blocking {
            Confidence::High
        } else {
            Confidence::Medium
        },
        blocking,
        message,
        path: Some(path),
        line: Some(line),
        suggestions: Vec::new(),
        fingerprint: String::new(),
        waived: None,
    };
    finding.fingerprint = finding.default_fingerprint();
    finding
}

struct MissingRow {
    path: String,
    line: usize,
    name: String,
    missing_in: Option<String>,
}

impl MissingRow {
    fn key(&self) -> String {
        format!(
            "missing {} {} {}",
            self.path,
            self.name,
            self.missing_in.as_deref().unwrap_or_default()
        )
    }
}

fn missing_rows(conn: &Connection) -> Result<Vec<MissingRow>> {
    let mut stmt = conn.prepare(
        "SELECT f.path, r.line, r.symbol_name, e.missing_in
         FROM edges e JOIN refs r ON r.id = e.ref_id JOIN files f ON f.id = r.file_id
         WHERE e.resolver = 'missing'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(MissingRow {
                path: row.get(0)?,
                line: row.get::<_, i64>(1)? as usize,
                name: row.get(2)?,
                missing_in: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Files that re-export, directly or through other barrels, from a changed
/// file: a name missing from one of them may be missing because of the
/// change.
fn barrels_over(conn: &Connection, changed: &HashSet<String>) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT f.path, b.target_path FROM import_bindings b
         JOIN files f ON f.id = b.file_id
         WHERE b.kind IN ('re_export', 're_export_all') AND b.target_path IS NOT NULL",
    )?;
    let mut by_target: HashMap<String, Vec<String>> = HashMap::new();
    for row in stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
        let (barrel, target): (String, String) = row?;
        by_target.entry(target).or_default().push(barrel);
    }
    let mut out = HashSet::new();
    let mut queue: Vec<&String> = changed.iter().collect();
    while let Some(target) = queue.pop() {
        for barrel in by_target.get(target).into_iter().flatten() {
            if !changed.contains(barrel) && out.insert(barrel.clone()) {
                queue.push(barrel);
            }
        }
    }
    Ok(out)
}

/// V1 (in changed files) and V3 (elsewhere, into a changed file or a barrel
/// over one): a name resolved into a repository module or class that does
/// not have it.
fn missing_names(
    conn: &Connection,
    changed: &HashSet<String>,
    barrels: &HashSet<String>,
    problems: &mut Vec<Problem>,
) -> Result<()> {
    for row in missing_rows(conn)? {
        let key = row.key();
        let MissingRow {
            path,
            line,
            name,
            missing_in,
        } = row;
        let missing_in = missing_in.unwrap_or_default();
        let finding = if changed.contains(&path) {
            let hint = legacy::validate_conn(conn, &name)?
                .candidates
                .first()
                .map(|s| format!("; did you mean {}?", s.qualified_name))
                .unwrap_or_default();
            finding(
                "V1",
                path,
                line,
                format!("`{name}` is not defined in {missing_in}{hint}"),
            )
        } else if changed.contains(&missing_in) {
            finding(
                "V3",
                path,
                line,
                format!("`{name}` no longer exists in {missing_in}, which this change edited"),
            )
        } else if barrels.contains(&missing_in) {
            finding(
                "V3",
                path,
                line,
                format!(
                    "`{name}` is no longer exported by {missing_in}, which re-exports a file this change edited"
                ),
            )
        } else {
            continue;
        };
        problems.push(Problem { finding, key });
    }
    Ok(())
}

/// V3 for a call left behind: a name the file imported from the repository
/// at `rev` and still calls, which now resolves to nothing (its import was
/// updated for a rename, this call site was not).
fn lost_imports(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    rev: &str,
    problems: &mut Vec<Problem>,
) -> Result<()> {
    let mut unbound = conn.prepare_cached(
        "SELECT r.line, r.symbol_name FROM refs r JOIN files f ON f.id = r.file_id
         WHERE f.path = ?1 AND r.receiver = 'none'
           AND NOT EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id
                           AND (e.confidence != 'name' OR e.resolver = 'missing'))
           AND NOT EXISTS (SELECT 1 FROM import_bindings b
                           WHERE b.file_id = r.file_id AND b.local = r.symbol_name)
           AND NOT EXISTS (SELECT 1 FROM symbols s
                           WHERE s.file_id = r.file_id AND s.name = r.symbol_name)",
    )?;
    let mut paths: Vec<&String> = changed.iter().filter(|p| blocks(p)).collect();
    paths.sort();
    let mut candidates = Vec::new();
    for path in paths {
        let Some(language) = Language::from_path(Path::new(path)) else {
            continue;
        };
        let refs: Vec<(usize, String)> = unbound
            .query_map(params![path], |row| {
                Ok((row.get::<_, i64>(0)? as usize, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|(_, name)| !known_global(language, name))
            .collect();
        if !refs.is_empty() {
            candidates.push((path.clone(), language, refs));
        }
    }
    if candidates.is_empty() {
        return Ok(());
    }
    let wanted: Vec<String> = candidates.iter().map(|c| c.0.clone()).collect();
    let before = files_at(root, rev, &wanted.iter().collect::<Vec<_>>());
    let mut in_repo = conn.prepare_cached(
        "SELECT EXISTS (SELECT 1 FROM import_bindings WHERE source = ?1 AND target_path IS NOT NULL)",
    )?;
    for (path, language, refs) in candidates {
        let Some(old) = before.get(&path) else {
            continue;
        };
        let Ok(parsed) = parse_file(language, old) else {
            continue;
        };
        let imported: HashMap<String, String> = parsed
            .bindings
            .into_iter()
            .map(|b| (b.local, b.source))
            .collect();
        for (line, name) in refs {
            let Some(source) = imported.get(&name) else {
                continue;
            };
            if !source.starts_with('.') && !in_repo.query_row(params![source], |row| row.get(0))? {
                continue;
            }
            problems.push(Problem {
                finding: finding(
                    "V3",
                    path.clone(),
                    line,
                    format!(
                        "`{name}` was imported from {source} before this change and now resolves to nothing"
                    ),
                ),
                key: format!("lost {path} {name} {source}"),
            });
        }
    }
    Ok(())
}

struct ArityRow {
    path: String,
    line: usize,
    args: Option<i64>,
    target: String,
    min: Option<i64>,
    max: Option<i64>,
    target_path: String,
}

impl ArityRow {
    fn key(&self) -> String {
        format!(
            "arity {} {} {} {}",
            self.path,
            self.target,
            self.target_path,
            self.args.unwrap_or_default()
        )
    }
}

fn arity_rows(conn: &Connection) -> Result<Vec<ArityRow>> {
    let mut stmt = conn.prepare(
        "SELECT f.path, r.line, r.arg_count, ts.qualified_name, ts.arity_min, ts.arity_max, tf.path
         FROM edges e JOIN refs r ON r.id = e.ref_id JOIN files f ON f.id = r.file_id
         JOIN symbols ts ON ts.id = e.to_symbol_id JOIN files tf ON tf.id = ts.file_id
         WHERE e.arity_mismatch = 1 AND e.confidence = 'high'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ArityRow {
                path: row.get(0)?,
                line: row.get::<_, i64>(1)? as usize,
                args: row.get(2)?,
                target: row.get(3)?,
                min: row.get(4)?,
                max: row.get(5)?,
                target_path: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// V2: a resolved call passes an argument count its target cannot take, where
/// the call or the target changed.
fn arity_mismatches(
    conn: &Connection,
    changed: &HashSet<String>,
    problems: &mut Vec<Problem>,
) -> Result<()> {
    for row in arity_rows(conn)? {
        if !changed.contains(&row.path) && !changed.contains(&row.target_path) {
            continue;
        }
        let key = row.key();
        let expected = match (row.min, row.max) {
            (Some(min), Some(max)) if min == max => min.to_string(),
            (Some(min), Some(max)) => format!("{min} to {max}"),
            (Some(min), None) => format!("at least {min}"),
            _ => "a different number of".to_string(),
        };
        let finding = finding(
            "V2",
            row.path,
            row.line,
            format!(
                "`{}` takes {expected} arguments; this call passes {}",
                row.target,
                row.args.unwrap_or_default()
            ),
        );
        problems.push(Problem { finding, key });
    }
    Ok(())
}

struct BindingRow {
    path: String,
    line: usize,
    local: String,
    imported: Option<String>,
    source: String,
    status: String,
    target: Option<String>,
    kind: String,
}

impl BindingRow {
    fn key(&self) -> String {
        format!(
            "import {} {} {} {}",
            self.path,
            self.imported.as_deref().unwrap_or(&self.local),
            self.source,
            self.status
        )
    }
}

fn binding_rows(conn: &Connection) -> Result<Vec<BindingRow>> {
    let mut stmt = conn.prepare(
        "SELECT f.path, b.line, b.local, b.imported, b.source, b.status, b.target_path, b.kind
         FROM import_bindings b JOIN files f ON f.id = b.file_id
         WHERE b.status != 'ok'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(BindingRow {
                path: row.get(0)?,
                line: row.get::<_, i64>(1)? as usize,
                local: row.get(2)?,
                imported: row.get(3)?,
                source: row.get(4)?,
                status: row.get(5)?,
                target: row.get(6)?,
                kind: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// V4: an import whose local module is gone or does not export the name, in a
/// changed file or pointing at one.
fn broken_imports(
    conn: &Connection,
    changed: &HashSet<String>,
    barrels: &HashSet<String>,
    problems: &mut Vec<Problem>,
) -> Result<()> {
    let changed_stems: HashSet<String> = changed.iter().map(|p| strip_extension(p)).collect();
    for row in binding_rows(conn)? {
        let points_at_change = row
            .target
            .as_ref()
            .is_some_and(|t| changed.contains(t) || barrels.contains(t))
            || lexical_target(&row.path, &row.source).is_some_and(|candidate| {
                changed_stems.contains(&candidate)
                    || changed_stems.contains(&format!("{candidate}/index"))
            });
        if !changed.contains(&row.path) && !points_at_change {
            continue;
        }
        let key = row.key();
        let message = if row.status == "missing_module" {
            format!("`{}` does not resolve to a file", row.source)
        } else if row.kind.starts_with("java") {
            format!("`{}` is not a class in this repository", row.source)
        } else {
            format!(
                "`{}` is not exported by {}",
                row.imported.as_deref().unwrap_or(&row.local),
                row.source
            )
        };
        problems.push(Problem {
            finding: finding("V4", row.path, row.line, message),
            key,
        });
    }
    Ok(())
}

fn strip_extension(path: &str) -> String {
    match path.rsplit_once('.') {
        Some((stem, ext)) if !ext.contains('/') => stem.trim_end_matches(".d").to_string(),
        _ => path.to_string(),
    }
}

/// `./x` from `a/b.ts` is `a/x`; `None` for package specifiers.
fn lexical_target(from: &str, source: &str) -> Option<String> {
    if !source.starts_with('.') {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for segment in source.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(strip_extension(&parts.join("/")))
}

/// A test file a runner can run: source code under a test path, not the
/// fixtures, snapshots or data files the tests read.
pub(crate) fn runnable_test(path: &str) -> bool {
    let data = [
        "fixtures",
        "fixture",
        "testdata",
        "snapshots",
        "__snapshots__",
    ];
    is_test_path(path)
        && !path.split('/').any(|segment| data.contains(&segment))
        && Language::from_path(Path::new(path)).is_some_and(|l| !l.is_data())
}

/// V6: test files that reach a changed symbol, plus changed test files.
fn tests_to_run(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<String>> {
    let mut tests: BTreeSet<String> = changed
        .iter()
        .filter(|p| runnable_test(p))
        .cloned()
        .collect();
    let rev = match changes {
        Changes::Since(rev) => rev.as_str(),
        _ => "HEAD",
    };
    let mut seeds: Vec<i64> = legacy::diff_changed_symbols(conn, root, rev, None)
        .map(|(_, symbols)| symbols.into_iter().map(|s| s.id).collect())
        .unwrap_or_default();
    let mut in_file = conn
        .prepare("SELECT s.id FROM symbols s JOIN files f ON f.id = s.file_id WHERE f.path = ?1")?;
    let untracked = untracked(root);
    let mut new_files: Vec<&String> = changed.iter().filter(|p| untracked.contains(*p)).collect();
    new_files.sort();
    for path in new_files {
        for id in in_file.query_map(params![path], |row| row.get(0))? {
            seeds.push(id?);
        }
    }
    seeds.sort_unstable();
    seeds.dedup();
    seeds.truncate(TEST_SEEDS);
    for seed in seeds {
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            break;
        }
        for test in graph::tests_reaching(conn, seed, TEST_DEPTH, TEST_SCAN_CAP)? {
            if runnable_test(&test.path) {
                tests.insert(test.path);
            }
        }
    }
    Ok(tests.into_iter().collect())
}

/// Files git does not track and does not ignore, from one git process.
pub(crate) fn untracked(root: &Path) -> HashSet<String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "core.quotePath=false",
            "ls-files",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().replace('\\', "/"))
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Repository-relative paths the change touches, deleted files included.
pub fn changed_paths(root: &Path, changes: &Changes) -> Result<HashSet<String>> {
    let git = |args: &[&str]| -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["-c", "core.quotePath=false"])
            .args(args)
            .output()
            .map_err(|e| anyhow!("git failed: {e}"))?;
        if !out.status.success() {
            bail!(
                "verify needs a git repository: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let mut paths = String::new();
    match changes {
        Changes::WorkingTree => {
            paths.push_str(&git(&[
                "diff",
                "--name-only",
                "--relative",
                "--no-renames",
                "HEAD",
            ])?);
            paths.push_str(&git(&["ls-files", "--others", "--exclude-standard"])?);
        }
        Changes::Staged => {
            paths.push_str(&git(&[
                "diff",
                "--name-only",
                "--relative",
                "--no-renames",
                "--cached",
            ])?);
        }
        Changes::Since(rev) => {
            if rev.starts_with('-') {
                bail!("invalid git revision {rev:?}");
            }
            let commit = git(&[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{rev}^{{commit}}"),
            ])?;
            paths.push_str(&git(&[
                "diff",
                "--name-only",
                "--relative",
                "--no-renames",
                commit.trim(),
            ])?);
            paths.push_str(&git(&["ls-files", "--others", "--exclude-standard"])?);
        }
    }
    Ok(paths
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.replace('\\', "/"))
        .collect())
}

impl fmt::Display for VerifyResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for VerifyResult {
    fn agent_text(&self) -> String {
        let problems: Vec<&Finding> = self.findings.iter().filter(|f| f.id != "V6").collect();
        let mut out = if problems.is_empty() {
            "✓ verify: no problems found.\n".to_string()
        } else {
            let blocking = self.blocking();
            let mut out = format!("✗ verify: {} problem(s)", problems.len());
            if blocking < problems.len() {
                out.push_str(&format!(", {blocking} blocking"));
            }
            out.push('\n');
            out
        };
        for finding in &problems {
            let at = match (&finding.path, finding.line) {
                (Some(path), Some(line)) => format!(" {path}:{line}"),
                (None, Some(line)) => format!(" (line {line})"),
                _ => String::new(),
            };
            let advisory = match &finding.waived {
                Some(w) if w.expired => format!(
                    " (accepted until {}, now expired)",
                    w.expires.as_deref().unwrap_or("?")
                ),
                Some(w) if w.by == "baseline" => " (in the baseline)".to_string(),
                Some(w) => format!(" (accepted: {})", w.reason.as_deref().unwrap_or("")),
                None if finding.blocking => String::new(),
                None => " (advisory)".to_string(),
            };
            out.push_str(&format!(
                "  [{}]{at} {}{advisory}\n",
                finding.id, finding.message
            ));
        }
        if !self.tests.is_empty() {
            out.push_str(&format!("Tests to run: {}\n", self.tests.join(", ")));
        }
        out
    }
}

impl VerifyResult {
    /// One line per blocking problem, for hook output and agents.
    pub fn agent_brief(&self) -> String {
        let problems: Vec<&Finding> = self.findings.iter().filter(|f| f.blocking).collect();
        if problems.is_empty() {
            return "tessera verify: ok".to_string();
        }
        let mut out = format!("tessera verify: {} problem(s) to fix", problems.len());
        for finding in problems.iter().take(AGENT_MAX_FINDINGS) {
            let at = match (&finding.path, finding.line) {
                (Some(path), Some(line)) => format!("{path}:{line} "),
                _ => String::new(),
            };
            out.push_str(&format!("\n{} {at}{}", finding.id, finding.message));
        }
        if problems.len() > AGENT_MAX_FINDINGS {
            out.push_str(&format!(
                "\n… and {} more",
                problems.len() - AGENT_MAX_FINDINGS
            ));
        }
        out
    }

    /// SARIF 2.1.0 for code-scanning uploads.
    pub fn sarif(&self) -> serde_json::Value {
        let rules = [
            ("V1", "Call to a name that does not exist where it points"),
            ("V2", "Argument count does not match the target"),
            ("V3", "Reference to a name the change removed"),
            ("V4", "Import of a missing module or export"),
            (
                "V5",
                "Use of a route, gRPC method or topic the change removed",
            ),
            ("V6", "Test that reaches the changed code"),
        ];
        let ids = super::formats::occurrence_ids(&self.findings);
        let results: Vec<serde_json::Value> = self
            .findings
            .iter()
            .zip(ids)
            .map(|(f, id)| {
                let level = if f.blocking {
                    "error"
                } else if f.id == "V6" {
                    "note"
                } else {
                    "warning"
                };
                serde_json::json!({
                    "ruleId": f.id,
                    "level": level,
                    "message": { "text": f.message },
                    "partialFingerprints": { "tesseraFingerprint/v1": id },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": super::formats::uri(f.path.as_deref().unwrap_or_default())
                            },
                            "region": { "startLine": f.line.unwrap_or(1) }
                        }
                    }]
                })
            })
            .collect();
        let rules: Vec<serde_json::Value> = rules
            .iter()
            .map(|(id, text)| serde_json::json!({ "id": id, "shortDescription": { "text": text } }))
            .collect();
        serde_json::json!({
            "version": "2.1.0",
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "tessera",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/iamsaquib8/tessera",
                        "rules": rules
                    }
                },
                "results": results
            }]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_targets_follow_relative_specifiers() {
        assert_eq!(
            lexical_target("src/users/repo.ts", "../util/format").as_deref(),
            Some("src/util/format")
        );
        assert_eq!(
            lexical_target("src/a.ts", "./b.js").as_deref(),
            Some("src/b")
        );
        assert_eq!(lexical_target("src/a.ts", "react"), None);
    }
}
