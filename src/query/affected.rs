//! `affected`: the test files a change reaches, each with the path from the
//! changed code to it, and the rails that widen the selection. New, changed
//! and previously failed tests always run; a lockfile, build file, test or CI
//! configuration, or fixture change, a `Tessera-No-Skip` commit trailer, or
//! a change too wide to follow runs every test.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::common::is_test_path;
use super::find::page_footer;
use super::verify::{changed_paths, files_at, runnable_test, untracked, Changes};
use super::{graph, legacy, map};
use crate::output::{paginate, AgentText, Page};
use crate::types::Language;

/// How many calls or contracts a test may be from the changed code.
pub const DEFAULT_DEPTH: usize = 6;
/// A change reaching more symbols than this runs every test: the walk stops
/// there, and a partial selection could drop a test that should run.
const MAX_VISITED: usize = 20_000;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AffectedArgs {
    /// Compare the working tree with this revision; `None` for `HEAD`.
    pub rev: Option<String>,
    /// Select for the staged changes instead.
    pub staged: bool,
    /// Tests that failed in an earlier run; they always run.
    pub failed: Vec<String>,
    /// A test path to explain.
    pub explain: Option<String>,
    /// Only tests in this workspace package.
    pub package: Option<String>,
    pub depth: usize,
    /// Tests per page; 0 for all of them.
    pub limit: usize,
    pub cursor: Option<String>,
}

/// A request `affected` cannot answer for a reason the caller can fix: no
/// git repository, an unknown revision or package.
#[derive(Debug)]
pub struct AffectedUsage(pub String);

impl fmt::Display for AffectedUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AffectedUsage {}

fn usage(message: String) -> anyhow::Error {
    AffectedUsage(message).into()
}

/// Why a test runs, strongest first: a test selected two ways keeps the
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// New, or edited by the change.
    Changed,
    /// Failed in an earlier run.
    Always,
    /// Reaches the changed code.
    Reached,
    /// Runs because the whole suite does.
    FullRun,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Changed => "changed",
            Kind::Always => "always",
            Kind::Reached => "reached",
            Kind::FullRun => "full-run",
        })
    }
}

/// How a step of a chain relates to the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Link {
    /// The first step: the test's own code.
    Test,
    /// The previous step calls or references this one.
    Calls,
    /// The previous step calls this one through the contract `via`.
    Contract,
    /// This step, the changed code, calls the previous one through the
    /// contract `via`: a changed client selects the tests of the service it
    /// calls.
    Client,
    /// The previous step uses this name, which the change removed.
    Removed,
    /// This contract, declared in a changed definition file, is the one the
    /// previous step serves or calls.
    Definition,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub symbol: String,
    pub path: String,
    pub line: Option<usize>,
    pub link: Link,
    /// The contract a `contract` or `client` link crosses.
    pub via: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AffectedTest {
    pub path: String,
    pub kind: Kind,
    /// The workspace package the test belongs to.
    pub package: Option<String>,
    pub reason: String,
    /// The changed symbol a reached test depends on.
    pub changed: Option<String>,
    /// From the test's own code to the changed symbol.
    pub chain: Vec<Step>,
    /// The test's symbols that reach the change, for runners that take test
    /// names.
    pub functions: Vec<String>,
    /// The directory of the manifest naming `package`.
    #[serde(skip)]
    manifest_dir: Option<String>,
    /// Reached from code outside any test function, or through a fixture:
    /// the whole file runs.
    #[serde(skip)]
    whole: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub selected: usize,
    /// Every test file in the index (in the package, with `--package`).
    pub total: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Explanation {
    pub test: String,
    pub selected: bool,
    pub kind: Option<Kind>,
    pub reason: String,
}

impl Explanation {
    pub fn line(&self) -> String {
        match self.kind.filter(|_| self.selected) {
            Some(kind) => format!("{} is selected ({kind}): {}", self.test, self.reason),
            None => format!("{} is not selected: {}", self.test, self.reason),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AffectedResult {
    /// The revision the working tree is compared with.
    pub rev: String,
    pub staged: bool,
    pub depth: usize,
    /// The selected tests, by path.
    pub tests: Vec<AffectedTest>,
    pub totals: Totals,
    /// Why every test runs, when one does.
    pub full_run: Option<String>,
    pub changed_symbols: Vec<String>,
    pub notes: Vec<String>,
    pub explain: Option<Explanation>,
    pub page: Page,
}

pub fn run(conn: &Connection, root: &Path, args: &AffectedArgs) -> Result<AffectedResult> {
    let depth = if args.depth == 0 {
        DEFAULT_DEPTH
    } else {
        args.depth
    };
    let (changes, base) = changes(root, args)?;
    let changed = changed_paths(root, &changes)?;
    let mut packages = Packages::new(root);
    if let Some(name) = &args.package {
        packages.check(conn, name)?;
    }
    let suite = suite(conn)?;
    let mut notes = Vec::new();
    if suite.is_empty() {
        notes.push("no test files in the index; nothing to select".to_string());
    }
    let seeds = seeds(conn, root, &changed, &changes)?;
    let mut walk = Walk::new(conn, root)?;
    walk.run(&seeds.seeds, depth)?;

    // Strongest reason first: a test selected two ways keeps the first.
    let mut chosen: BTreeMap<String, AffectedTest> = BTreeMap::new();
    for test in changed_tests(root, &changed, &changes, &base, &seeds, &mut walk)? {
        choose(&mut chosen, test);
    }
    for line in &args.failed {
        let Some(path) = normalize(root, line) else {
            continue;
        };
        if suite.contains(&path) || root.join(&path).is_file() {
            let test = plain(&path, Kind::Always, "failed in an earlier run");
            choose(&mut chosen, test);
        } else {
            notes.push(format!(
                "the failed list names {path}, which is not in the repository"
            ));
        }
    }
    for test in reached_tests(&seeds, &mut walk, &suite)? {
        choose(&mut chosen, test);
    }
    let full_run = full_run(root, &changed, &changes, &base, walk.capped);
    if let Some(reason) = &full_run {
        for path in &suite {
            choose(&mut chosen, plain(path, Kind::FullRun, reason));
        }
    }

    let mut tests = Vec::new();
    for (path, mut test) in chosen {
        let found = packages.find(&path);
        test.package = found.as_ref().map(|(_, name)| name.clone());
        test.manifest_dir = found.map(|(dir, _)| dir);
        if args.package.is_none() || test.package == args.package {
            tests.push(test);
        }
    }
    // The suite in scope, plus selected tests the index does not hold.
    let mut all: BTreeSet<String> = tests.iter().map(|t| t.path.clone()).collect();
    for path in &suite {
        let in_scope = match &args.package {
            None => true,
            Some(name) => packages.find(path).is_some_and(|(_, n)| &n == name),
        };
        if in_scope {
            all.insert(path.clone());
        }
    }
    let explain = args.explain.as_deref().map(|test| {
        let test = normalize(root, test).unwrap_or_else(|| test.to_string());
        explanation(test, &tests, &suite, &all, &mut packages, args, depth)
    });

    let totals = Totals {
        selected: tests.len(),
        total: all.len(),
        skipped: all.len() - tests.len(),
    };
    let limit = if args.limit == 0 {
        tests.len().max(1)
    } else {
        args.limit
    };
    let (tests, page) = paginate(tests, limit, args.cursor.as_deref())?;
    Ok(AffectedResult {
        rev: args.rev.clone().unwrap_or_else(|| "HEAD".to_string()),
        staged: args.staged,
        depth,
        tests,
        totals,
        full_run,
        changed_symbols: seeds.labels.into_iter().collect(),
        notes,
        explain,
        page,
    })
}

fn choose(chosen: &mut BTreeMap<String, AffectedTest>, test: AffectedTest) {
    chosen.entry(test.path.clone()).or_insert(test);
}

/// Test files the change adds or edits, and Rust `tests` modules whose own
/// lines it edits.
fn changed_tests(
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    base: &str,
    seeds: &Seeds,
    walk: &mut Walk<'_>,
) -> Result<Vec<AffectedTest>> {
    let added = added(root, changes, base);
    let mut paths: Vec<&String> = changed
        .iter()
        .filter(|p| is_test_file(p) && root.join(p).is_file())
        .collect();
    paths.sort();
    let mut out: Vec<AffectedTest> = paths
        .into_iter()
        .map(|path| {
            let reason = if added.contains(path) {
                "a new test"
            } else {
                "the change edits this test"
            };
            plain(path, Kind::Changed, reason)
        })
        .collect();
    for seed in seeds.seeds.iter().filter(|s| s.tail.is_none()) {
        if let Some(symbol) = walk.symbol(seed.id)? {
            if is_test_symbol(&symbol) && root.join(&symbol.path).is_file() {
                out.push(plain(
                    &symbol.path,
                    Kind::Changed,
                    "the change edits this test",
                ));
            }
        }
    }
    Ok(out)
}

/// Test files the walk reached, each with the chain of its first (shortest)
/// hit; and every test under a reached `conftest.py`, whose fixtures tests
/// take as parameters rather than call.
fn reached_tests(
    seeds: &Seeds,
    walk: &mut Walk<'_>,
    suite: &BTreeSet<String>,
) -> Result<Vec<AffectedTest>> {
    let mut out: Vec<AffectedTest> = seeds
        .direct
        .iter()
        .map(|(path, chain)| reached(path, chain.clone(), Vec::new(), true))
        .collect();
    for (path, hits) in std::mem::take(&mut walk.hits) {
        let chain = match &hits[0] {
            Hit::Symbol(id) => walk.chain(*id)?,
            Hit::TopLevel { line, below } => {
                let mut rest = walk.chain(*below)?;
                rest[0].link = Link::Calls;
                let mut chain = vec![top_level(&path, *line)];
                chain.extend(rest);
                chain
            }
        };
        let mut functions = BTreeSet::new();
        for hit in &hits {
            if let Hit::Symbol(id) = hit {
                functions.extend(walk.symbol(*id)?.map(|s| s.name));
            }
        }
        let whole = hits.iter().any(|h| matches!(h, Hit::TopLevel { .. }));
        out.push(reached(
            &path,
            chain,
            functions.into_iter().collect(),
            whole,
        ));
    }
    for id in walk.conftests.clone() {
        let chain = walk.chain(id)?;
        let conftest = chain[0].path.clone();
        let dir = parent(&conftest);
        let under = |p: &&String| dir.is_empty() || p.starts_with(&format!("{dir}/"));
        for path in suite.iter().filter(under) {
            let mut test = reached(path, chain.clone(), Vec::new(), true);
            test.reason = format!("uses fixtures from {conftest}: {}", test.reason);
            out.push(test);
        }
    }
    Ok(out)
}

/// Why `test` is or is not in `tests`.
fn explanation(
    test: String,
    tests: &[AffectedTest],
    suite: &BTreeSet<String>,
    in_scope: &BTreeSet<String>,
    packages: &mut Packages,
    args: &AffectedArgs,
    depth: usize,
) -> Explanation {
    if let Some(found) = tests.iter().find(|t| t.path == test) {
        return Explanation {
            test,
            selected: true,
            kind: Some(found.kind),
            reason: found.reason.clone(),
        };
    }
    let reason = if !suite.contains(&test) {
        "it is not a test file in the index".to_string()
    } else if !in_scope.contains(&test) {
        let package = packages.find(&test).map(|(_, name)| name);
        format!(
            "it is in package {}, not {}",
            package.as_deref().unwrap_or("(none)"),
            args.package.as_deref().unwrap_or_default()
        )
    } else {
        format!("no path from the changed symbols within depth {depth}")
    };
    Explanation {
        test,
        selected: false,
        kind: None,
        reason,
    }
}

fn plain(path: &str, kind: Kind, reason: &str) -> AffectedTest {
    AffectedTest {
        path: path.to_string(),
        kind,
        package: None,
        reason: reason.to_string(),
        changed: None,
        chain: Vec::new(),
        functions: Vec::new(),
        manifest_dir: None,
        whole: true,
    }
}

fn reached(path: &str, chain: Vec<Step>, functions: Vec<String>, whole: bool) -> AffectedTest {
    AffectedTest {
        path: path.to_string(),
        kind: Kind::Reached,
        package: None,
        reason: render(&chain),
        changed: chain.last().map(|s| s.symbol.clone()),
        chain,
        functions,
        manifest_dir: None,
        whole,
    }
}

fn top_level(path: &str, line: usize) -> Step {
    Step {
        symbol: "(top level)".to_string(),
        path: path.to_string(),
        line: Some(line),
        link: Link::Test,
        via: None,
    }
}

/// `test → caller → changed`, with the contract each hop crosses.
fn render(chain: &[Step]) -> String {
    let mut out = String::new();
    for (i, step) in chain.iter().enumerate() {
        let via = step
            .via
            .as_deref()
            .map(|v| format!(" via {v}"))
            .unwrap_or_default();
        let text = match step.link {
            _ if i == 0 => step.symbol.clone(),
            Link::Test | Link::Calls => format!(" → {}", step.symbol),
            Link::Contract => format!(" → {}{via}", step.symbol),
            Link::Client => format!(" ← {}{via}", step.symbol),
            Link::Removed => format!(" → {} (removed from {})", step.symbol, step.path),
            Link::Definition => format!(" → {} in {}", step.symbol, step.path),
        };
        out.push_str(&text);
    }
    out
}

/// A test file a runner can run; `conftest.py` and `__init__.py` hold
/// fixtures and package markers, not tests.
fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
    runnable_test(path) && name != "conftest.py" && name != "__init__.py"
}

#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    path: String,
    line: usize,
}

/// Code in a test file, or in a Rust `tests` module inside a source file.
fn is_test_symbol(symbol: &Symbol) -> bool {
    if is_test_file(&symbol.path) {
        return true;
    }
    let segments: Vec<&str> = symbol
        .name
        .split(['.', ':'])
        .filter(|s| !s.is_empty())
        .collect();
    symbol.path.ends_with(".rs")
        && segments
            .split_last()
            .is_some_and(|(_, modules)| modules.iter().any(|m| *m == "tests" || *m == "test"))
}

/// Every test file in the index: runnable test files, and Rust sources with
/// a `tests` module.
fn suite(conn: &Connection) -> Result<BTreeSet<String>> {
    let mut files = conn.prepare("SELECT path FROM files")?;
    let mut out: BTreeSet<String> = files
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| is_test_file(path))
        .collect();
    let mut inline = conn.prepare(
        "SELECT DISTINCT f.path FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.kind = 'module' AND s.name IN ('tests', 'test') AND f.path LIKE '%.rs'",
    )?;
    for path in inline.query_map([], |row| row.get::<_, String>(0))? {
        out.insert(path?);
    }
    Ok(out)
}

/// A repository-relative path from a list entry: `./a`, `a\b` and a pytest
/// node id (`a.py::test_x`) name their file. Blank lines and `#` comments
/// name nothing.
fn normalize(root: &Path, entry: &str) -> Option<String> {
    let entry = entry.trim();
    if entry.is_empty() || entry.starts_with('#') {
        return None;
    }
    let entry = entry.replace('\\', "/");
    let file = entry.split("::").next().unwrap_or_default();
    let file = Path::new(file)
        .strip_prefix(root)
        .map(|rel| rel.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file.to_string());
    let file = file.trim_start_matches("./");
    if file.is_empty() {
        None
    } else {
        Some(file.to_string())
    }
}

fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

/// The stdout of a git command that succeeded.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

/// Fails with [`AffectedUsage`] outside a git work tree.
pub fn require_git(root: &Path) -> Result<()> {
    let inside = git(root, &["rev-parse", "--is-inside-work-tree"]);
    if inside.as_deref().map(str::trim) == Some("true") {
        Ok(())
    } else {
        Err(usage(format!(
            "affected needs a git repository; {} is not inside one",
            root.display()
        )))
    }
}

/// Which change to select for, and the commit it is compared with.
fn changes(root: &Path, args: &AffectedArgs) -> Result<(Changes, String)> {
    require_git(root)?;
    if args.staged && args.rev.is_some() {
        return Err(usage("pass a revision or --staged, not both".to_string()));
    }
    let rev = args.rev.as_deref().unwrap_or("HEAD");
    if rev.is_empty() || rev.starts_with('-') {
        return Err(usage(format!("invalid git revision {rev:?}")));
    }
    let commit = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok_or_else(|| usage(format!("`{rev}` is not a commit in this repository")))?;
    let changes = if args.staged {
        Changes::Staged
    } else if args.rev.is_some() {
        Changes::Since(rev.to_string())
    } else {
        Changes::WorkingTree
    };
    Ok((changes, commit.trim().to_string()))
}

/// Paths the change adds: untracked files and files added since `base`.
fn added(root: &Path, changes: &Changes, base: &str) -> HashSet<String> {
    let mut out = untracked(root);
    let since = match changes {
        Changes::Staged => "--cached",
        _ => base,
    };
    let listed = git(
        root,
        &[
            "diff",
            "--name-only",
            "--relative",
            "--no-renames",
            "--diff-filter=A",
            since,
        ],
    );
    out.extend(
        listed
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().replace('\\', "/"))
            .filter(|l| !l.is_empty()),
    );
    out
}

/// Why every test runs, if one does.
fn full_run(
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
    base: &str,
    capped: bool,
) -> Option<String> {
    let mut triggers: Vec<(&String, &str)> = changed
        .iter()
        .filter_map(|path| Some((path, trigger(path)?)))
        .collect();
    triggers.sort();
    if let Some((path, what)) = triggers.first() {
        let more = match triggers.len() - 1 {
            0 => String::new(),
            n => format!(", and {n} more file(s) like it"),
        };
        return Some(format!("{path} changed ({what}){more}"));
    }
    if let Changes::Since(_) = changes {
        if let Some((commit, value)) = no_skip(root, base) {
            let value = if value.is_empty() {
                String::new()
            } else {
                format!(" ({value})")
            };
            return Some(format!(
                "commit {commit} has a Tessera-No-Skip trailer{value}"
            ));
        }
    }
    if capped {
        Some(format!(
            "the change reaches more than {MAX_VISITED} symbols, too many to narrow the selection"
        ))
    } else {
        None
    }
}

const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "cargo.lock",
    "go.sum",
    "go.work.sum",
    "poetry.lock",
    "uv.lock",
    "pdm.lock",
    "pipfile.lock",
    "gemfile.lock",
    "composer.lock",
    "gradle.lockfile",
    "packages.lock.json",
];
const BUILD_FILES: &[&str] = &[
    "package.json",
    "pnpm-workspace.yaml",
    "lerna.json",
    "nx.json",
    "turbo.json",
    "cargo.toml",
    "go.mod",
    "go.work",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "pipfile",
    "gemfile",
    "composer.json",
    "directory.build.props",
    "directory.packages.props",
    "cmakelists.txt",
    "makefile",
    "gnumakefile",
    ".npmrc",
    ".yarnrc",
    ".yarnrc.yml",
    ".pnpmfile.cjs",
];
/// Test runner, toolchain and CI configuration, by file name or prefix.
const CONFIG_FILES: &[&str] = &[
    "pytest.ini",
    "conftest.py",
    "tox.ini",
    "noxfile.py",
    ".swcrc",
    ".rspec",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".nvmrc",
    ".node-version",
    ".python-version",
    ".ruby-version",
    ".tool-versions",
    "jenkinsfile",
    ".gitlab-ci.yml",
    "azure-pipelines.yml",
    "bitbucket-pipelines.yml",
    ".travis.yml",
];
const CONFIG_PREFIXES: &[&str] = &[
    "jest.config.",
    "jest.setup.",
    "vitest.config.",
    "vitest.workspace.",
    "vitest.setup.",
    "vite.config.",
    "babel.config.",
    ".babelrc",
    ".mocharc",
    "karma.conf.",
    "playwright.config.",
    "cypress.config.",
    "phpunit.xml",
];
const CONFIG_DIRS: &[&str] = &[
    ".github/workflows/",
    ".github/actions/",
    ".circleci/",
    ".buildkite/",
    ".cargo/",
];
const FIXTURE_DIRS: &[&str] = &[
    "fixtures",
    "fixture",
    "__fixtures__",
    "testdata",
    "test-data",
    "test_data",
    "snapshots",
    "__snapshots__",
];

/// What a changed file is when changing it runs every test: lockfiles and
/// build files change what every test runs against, configuration changes
/// how tests run, and tests read fixtures through no call the index sees.
fn trigger(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or_default();
    if LOCKFILES.contains(&name) {
        return Some("a lockfile");
    }
    let pip = name.ends_with(".txt") || name.ends_with(".in");
    let requirements = pip
        && (name.starts_with("requirements")
            || name.starts_with("constraints")
            || lower.split('/').any(|dir| dir == "requirements"));
    if BUILD_FILES.contains(&name)
        || requirements
        || [
            ".csproj",
            ".sln",
            ".gemspec",
            ".gradle",
            ".gradle.kts",
            ".versions.toml",
        ]
        .iter()
        .any(|ext| name.ends_with(ext))
    {
        return Some("a build file");
    }
    let tsconfig =
        (name.starts_with("tsconfig") || name.starts_with("jsconfig")) && name.ends_with(".json");
    if tsconfig
        || CONFIG_FILES.contains(&name)
        || CONFIG_PREFIXES.iter().any(|p| name.starts_with(p))
        || CONFIG_DIRS
            .iter()
            .any(|dir| lower.starts_with(dir) || lower.contains(&format!("/{dir}")))
        || lower == ".tessera/config.toml"
        || lower.ends_with("/.tessera/config.toml")
    {
        return Some("test or CI configuration");
    }
    let fixture = lower.split('/').any(|s| FIXTURE_DIRS.contains(&s))
        || name.ends_with(".snap")
        || (is_test_path(path) && !runnable_test(path));
    fixture.then_some("a test fixture or snapshot")
}

/// The first commit in `base..HEAD` with a `Tessera-No-Skip:` trailer, and
/// the trailer's value.
fn no_skip(root: &Path, base: &str) -> Option<(String, String)> {
    let log = git(
        root,
        &["log", "--format=%H%x1f%B%x1e", &format!("{base}..HEAD")],
    )?;
    log.split('\x1e').find_map(|entry| {
        let (hash, message) = entry.trim_start().split_once('\x1f')?;
        let value = trailer(message, "Tessera-No-Skip")?;
        Some((hash.chars().take(12).collect(), value))
    })
}

/// A `key: value` trailer in the message's last paragraph, as
/// `git interpret-trailers` reads one; the subject line is never a trailer.
fn trailer(message: &str, key: &str) -> Option<String> {
    let (_, body) = message.trim_end().split_once('\n')?;
    let last = body.rsplit("\n\n").next()?;
    last.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case(key) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

/// Where the walk starts: a changed symbol, or for a removed name or a
/// changed contract definition, the code that uses it, with that name as
/// the chain's last step.
struct Seed {
    id: i64,
    tail: Option<Step>,
}

#[derive(Default)]
struct Seeds {
    seeds: Vec<Seed>,
    /// Test files whose top-level code uses a removed name.
    direct: Vec<(String, Vec<Step>)>,
    labels: BTreeSet<String>,
}

fn seeds(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    changes: &Changes,
) -> Result<Seeds> {
    let rev = match changes {
        Changes::Since(rev) => rev.as_str(),
        _ => "HEAD",
    };
    let mut out = Seeds::default();
    let (_, symbols) = legacy::diff_changed_symbols(conn, root, rev, None)?;
    for symbol in symbols.into_iter().filter(|s| changed.contains(&s.path)) {
        out.labels.insert(symbol.qualified_name);
        out.seeds.push(Seed {
            id: symbol.id,
            tail: None,
        });
    }
    let untracked = untracked(root);
    let mut new_files: Vec<&String> = changed.iter().filter(|p| untracked.contains(*p)).collect();
    new_files.sort();
    for path in new_files {
        for symbol in graph::symbols_in_file(conn, path)? {
            out.labels.insert(symbol.qualified_name);
            out.seeds.push(Seed {
                id: symbol.id,
                tail: None,
            });
        }
    }
    removed_names(conn, root, changed, rev, &mut out)?;
    definitions(conn, root, changed, rev, &mut out)?;
    out.seeds.sort_by_key(|s| (s.id, s.tail.is_some()));
    out.seeds.dedup_by_key(|s| s.id);
    Ok(out)
}

/// Names a changed file defined at `rev` and no longer does (a rename, a
/// deletion): the code that still uses one is where the walk starts.
fn removed_names(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    rev: &str,
    out: &mut Seeds,
) -> Result<()> {
    let mut paths: Vec<&String> = changed
        .iter()
        .filter(|p| Language::from_path(Path::new(p.as_str())).is_some_and(|l| !l.is_data()))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Ok(());
    }
    let before = files_at(root, rev, &paths);
    // Unresolved uses of the name: bare or module-qualified calls anywhere,
    // any use in the changed file, and uses the resolver traced to it.
    let mut users = conn.prepare_cached(
        "SELECT r.from_symbol_id, f.path, r.line FROM refs r JOIN files f ON f.id = r.file_id
         WHERE r.symbol_name = ?1
           AND NOT EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id AND e.to_symbol_id IS NOT NULL)
           AND (r.receiver IN ('none', 'name') OR f.path = ?2
                OR EXISTS (SELECT 1 FROM edges e WHERE e.ref_id = r.id AND e.missing_in = ?2))
         ORDER BY f.path, r.line, r.id",
    )?;
    for path in paths {
        let (Some(language), Some(old)) = (Language::from_path(Path::new(path)), before.get(path))
        else {
            continue;
        };
        let Ok(parsed) = crate::extract::parse_file(language, old) else {
            continue;
        };
        let now: HashSet<String> = graph::symbols_in_file(conn, path)?
            .into_iter()
            .map(|s| s.name)
            .collect();
        let mut gone: BTreeMap<String, String> = BTreeMap::new();
        for symbol in parsed.symbols {
            if !now.contains(&symbol.name) {
                gone.entry(symbol.name).or_insert(symbol.qualified_name);
            }
        }
        for (name, qualified) in gone {
            out.labels.insert(format!("{qualified} (removed)"));
            let tail = Step {
                symbol: qualified,
                path: path.clone(),
                line: None,
                link: Link::Removed,
                via: None,
            };
            let rows: Vec<(Option<i64>, String, i64)> = users
                .query_map(params![name, path], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            for (from, file, line) in rows {
                match from {
                    Some(id) => out.seeds.push(Seed {
                        id,
                        tail: Some(tail.clone()),
                    }),
                    None if is_test_file(&file) => out.direct.push((
                        file.clone(),
                        vec![top_level(&file, line as usize), tail.clone()],
                    )),
                    None => {}
                }
            }
        }
    }
    Ok(())
}

/// A changed contract definition (`.proto`, OpenAPI, SQL, GraphQL,
/// configuration): the code on either side of each contract it declares,
/// before or after the change.
fn definitions(
    conn: &Connection,
    root: &Path,
    changed: &HashSet<String>,
    rev: &str,
    out: &mut Seeds,
) -> Result<()> {
    let mut paths: Vec<&String> = changed
        .iter()
        .filter(|p| Language::from_path(Path::new(p.as_str())).is_some_and(|l| l.is_data()))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Ok(());
    }
    let before = files_at(root, rev, &paths);
    let mut now = conn.prepare_cached(
        "SELECT f.key, f.name FROM facts f JOIN files fl ON fl.id = f.file_id WHERE fl.path = ?1",
    )?;
    let mut sides = conn.prepare_cached(
        "SELECT DISTINCT f.symbol_id FROM facts f JOIN files fl ON fl.id = f.file_id
         WHERE f.key = ?1 AND fl.path != ?2 AND f.symbol_id IS NOT NULL ORDER BY f.symbol_id",
    )?;
    for path in paths {
        let mut keys: BTreeMap<String, String> = now
            .query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if let (Some(language), Some(old)) =
            (Language::from_path(Path::new(path)), before.get(path))
        {
            if let Ok(parsed) = crate::extract::parse_file_at(language, path, old) {
                for fact in parsed.facts {
                    keys.entry(fact.key).or_insert(fact.name);
                }
            }
        }
        for (key, name) in keys {
            let ids: Vec<i64> = sides
                .query_map(params![key, path], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            if ids.is_empty() {
                continue;
            }
            out.labels.insert(format!("{name} ({path})"));
            let tail = Step {
                symbol: name,
                path: path.clone(),
                line: None,
                link: Link::Definition,
                via: None,
            };
            for id in ids {
                out.seeds.push(Seed {
                    id,
                    tail: Some(tail.clone()),
                });
            }
        }
    }
    Ok(())
}

/// Where a test file reaches the change: one of its symbols, or code outside
/// any symbol that uses `below`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Hit {
    Symbol(i64),
    TopLevel { line: usize, below: i64 },
}

/// Breadth-first from the seeds to everything that uses them, through
/// resolved references and contracts, so each test's first hit has the
/// shortest chain.
struct Walk<'a> {
    conn: &'a Connection,
    symbols: HashMap<i64, Option<Symbol>>,
    /// Provider → the clients that call it, and the contract.
    clients: HashMap<i64, Vec<(i64, String)>>,
    /// Client → the providers it calls, and the contract.
    providers: HashMap<i64, Vec<(i64, String)>>,
    depth: HashMap<i64, usize>,
    parent: HashMap<i64, (i64, Link, Option<String>)>,
    tails: HashMap<i64, Step>,
    hits: BTreeMap<String, Vec<Hit>>,
    /// Reached symbols in `conftest.py` files, whose fixtures tests use
    /// without calling them.
    conftests: Vec<i64>,
    capped: bool,
}

impl<'a> Walk<'a> {
    fn new(conn: &'a Connection, root: &Path) -> Result<Self> {
        let mut clients: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
        let mut providers: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
        let contracts: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM facts WHERE symbol_id IS NOT NULL)",
            [],
            |row| row.get(0),
        )?;
        if contracts {
            for hop in map::hops(&map::Sources::single(conn, root)?)? {
                clients
                    .entry(hop.to.1)
                    .or_default()
                    .push((hop.from.1, hop.via.clone()));
                providers
                    .entry(hop.from.1)
                    .or_default()
                    .push((hop.to.1, hop.via));
            }
        }
        Ok(Self {
            conn,
            symbols: HashMap::new(),
            clients,
            providers,
            depth: HashMap::new(),
            parent: HashMap::new(),
            tails: HashMap::new(),
            hits: BTreeMap::new(),
            conftests: Vec::new(),
            capped: false,
        })
    }

    fn symbol(&mut self, id: i64) -> Result<Option<Symbol>> {
        if let Some(found) = self.symbols.get(&id) {
            return Ok(found.clone());
        }
        let found = graph::symbol_by_id(self.conn, id)?.map(|s| Symbol {
            name: s.qualified_name,
            path: s.path,
            line: s.start_line,
        });
        self.symbols.insert(id, found.clone());
        Ok(found)
    }

    /// Records a reached symbol that is a test or a pytest fixture.
    fn visit(&mut self, id: i64) -> Result<()> {
        let Some(symbol) = self.symbol(id)? else {
            return Ok(());
        };
        if is_test_symbol(&symbol) {
            self.hits
                .entry(symbol.path)
                .or_default()
                .push(Hit::Symbol(id));
        } else if symbol.path.rsplit('/').next() == Some("conftest.py") {
            self.conftests.push(id);
        }
        Ok(())
    }

    fn run(&mut self, seeds: &[Seed], max_depth: usize) -> Result<()> {
        let conn = self.conn;
        let mut callers = conn.prepare_cached(
            "SELECT e.from_symbol_id, f.path, r.line FROM edges e
             LEFT JOIN refs r ON r.id = e.ref_id LEFT JOIN files f ON f.id = r.file_id
             WHERE e.to_symbol_id = ?1 ORDER BY 1, 2, 3",
        )?;
        let mut queue = VecDeque::new();
        for seed in seeds {
            if self.depth.len() >= MAX_VISITED {
                self.capped = true;
                return Ok(());
            }
            if self.depth.insert(seed.id, 0).is_none() {
                if let Some(tail) = &seed.tail {
                    self.tails.insert(seed.id, tail.clone());
                }
                self.visit(seed.id)?;
                queue.push_back(seed.id);
            }
        }
        while let Some(node) = queue.pop_front() {
            let level = self.depth[&node];
            if level >= max_depth {
                continue;
            }
            let rows: Vec<(Option<i64>, Option<String>, Option<i64>)> = callers
                .query_map(params![node], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            let mut next: Vec<(i64, Link, Option<String>)> = Vec::new();
            for (from, path, line) in rows {
                match (from, path) {
                    (Some(id), _) => next.push((id, Link::Calls, None)),
                    (None, Some(path)) if is_test_file(&path) => {
                        let hit = Hit::TopLevel {
                            line: line.unwrap_or_default() as usize,
                            below: node,
                        };
                        let hits = self.hits.entry(path).or_default();
                        if !hits.contains(&hit) {
                            hits.push(hit);
                        }
                    }
                    _ => {}
                }
            }
            for (client, via) in self.clients.get(&node).into_iter().flatten() {
                next.push((*client, Link::Contract, Some(via.clone())));
            }
            if level == 0 {
                for (provider, via) in self.providers.get(&node).into_iter().flatten() {
                    next.push((*provider, Link::Client, Some(via.clone())));
                }
            }
            for (id, link, via) in next {
                if self.depth.contains_key(&id) {
                    continue;
                }
                if self.depth.len() >= MAX_VISITED {
                    self.capped = true;
                    return Ok(());
                }
                self.depth.insert(id, level + 1);
                self.parent.insert(id, (node, link, via));
                self.visit(id)?;
                queue.push_back(id);
            }
        }
        Ok(())
    }

    /// From `start` back to the seed it was reached from.
    fn chain(&mut self, start: i64) -> Result<Vec<Step>> {
        let mut steps = vec![self.step(start, Link::Test, None)?];
        let mut current = start;
        while let Some((next, link, via)) = self.parent.get(&current).cloned() {
            steps.push(self.step(next, link, via)?);
            current = next;
        }
        if let Some(tail) = self.tails.get(&current) {
            steps.push(tail.clone());
        }
        Ok(steps)
    }

    fn step(&mut self, id: i64, link: Link, via: Option<String>) -> Result<Step> {
        let symbol = self.symbol(id)?;
        Ok(Step {
            symbol: symbol
                .as_ref()
                .map_or_else(|| format!("#{id}"), |s| s.name.clone()),
            path: symbol.as_ref().map(|s| s.path.clone()).unwrap_or_default(),
            line: symbol.map(|s| s.line),
            link,
            via,
        })
    }
}

/// Workspace packages: the nearest manifest of a file's ecosystem that
/// names one (npm, pnpm and yarn workspaces, Cargo, Go and Maven modules,
/// Gradle projects, Python projects).
struct Packages {
    root: PathBuf,
    names: HashMap<(String, &'static str), Option<String>>,
}

impl Packages {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            names: HashMap::new(),
        }
    }

    /// The directory of the manifest naming `path`'s package, and the name.
    fn find(&mut self, path: &str) -> Option<(String, String)> {
        let manifests = manifests_for(path);
        if manifests.is_empty() {
            return None;
        }
        let mut dir = parent(path);
        loop {
            for manifest in manifests {
                if let Some(name) = self.name(&dir, manifest) {
                    return Some((dir, name));
                }
            }
            if dir.is_empty() {
                return None;
            }
            dir = parent(&dir);
        }
    }

    fn name(&mut self, dir: &str, manifest: &'static str) -> Option<String> {
        let key = (dir.to_string(), manifest);
        if let Some(found) = self.names.get(&key) {
            return found.clone();
        }
        let found = std::fs::read_to_string(self.root.join(dir).join(manifest))
            .ok()
            .and_then(|text| package_name(manifest, &text, dir, &self.root));
        self.names.insert(key, found.clone());
        found
    }

    /// Fails, naming the packages there are, when no indexed file belongs
    /// to `name`.
    fn check(&mut self, conn: &Connection, name: &str) -> Result<()> {
        let mut stmt = conn.prepare("SELECT path FROM files ORDER BY path")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut known = BTreeSet::new();
        for path in paths {
            if let Some((_, package)) = self.find(&path) {
                if package == name {
                    return Ok(());
                }
                known.insert(package);
            }
        }
        let known: Vec<String> = known.into_iter().collect();
        Err(usage(if known.is_empty() {
            format!("no package named `{name}`: this repository has no package manifests")
        } else {
            format!(
                "no package named `{name}`; packages here: {}",
                known.join(", ")
            )
        }))
    }
}

fn extension(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or_default();
    name.rsplit_once('.').map_or("", |(_, ext)| ext)
}

fn manifests_for(path: &str) -> &'static [&'static str] {
    match extension(path) {
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => &["package.json"],
        "rs" => &["Cargo.toml"],
        "go" => &["go.mod"],
        "java" | "kt" | "kts" | "scala" | "groovy" => {
            &["pom.xml", "build.gradle.kts", "build.gradle"]
        }
        "py" => &["pyproject.toml"],
        "php" => &["composer.json"],
        _ => &[],
    }
}

fn package_name(manifest: &str, text: &str, dir: &str, root: &Path) -> Option<String> {
    let name = match manifest {
        "package.json" | "composer.json" => serde_json::from_str::<serde_json::Value>(text)
            .ok()?
            .get("name")?
            .as_str()?
            .to_string(),
        "Cargo.toml" => toml::from_str::<toml::Table>(text)
            .ok()?
            .get("package")?
            .get("name")?
            .as_str()?
            .to_string(),
        "pyproject.toml" => {
            let table = toml::from_str::<toml::Table>(text).ok()?;
            let project = table.get("project").and_then(|p| p.get("name"));
            let poetry = || table.get("tool")?.get("poetry")?.get("name");
            project.or_else(poetry)?.as_str()?.to_string()
        }
        "go.mod" => {
            let line = text
                .lines()
                .find_map(|line| line.trim().strip_prefix("module "))?;
            let module = line.split("//").next().unwrap_or(line);
            module.trim().trim_matches('"').to_string()
        }
        "pom.xml" => {
            // The project's own artifactId, not its parent's.
            let own = match (text.find("<parent>"), text.find("</parent>")) {
                (Some(start), Some(end)) if start < end => {
                    format!("{}{}", &text[..start], &text[end..])
                }
                _ => text.to_string(),
            };
            let start = own.find("<artifactId>")? + "<artifactId>".len();
            let end = own[start..].find("</artifactId>")? + start;
            own[start..end].trim().to_string()
        }
        // Gradle names a project after its directory.
        _ => Path::new(dir)
            .file_name()
            .or_else(|| root.file_name())?
            .to_string_lossy()
            .into_owned(),
    };
    (!name.is_empty()).then_some(name)
}

/// A test runner `affected` writes a command line for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runner {
    Jest,
    Vitest,
    Pytest,
    GoTest,
    Cargo,
}

impl Runner {
    fn runs(self, path: &str) -> bool {
        let ext = extension(path);
        match self {
            Runner::Jest | Runner::Vitest => matches!(
                ext,
                "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts"
            ),
            Runner::Pytest => ext == "py",
            Runner::GoTest => ext == "go",
            Runner::Cargo => ext == "rs",
        }
    }
}

/// `arg` as one shell word.
fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:@%+=,-".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn command_line(command: &str, args: Vec<String>) -> String {
    if args.is_empty() {
        String::new()
    } else {
        format!("{command} {}\n", args.join(" "))
    }
}

/// `TestCalc.test_double`: a pytest test function or method.
fn pytest_name(name: &str) -> bool {
    let segments: Vec<&str> = name.split('.').collect();
    segments.split_last().is_some_and(|(test, classes)| {
        test.starts_with("test") && classes.iter().all(|c| c.starts_with("Test"))
    })
}

/// A top-level Go test, example or fuzz target, which `-run` matches.
/// `TestMain` is not one: it wraps every test in its package.
fn go_test_name(name: &str) -> bool {
    !name.contains('.')
        && name != "TestMain"
        && ["Test", "Example", "Fuzz"]
            .iter()
            .any(|p| name.starts_with(p))
}

/// `tests/<name>.rs` or `tests/<name>/main.rs` under the crate: the target
/// `cargo test --test <name>` runs.
fn integration_target(crate_dir: &str, path: &str) -> Option<String> {
    let rel = if crate_dir.is_empty() {
        path
    } else {
        path.strip_prefix(crate_dir)?.strip_prefix('/')?
    };
    let rest = rel.strip_prefix("tests/")?;
    match rest.split_once('/') {
        None => rest.strip_suffix(".rs").map(str::to_string),
        Some((name, "main.rs")) => Some(name.to_string()),
        _ => None,
    }
}

/// The test functions a runner can be narrowed to: only for a test selected
/// for what it reaches, outside a full run, when each is a name the runner
/// takes.
fn narrow_to(test: &AffectedTest, full: bool, valid: fn(&str) -> bool) -> Option<&[String]> {
    let narrow = !full
        && test.kind == Kind::Reached
        && !test.whole
        && !test.functions.is_empty()
        && test.functions.iter().all(|f| valid(f));
    if narrow {
        Some(&test.functions)
    } else {
        None
    }
}

/// `./calc` for a package directory inside a Go module at `module`.
fn go_package(module: &str, dir: &str) -> String {
    let rel = if module.is_empty() {
        Some(dir)
    } else if dir == module {
        Some("")
    } else {
        dir.strip_prefix(module).and_then(|r| r.strip_prefix('/'))
    };
    match rel {
        Some("") => ".".to_string(),
        Some(rel) => format!("./{rel}"),
        None => format!("./{dir}"),
    }
}

impl AffectedResult {
    /// The command lines that run the selected tests with `runner`, one per
    /// line; empty when it has none to run.
    pub fn command(&self, runner: Runner) -> String {
        let full = self.full_run.is_some();
        let tests: Vec<&AffectedTest> =
            self.tests.iter().filter(|t| runner.runs(&t.path)).collect();
        match runner {
            Runner::Jest => command_line(
                "jest --runTestsByPath",
                tests.iter().map(|t| quote(&t.path)).collect(),
            ),
            Runner::Vitest => {
                command_line("vitest run", tests.iter().map(|t| quote(&t.path)).collect())
            }
            Runner::Pytest => {
                let mut args = Vec::new();
                for test in &tests {
                    match narrow_to(test, full, pytest_name) {
                        Some(functions) => {
                            args.extend(functions.iter().map(|f| {
                                quote(&format!("{}::{}", test.path, f.replace('.', "::")))
                            }))
                        }
                        None => args.push(quote(&test.path)),
                    }
                }
                command_line("pytest", args)
            }
            Runner::GoTest => {
                type Module = (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>);
                let mut modules: BTreeMap<String, Module> = BTreeMap::new();
                for test in &tests {
                    let module = test.manifest_dir.clone().unwrap_or_default();
                    let package = go_package(&module, &parent(&test.path));
                    let (whole, narrowed, names) = modules.entry(module).or_default();
                    match narrow_to(test, full, go_test_name) {
                        Some(functions) => {
                            narrowed.insert(package);
                            names.extend(functions.iter().cloned());
                        }
                        None => {
                            whole.insert(package);
                        }
                    }
                }
                let mut out = String::new();
                for (module, (whole, narrowed, names)) in modules {
                    let go = if module.is_empty() {
                        "go".to_string()
                    } else {
                        format!("go -C {}", quote(&module))
                    };
                    out.push_str(&command_line(
                        &format!("{go} test"),
                        whole.iter().map(|p| quote(p)).collect(),
                    ));
                    let narrowed: Vec<String> =
                        narrowed.difference(&whole).map(|p| quote(p)).collect();
                    if !narrowed.is_empty() {
                        let names: Vec<String> = names.into_iter().collect();
                        out.push_str(&format!(
                            "{go} test {} -run {}\n",
                            narrowed.join(" "),
                            quote(&format!("^({})$", names.join("|")))
                        ));
                    }
                }
                out
            }
            Runner::Cargo => {
                let mut workspace = false;
                let mut crates: BTreeMap<String, (bool, BTreeSet<String>)> = BTreeMap::new();
                for test in &tests {
                    let Some(name) = &test.package else {
                        workspace = true;
                        continue;
                    };
                    let (whole, targets) = crates.entry(name.clone()).or_default();
                    let dir = test.manifest_dir.as_deref().unwrap_or_default();
                    match integration_target(dir, &test.path) {
                        Some(target) if !full => {
                            targets.insert(target);
                        }
                        _ => *whole = true,
                    }
                }
                let mut out = String::new();
                if workspace {
                    out.push_str("cargo test --workspace\n");
                }
                for (name, (whole, targets)) in crates {
                    let mut args = vec!["-p".to_string(), quote(&name)];
                    if !whole {
                        for target in targets {
                            args.push("--test".to_string());
                            args.push(quote(&target));
                        }
                    }
                    out.push_str(&command_line("cargo test", args));
                }
                out
            }
        }
    }

    /// The counts, and why every test runs when one does.
    pub fn summary(&self) -> String {
        let against = if self.staged {
            "the staged changes".to_string()
        } else {
            format!("changes since {}", self.rev)
        };
        let mut out = format!(
            "{} of {} test files selected ({} skipped) for {against}\n",
            self.totals.selected, self.totals.total, self.totals.skipped
        );
        if let Some(reason) = &self.full_run {
            out.push_str(&format!("full run: {reason}\n"));
        }
        out
    }
}

impl fmt::Display for AffectedResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for AffectedResult {
    fn agent_text(&self) -> String {
        let mut out = String::new();
        if let Some(explain) = &self.explain {
            out.push_str(&explain.line());
            out.push('\n');
        }
        out.push_str(&self.summary());
        for test in &self.tests {
            out.push_str(&format!(
                "  {} [{}] {}\n",
                test.path, test.kind, test.reason
            ));
        }
        for note in &self.notes {
            out.push_str(&format!("note: {note}\n"));
        }
        out.push_str(&page_footer(&self.page));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_run_triggers() {
        for (path, what) in [
            ("web/package-lock.json", "a lockfile"),
            ("Cargo.lock", "a lockfile"),
            ("svc/requirements-dev.txt", "a build file"),
            ("app/App.csproj", "a build file"),
            ("tsconfig.base.json", "test or CI configuration"),
            ("tests/conftest.py", "test or CI configuration"),
            (".github/workflows/ci.yml", "test or CI configuration"),
            ("pkg/.cargo/config.toml", "test or CI configuration"),
            (
                "src/__snapshots__/a.test.ts.snap",
                "a test fixture or snapshot",
            ),
            ("tests/stability/cli.json", "a test fixture or snapshot"),
            ("go/testdata/in.txt", "a test fixture or snapshot"),
            ("requirements/base.txt", "a build file"),
            ("svc/requirements/dev.in", "a build file"),
            ("gradle/libs.versions.toml", "a build file"),
            (
                "buildSrc/src/main/kotlin/java-conventions.gradle.kts",
                "a build file",
            ),
            ("lib/deps.gradle", "a build file"),
            ("Makefile", "a build file"),
            ("web/.npmrc", "a build file"),
            (".yarnrc.yml", "a build file"),
            (
                ".github/actions/setup/action.yml",
                "test or CI configuration",
            ),
        ] {
            assert_eq!(trigger(path), Some(what), "{path}");
        }
        for path in [
            "README.md",
            "src/app.ts",
            "test/app.test.ts",
            ".tessera/.gitignore",
            "docs/package.md",
        ] {
            assert_eq!(trigger(path), None, "{path}");
        }
    }

    #[test]
    fn trailers_are_in_the_last_paragraph_never_the_subject() {
        assert_eq!(
            trailer(
                "fix: x\n\nbody\n\nTessera-No-Skip: yes\nRefs: #1\n",
                "Tessera-No-Skip"
            ),
            Some("yes".to_string())
        );
        assert_eq!(
            trailer("fix: x\n\ntessera-no-skip:\n", "Tessera-No-Skip"),
            Some(String::new())
        );
        assert_eq!(trailer("Tessera-No-Skip: yes", "Tessera-No-Skip"), None);
        assert_eq!(
            trailer(
                "fix\n\nTessera-No-Skip: yes\n\nlast words",
                "Tessera-No-Skip"
            ),
            None
        );
    }

    #[test]
    fn runner_names_and_targets() {
        assert!(pytest_name("TestCalc.test_double"));
        assert!(pytest_name("test_load"));
        assert!(!pytest_name("helper"));
        assert!(!pytest_name("Calc.test_x"));
        assert!(go_test_name("TestLoad"));
        assert!(!go_test_name("helper"));
        assert!(!go_test_name("Suite.TestX"));
        assert_eq!(
            integration_target("rs", "rs/tests/quad.rs").as_deref(),
            Some("quad")
        );
        assert_eq!(
            integration_target("", "tests/cli/main.rs").as_deref(),
            Some("cli")
        );
        assert_eq!(integration_target("", "tests/support/mod.rs"), None);
        assert_eq!(integration_target("rs", "rs/src/lib.rs"), None);
        assert_eq!(go_package("go", "go/calc"), "./calc");
        assert_eq!(go_package("", "calc"), "./calc");
        assert_eq!(go_package("go", "go"), ".");
        assert_eq!(quote("a/b.ts"), "a/b.ts");
        assert_eq!(quote("app/[id]/x's.ts"), "'app/[id]/x'\\''s.ts'");
    }

    #[test]
    fn package_names_from_manifests() {
        let root = Path::new("/r");
        assert_eq!(
            package_name("pom.xml", "<project><parent><artifactId>p</artifactId></parent><artifactId>own</artifactId></project>", "svc", root).as_deref(),
            Some("own")
        );
        assert_eq!(
            package_name(
                "go.mod",
                "// x\nmodule example.com/a // comment\n",
                "",
                root
            )
            .as_deref(),
            Some("example.com/a")
        );
        assert_eq!(
            package_name("pyproject.toml", "[tool.poetry]\nname = \"p\"\n", "", root).as_deref(),
            Some("p")
        );
        assert_eq!(
            package_name("Cargo.toml", "[workspace]\nmembers = []\n", "", root),
            None
        );
        assert_eq!(
            package_name("build.gradle", "", "apps/api", root).as_deref(),
            Some("api")
        );
    }
}
