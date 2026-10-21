use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::common::{is_test_path, known_global};
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
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub ok: bool,
    pub findings: Vec<Finding>,
    /// Test files that reach the changed code (V6).
    pub tests: Vec<String>,
}

impl VerifyResult {
    fn new(mut findings: Vec<Finding>, tests: Vec<String>) -> Self {
        findings.sort_by(|a, b| {
            b.blocking
                .cmp(&a.blocking)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });
        findings.dedup_by(|a, b| a.id == b.id && a.path == b.path && a.line == b.line);
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
        return verify_changes(conn, root, &changes);
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
    let changed = changed_paths(root, changes)?;
    verify_paths(conn, root, &changed, changes)
}

/// The same checks for an explicit set of changed files (hooks pass the
/// files an agent edited).
pub fn verify_paths(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
) -> Result<VerifyResult> {
    let rev = match changes {
        Changes::Since(rev) => rev.as_str(),
        _ => "HEAD",
    };
    let mut findings = problem_findings(conn, root, changed, rev)?;
    let tests = tests_to_run(conn, root, changed, changes)?;
    for test in &tests {
        findings.push(Finding {
            id: "V6".to_string(),
            confidence: Confidence::High,
            blocking: false,
            message: "reaches the changed code; run it".to_string(),
            path: Some(test.clone()),
            line: None,
            suggestions: Vec::new(),
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
    Ok(VerifyResult::new(
        problem_findings(conn, root, changed, "HEAD")?,
        Vec::new(),
    ))
}

fn problem_findings(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    rev: &str,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    missing_names(conn, changed, &mut findings)?;
    arity_mismatches(conn, changed, &mut findings)?;
    broken_imports(conn, changed, &mut findings)?;
    contract_breaks(conn, root, changed, rev, &mut findings)?;
    Ok(findings)
}

/// Provider fact kinds: what a server, a handler or a producer offers.
const PROVIDERS: &[&str] = &[kind::ROUTE, kind::RPC_IMPL, kind::RPC_DEF, kind::PRODUCER];

#[derive(Debug)]
struct Contracted {
    key: String,
    path: String,
    line: usize,
    confidence: String,
}

fn facts_where(
    conn: &Connection,
    sql: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Contracted>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT f.key, fl.path, f.line, f.confidence FROM facts f
         JOIN files fl ON fl.id = f.file_id WHERE {sql}"
    ))?;
    let rows = stmt.query_map(args, |row| {
        Ok(Contracted {
            key: row.get(0)?,
            path: row.get(1)?,
            line: row.get::<_, i64>(2)? as usize,
            confidence: row.get(3)?,
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
    let mut removed: Vec<(crate::facts::IndexedFact, String)> = Vec::new();
    for path in changed {
        let Some(language) = Language::from_path(Path::new(path)) else {
            continue;
        };
        let Some(before) = file_at(root, rev, path) else {
            continue;
        };
        let Ok(parsed) = crate::extract::parse_file_at(language, path, &before) else {
            continue;
        };
        let old: Vec<crate::facts::IndexedFact> = parsed
            .facts
            .into_iter()
            .filter(|f| PROVIDERS.contains(&f.kind))
            .collect();
        if old.is_empty() {
            continue;
        }
        for fact in old {
            let offered =
                facts_where(conn, "f.key = ?1 AND f.kind = ?2", &[&fact.key, &fact.kind])?;
            if offered.is_empty() {
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
        for consumer in consumers {
            let high = fact.confidence == "high" && consumer.confidence == "high";
            let blocking = high && fact.kind != kind::PRODUCER;
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
                .filter(|call| {
                    let Some((m, p)) = call
                        .key
                        .strip_prefix("http ")
                        .and_then(|k| k.split_once(' '))
                    else {
                        return false;
                    };
                    let segments: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
                    (m == method || m == "ANY" || method == "ANY")
                        && super::map::path_match(&route, &segments).is_some()
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
fn file_at(root: &Path, rev: &str, path: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("{rev}:./{path}")])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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
    Finding {
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
    }
}

/// V1 (in changed files) and V3 (elsewhere, into a changed file): a name
/// resolved into a repository module or class that does not have it.
fn missing_names(
    conn: &Connection,
    changed: &HashSet<String>,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT f.path, r.line, r.symbol_name, e.missing_in
         FROM edges e JOIN refs r ON r.id = e.ref_id JOIN files f ON f.id = r.file_id
         WHERE e.resolver = 'missing'",
    )?;
    let rows: Vec<(String, usize, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get(2)?,
                row.get(3)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (path, line, name, missing_in) in rows {
        let missing_in = missing_in.unwrap_or_default();
        if changed.contains(&path) {
            let hint = legacy::validate_conn(conn, &name)?
                .candidates
                .first()
                .map(|s| format!("; did you mean {}?", s.qualified_name))
                .unwrap_or_default();
            findings.push(finding(
                "V1",
                path,
                line,
                format!("`{name}` is not defined in {missing_in}{hint}"),
            ));
        } else if changed.contains(&missing_in) {
            findings.push(finding(
                "V3",
                path,
                line,
                format!("`{name}` no longer exists in {missing_in}, which this change edited"),
            ));
        }
    }
    Ok(())
}

/// V2: a resolved call passes an argument count its target cannot take, where
/// the call or the target changed.
fn arity_mismatches(
    conn: &Connection,
    changed: &HashSet<String>,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    type Row = (
        String,
        usize,
        Option<i64>,
        String,
        Option<i64>,
        Option<i64>,
        String,
    );
    let mut stmt = conn.prepare(
        "SELECT f.path, r.line, r.arg_count, ts.qualified_name, ts.arity_min, ts.arity_max, tf.path
         FROM edges e JOIN refs r ON r.id = e.ref_id JOIN files f ON f.id = r.file_id
         JOIN symbols ts ON ts.id = e.to_symbol_id JOIN files tf ON tf.id = ts.file_id
         WHERE e.arity_mismatch = 1 AND e.confidence = 'high'",
    )?;
    let rows: Vec<Row> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (path, line, args, target, min, max, target_path) in rows {
        if !changed.contains(&path) && !changed.contains(&target_path) {
            continue;
        }
        let expected = match (min, max) {
            (Some(min), Some(max)) if min == max => min.to_string(),
            (Some(min), Some(max)) => format!("{min} to {max}"),
            (Some(min), None) => format!("at least {min}"),
            _ => "a different number of".to_string(),
        };
        findings.push(finding(
            "V2",
            path,
            line,
            format!(
                "`{target}` takes {expected} arguments; this call passes {}",
                args.unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

/// V4: an import whose local module is gone or does not export the name, in a
/// changed file or pointing at one.
fn broken_imports(
    conn: &Connection,
    changed: &HashSet<String>,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    type Row = (
        String,
        usize,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
    );
    let changed_stems: HashSet<String> = changed.iter().map(|p| strip_extension(p)).collect();
    let mut stmt = conn.prepare(
        "SELECT f.path, b.line, b.local, b.imported, b.source, b.status, b.target_path
         FROM import_bindings b JOIN files f ON f.id = b.file_id
         WHERE b.status != 'ok'",
    )?;
    let rows: Vec<Row> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (path, line, local, imported, source, status, target) in rows {
        let points_at_change = target.as_ref().is_some_and(|t| changed.contains(t))
            || lexical_target(&path, &source).is_some_and(|candidate| {
                changed_stems.contains(&candidate)
                    || changed_stems.contains(&format!("{candidate}/index"))
            });
        if !changed.contains(&path) && !points_at_change {
            continue;
        }
        let message = if status == "missing_module" {
            format!("`{source}` does not resolve to a file")
        } else {
            format!(
                "`{}` is not exported by {source}",
                imported.unwrap_or(local)
            )
        };
        findings.push(finding("V4", path, line, message));
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

/// V6: test files that reach a changed symbol, plus changed test files.
fn tests_to_run(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
) -> Result<Vec<String>> {
    let mut tests: BTreeSet<String> = changed
        .iter()
        .filter(|p| is_test_path(p))
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
    for path in changed {
        if !tracked(root, path) {
            for id in in_file.query_map(params![path], |row| row.get(0))? {
                seeds.push(id?);
            }
        }
    }
    seeds.sort_unstable();
    seeds.dedup();
    for seed in seeds {
        for test in graph::tests_reaching(conn, seed, TEST_DEPTH, TEST_SCAN_CAP)? {
            tests.insert(test.path);
        }
    }
    Ok(tests.into_iter().collect())
}

fn tracked(root: &Path, path: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--error-unmatch", "--", path])
        .output()
        .is_ok_and(|out| out.status.success())
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
            paths.push_str(&git(&["diff", "--name-only", "--relative", "HEAD"])?);
            paths.push_str(&git(&["ls-files", "--others", "--exclude-standard"])?);
        }
        Changes::Staged => {
            paths.push_str(&git(&["diff", "--name-only", "--relative", "--cached"])?);
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
            paths.push_str(&git(&["diff", "--name-only", "--relative", commit.trim()])?);
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
            let advisory = if finding.blocking { "" } else { " (advisory)" };
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
        let results: Vec<serde_json::Value> = self
            .findings
            .iter()
            .map(|f| {
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
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": f.path.clone().unwrap_or_default() },
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
