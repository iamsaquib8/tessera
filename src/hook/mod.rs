//! `tessera hook <agent> <event>`: one entry point for every agent's hooks.
//! It reads the event JSON on stdin, keeps per-session state, and answers in
//! the agent's documented output shape. Hooks fail open: when Tessera cannot
//! answer (no index yet, a busy writer, an error), it prints nothing.

pub mod payload;
pub mod session;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::config::TesseraConfig;
use crate::db;
use crate::fresh::{self, Freshness};
use crate::indexer::{self, IndexLock};
use crate::output::Confidence;
use crate::query::{self, graph, Changes, Finding, Problem, VerifyResult};
use crate::types::SymbolRecord;
use crate::walk;
use crate::workspace::Workspace;
use payload::Payload;
use session::{content_hash, Session, Store};

/// How long an edit hook waits for another index writer before staying silent.
const EDIT_WAIT: Duration = Duration::from_secs(3);
/// Stop may wait longer: the turn is over.
const STOP_WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(25);
/// Stop blocks at most this many times per turn.
const MAX_STOP_BLOCKS: u32 = 3;
/// The pre-edit note names at most this many symbols, with this many call
/// sites and tests each.
const NOTE_SYMBOLS: usize = 3;
const NOTE_SITES: usize = 3;
const NOTE_TESTS: usize = 2;
const TEST_DEPTH: usize = 4;
const TEST_SCAN_CAP: usize = 2_000;
/// The baseline checks contracts (V5) in at most this many files that were
/// already changed when the session began.
const BASELINE_CONTRACT_FILES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    SessionStart,
    PreEdit,
    PostEdit,
    Stop,
}

impl Event {
    /// Accepts each agent's own event names as well as Tessera's.
    pub fn parse(name: &str) -> Option<Self> {
        match squash(name).as_str() {
            "sessionstart" => Some(Self::SessionStart),
            "preedit" | "pretooluse" | "beforetool" => Some(Self::PreEdit),
            "postedit" | "posttooluse" | "aftertool" | "afterfileedit" => Some(Self::PostEdit),
            "stop" | "agentstop" | "afteragent" => Some(Self::Stop),
            _ => None,
        }
    }
}

/// Output families, each from the vendor's documented hook contract
/// (docs/agents.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    /// Claude Code, also when pointed at DeepSeek.
    Claude,
    /// Claude's shape, but Stop must always print JSON.
    Codex,
    /// Claude's shape; a pre-tool answer must carry a permission decision,
    /// so it gets no pre-edit note.
    Qwen,
    Gemini,
    Copilot,
    Cursor,
    Augment,
    /// Answers `{}` to everything but Stop.
    Antigravity,
}

impl Agent {
    pub fn parse(name: &str) -> Option<Self> {
        match squash(name).as_str() {
            "claude" | "claudecode" | "deepseek" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "qwen" | "qwencode" => Some(Self::Qwen),
            "gemini" | "geminicli" => Some(Self::Gemini),
            "copilot" | "copilotcli" => Some(Self::Copilot),
            "cursor" => Some(Self::Cursor),
            "augment" | "auggie" => Some(Self::Augment),
            "antigravity" | "agy" => Some(Self::Antigravity),
            _ => None,
        }
    }
}

fn squash(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Information the agent should see; never stops it.
    Context(String),
    /// A problem the agent must fix before it goes on.
    Block(String),
}

#[derive(Debug, Clone, Default)]
pub struct HookOptions {
    pub root: Option<PathBuf>,
    pub db: Option<PathBuf>,
    /// The `tessera` executable, run in the background to build the index.
    pub exe: Option<PathBuf>,
}

/// Runs one hook and returns the JSON to print, if any.
pub fn run(agent: &str, event: &str, input: &str, options: &HookOptions) -> Result<Option<Value>> {
    let (Some(agent), Some(kind)) = (Agent::parse(agent), Event::parse(event)) else {
        return Ok(None);
    };
    let payload = Payload::parse(input);
    let cwd = match &payload.cwd {
        Some(cwd) => cwd.clone(),
        None => std::env::current_dir()?,
    };
    let ws = Workspace::resolve(options.root.as_deref(), options.db.as_deref(), &cwd);
    if ws.is_too_broad() {
        return Ok(None);
    }
    fs::create_dir_all(ws.db_path.parent().unwrap_or(Path::new(".")))?;
    indexer::ignore_index_dir(&ws.db_path)?;
    let hook = Hook {
        ws: &ws,
        exe: options.exe.as_deref(),
    };
    let (store, mut session, new) = Store::open(&ws.db_path, &payload.session)?;
    if new {
        hook.start(&mut session);
        if kind == Event::PostEdit {
            // First seen after an edit: that edit is the agent's, not the
            // state the session began in.
            for path in payload.paths_in(&ws.root) {
                session.dirty.remove(&path);
            }
        }
    }
    let reply = match kind {
        Event::SessionStart => {
            session::expire(&ws.db_path);
            if !new {
                hook.refresh_in_background();
            }
            None
        }
        Event::PreEdit | Event::PostEdit
            if session.last_edit == Some(session::hash(input.as_bytes())) =>
        {
            // The same event again: Tessera is wired twice (plugin and init).
            None
        }
        Event::PreEdit if matches!(agent, Agent::Claude | Agent::Codex) => {
            session.last_edit = Some(session::hash(input.as_bytes()));
            hook.pre_edit(&payload, &mut session)?
        }
        Event::PreEdit => None,
        Event::PostEdit => {
            session.last_edit = Some(session::hash(input.as_bytes()));
            hook.post_edit(&payload, &mut session)?
        }
        Event::Stop => hook.stop(&payload, &mut session)?,
    };
    store.save(&session)?;
    Ok(reply
        .and_then(|reply| render(agent, kind, event, reply))
        .or_else(|| silent(agent, kind)))
}

/// The agent's output shape for `reply`; `None` where its contract has no
/// way to say it.
pub fn render(agent: Agent, kind: Event, event: &str, reply: Reply) -> Option<Value> {
    let (text, block) = match reply {
        Reply::Context(text) => (text, false),
        Reply::Block(text) => (text, true),
    };
    let context = |name: &str| json!({ "hookSpecificOutput": { "hookEventName": name, "additionalContext": text } });
    match (agent, kind) {
        (Agent::Claude | Agent::Codex | Agent::Qwen, Event::PostEdit | Event::Stop) if block => {
            Some(json!({ "decision": "block", "reason": text }))
        }
        (Agent::Claude | Agent::Codex, Event::PreEdit) => Some(context("PreToolUse")),
        (Agent::Claude | Agent::Codex | Agent::Qwen | Agent::Augment, Event::PostEdit) => {
            Some(context("PostToolUse"))
        }
        (Agent::Gemini, Event::PostEdit) => {
            Some(json!({ "hookSpecificOutput": { "additionalContext": text } }))
        }
        (Agent::Gemini, Event::Stop) => Some(json!({ "decision": "deny", "reason": text })),
        (Agent::Copilot, Event::PostEdit) => Some(json!({ "additionalContext": text })),
        (Agent::Copilot, Event::Stop) => Some(json!({ "decision": "block", "reason": text })),
        (Agent::Cursor, Event::PostEdit) if squash(event) != "afterfileedit" => {
            Some(json!({ "additional_context": text }))
        }
        (Agent::Cursor, Event::Stop) => Some(json!({ "followup_message": text })),
        (Agent::Augment, Event::Stop) => Some(json!({
            "hookSpecificOutput": { "hookEventName": "Stop", "decision": "block", "reason": text }
        })),
        (Agent::Antigravity, Event::Stop) => {
            Some(json!({ "decision": "continue", "reason": text }))
        }
        _ => None,
    }
}

/// What to print when there is nothing to say, for agents that need JSON
/// on every successful run.
pub fn silent(agent: Agent, kind: Event) -> Option<Value> {
    match (agent, kind) {
        (Agent::Codex, Event::Stop) | (Agent::Antigravity, _) => Some(json!({})),
        _ => None,
    }
}

/// The output for a hook that could not run: the agent's no-op answer.
pub fn fallback(agent: &str, event: &str) -> Option<Value> {
    silent(Agent::parse(agent)?, Event::parse(event)?)
}

struct Hook<'a> {
    ws: &'a Workspace,
    exe: Option<&'a Path>,
}

impl Hook<'_> {
    /// First contact with a session: note what was already changed, and
    /// bring the index up to date in the background.
    fn start(&self, session: &mut Session) {
        session.baseline = git(&self.ws.root, &["rev-parse", "--verify", "--quiet", "HEAD"]);
        if let Ok(paths) = query::changed_paths(&self.ws.root, &Changes::WorkingTree) {
            session.dirty = paths
                .into_iter()
                .map(|path| {
                    let hash = content_hash(&self.ws.root.join(&path));
                    (path, hash)
                })
                .collect();
        }
        self.refresh_in_background();
    }

    fn refresh_in_background(&self) {
        if !matches!(fresh::check(self.ws), Ok(Freshness::Fresh)) {
            self.index_in_background();
        }
    }

    fn index_in_background(&self) {
        let Some(exe) = self.exe else {
            return;
        };
        let _ = Command::new(exe)
            .arg("--quiet")
            .arg("--root")
            .arg(&self.ws.root)
            .arg("--db")
            .arg(&self.ws.db_path)
            .arg("index")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    /// Waits up to `wait` for an index no one is writing, starting a build
    /// when there is none; false when it is still not there.
    fn ready(&self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut started = false;
        loop {
            if let Ok(Some(lock)) = IndexLock::try_acquire(&self.ws.db_path) {
                if db::open_ro(&self.ws.db_path).is_ok() {
                    return true;
                }
                drop(lock);
                if !started {
                    self.index_in_background();
                    started = true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(POLL);
        }
    }

    /// Edited files Tessera indexes: inside the root, not ignored or
    /// generated. Deleted files stay, so their importers are checked.
    fn admitted(&self, paths: impl IntoIterator<Item = String>) -> Vec<String> {
        let config = TesseraConfig::load(&self.ws.root);
        let mut out: Vec<String> = paths
            .into_iter()
            .filter(|path| {
                walk::admits(&self.ws.root, path, &config)
                    || (!self.ws.root.join(path).exists()
                        && !path.split('/').any(|name| name.starts_with('.')))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Before an edit: who depends on the code it touches, once per symbol.
    fn pre_edit(&self, payload: &Payload, session: &mut Session) -> Result<Option<Reply>> {
        if !payload.is_edit() {
            return Ok(None);
        }
        let paths: Vec<String> = self
            .admitted(payload.paths_in(&self.ws.root))
            .into_iter()
            .filter(|path| self.ws.root.join(path).is_file())
            .collect();
        if paths.is_empty() || !self.ready(EDIT_WAIT) {
            return Ok(None);
        }
        indexer::index_files(self.ws, &paths)?;
        self.take_baseline(session, &paths);
        let conn = db::open_ro(&self.ws.db_path)?;
        let old_texts: &[String] = if paths.len() == 1 {
            &payload.old_texts
        } else {
            &[]
        };
        let mut notes = Vec::new();
        for path in &paths {
            let source = fs::read_to_string(self.ws.root.join(path)).unwrap_or_default();
            let spans = edit_spans(&source, old_texts);
            for symbol in touched(graph::symbols_in_file(&conn, path)?, spans.as_deref()) {
                let key = format!("{}#{}", symbol.path, symbol.qualified_name);
                if session.announced.contains(&key) {
                    continue;
                }
                if let Some(note) = note(&conn, &symbol)? {
                    notes.push((key, note));
                }
            }
        }
        notes.sort_by_key(|n| std::cmp::Reverse(n.1 .0));
        notes.truncate(NOTE_SYMBOLS);
        if notes.is_empty() {
            return Ok(None);
        }
        session.counts.notes += 1;
        let mut text = "Tessera: this edit touches code used from other files.".to_string();
        for (key, (_, line)) in notes {
            session.announced.insert(key);
            text.push_str("\n- ");
            text.push_str(&line);
        }
        text.push_str("\nKeep those call sites working, or update them in the same change.");
        Ok(Some(Reply::Context(text)))
    }

    /// After an edit: V1–V5 over every file edited this session. A problem
    /// is reported first; it blocks only when it survives an edit that fixed
    /// nothing, so a change made in several steps is never interrupted.
    fn post_edit(&self, payload: &Payload, session: &mut Session) -> Result<Option<Reply>> {
        if !payload.is_edit() {
            return Ok(None);
        }
        let paths = self.admitted(payload.paths_in(&self.ws.root));
        if paths.is_empty() {
            return Ok(None);
        }
        session.edited.extend(paths.iter().cloned());
        if !self.ready(EDIT_WAIT) {
            return Ok(None);
        }
        self.take_baseline(session, &paths);
        indexer::index_files(self.ws, &paths)?;
        let conn = db::open_ro(&self.ws.db_path)?;
        let scope: HashSet<String> = session.edited.iter().cloned().collect();
        let found = query::problems(&conn, &self.ws.root, &scope, &Changes::WorkingTree)?;
        let (findings, now) = judge(found, session);
        let result = VerifyResult::new(findings, Vec::new());
        let (persisting, fixed) = compare(&session.problems, &now);
        session.problems = now;
        if result.ok {
            return Ok(None);
        }
        let brief = result.agent_brief();
        Ok(Some(if persisting > 0 && fixed == 0 {
            session.counts.post_blocks += 1;
            session.blocked.extend(blocked(&result));
            Reply::Block(format!(
                "{brief}\nThese were already there after your previous edit. Fix them before you go on."
            ))
        } else {
            session.counts.reports += 1;
            Reply::Context(format!(
                "{brief}\nIf the change is not finished yet, carry on; Tessera checks again when you stop."
            ))
        }))
    }

    /// At the end of a turn: everything changed since the session began.
    fn stop(&self, payload: &Payload, session: &mut Session) -> Result<Option<Reply>> {
        if !payload.finished() {
            return Ok(None);
        }
        if payload.stop_hook_active == Some(false) {
            session.stop_blocks = 0;
        }
        if session.stop_blocks >= MAX_STOP_BLOCKS {
            return Ok(None);
        }
        let since = session.baseline.clone().map(Changes::Since);
        let (changes, changed) = match since.as_ref().and_then(|changes| {
            Some((
                changes.clone(),
                query::changed_paths(&self.ws.root, changes).ok()?,
            ))
        }) {
            Some(found) => found,
            None => (
                Changes::WorkingTree,
                query::changed_paths(&self.ws.root, &Changes::WorkingTree)?,
            ),
        };
        let fresh_changes = changed.into_iter().filter(|path| {
            session
                .dirty
                .get(path)
                .is_none_or(|hash| *hash != content_hash(&self.ws.root.join(path)))
        });
        let scope = self.admitted(fresh_changes.chain(session.edited.iter().cloned()));
        if scope.is_empty() || !self.ready(STOP_WAIT) {
            return Ok(None);
        }
        self.take_baseline(session, &scope);
        indexer::index_files(self.ws, &scope)?;
        let conn = db::open_ro(&self.ws.db_path)?;
        let scope: HashSet<String> = scope.into_iter().collect();
        let found = query::problems(&conn, &self.ws.root, &scope, &changes)?;
        let (findings, now) = judge(found, session);
        session.problems = now;
        if !findings.iter().any(|f| f.blocking) {
            return Ok(None);
        }
        let result = query::with_tests(&conn, &self.ws.root, &scope, &changes, findings)?;
        session.stop_blocks += 1;
        session.counts.stop_blocks += 1;
        session.blocked.extend(blocked(&result));
        let mut reason = result.agent_brief();
        if !result.tests.is_empty() {
            let tests: Vec<&str> = result.tests.iter().take(5).map(String::as_str).collect();
            reason.push_str(&format!(
                "\nTests that reach the change: {}",
                tests.join(", ")
            ));
        }
        reason.push_str("\nFix these before you finish.");
        Ok(Some(Reply::Block(reason)))
    }

    /// Takes, once per session, the problems the index holds before it
    /// learns of the agent's edits: they were there before the agent, so
    /// they never block it. Files whose indexed content is already the
    /// agent's are left out, so their problems still count as the agent's.
    fn take_baseline(&self, session: &mut Session, touched: &[String]) {
        if session.baseline_problems.is_some() {
            return;
        }
        let Ok(conn) = db::open_ro(&self.ws.db_path) else {
            return;
        };
        let root = &self.ws.root;
        let since_commit = session
            .baseline
            .clone()
            .and_then(|rev| query::changed_paths(root, &Changes::Since(rev)).ok());
        let mut candidates: BTreeSet<String> = session.edited.clone();
        candidates.extend(touched.iter().cloned());
        candidates.extend(since_commit.iter().flatten().cloned());
        let skip: HashSet<String> = candidates
            .into_iter()
            .filter(|path| {
                let changed = match session.dirty.get(path) {
                    Some(hash) => *hash != content_hash(&root.join(path)),
                    None => since_commit.as_ref().is_none_or(|c| c.contains(path)),
                };
                let indexed = db::file_sha(&conn, path).ok().flatten().map(|(_, sha)| sha);
                let current = fs::read(root.join(path)).ok().map(|b| text_sha(&b));
                changed && indexed == current
            })
            .collect();
        let contract_paths: HashSet<String> = session
            .dirty
            .iter()
            .filter(|(path, hash)| {
                !skip.contains(*path) && **hash == content_hash(&root.join(path))
            })
            .map(|(path, _)| path.clone())
            .take(BASELINE_CONTRACT_FILES)
            .collect();
        if let Ok(keys) = query::problem_keys(&conn, root, &skip, &contract_paths) {
            let mut counts = BTreeMap::new();
            for key in keys {
                *counts.entry(short_hash(&key)).or_insert(0) += 1;
            }
            session.baseline_problems = Some(counts);
        }
    }
}

/// The agent's problems: findings whose key has more findings now than when
/// the session began. The others stay in the result as advisories. Returns
/// the findings and the agent's keys, counted.
fn judge(found: Vec<Problem>, session: &Session) -> (Vec<Finding>, BTreeMap<String, u32>) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for problem in found.iter().filter(|p| p.finding.blocking) {
        *counts.entry(problem.key.clone()).or_insert(0) += 1;
    }
    let before = |key: &str| {
        session
            .baseline_problems
            .as_ref()
            .and_then(|b| b.get(&short_hash(key)).copied())
            .unwrap_or(0)
    };
    counts.retain(|key, count| *count > before(key));
    let findings = found
        .into_iter()
        .map(|mut p| {
            p.finding.blocking &= counts.contains_key(&p.key);
            p.finding
        })
        .collect();
    (findings, counts)
}

/// Blocking findings as `id path message`, for the session's record.
fn blocked(result: &VerifyResult) -> Vec<String> {
    result
        .findings
        .iter()
        .filter(|f| f.blocking)
        .map(|f| {
            format!(
                "{} {} {}",
                f.id,
                f.path.as_deref().unwrap_or("-"),
                f.message
            )
        })
        .collect()
}

/// How many problems survived from `before` to `now`, and how many went
/// away; one fewer call site of a missing name counts as fixed.
fn compare(before: &BTreeMap<String, u32>, now: &BTreeMap<String, u32>) -> (u32, u32) {
    let mut persisting = 0;
    let mut fixed = 0;
    for (key, &was) in before {
        let is = now.get(key).copied().unwrap_or(0);
        persisting += was.min(is);
        fixed += was.saturating_sub(is);
    }
    (persisting, fixed)
}

/// A problem key as stored in the session: short, since the baseline holds
/// one per problem in the repository.
fn short_hash(key: &str) -> String {
    session::hash(key.as_bytes())[..16].to_string()
}

/// The content hash the indexer stores: of the text as decoded, without a
/// byte-order mark.
fn text_sha(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    session::hash(text.strip_prefix('\u{feff}').unwrap_or(&text).as_bytes())
}

/// Line ranges (1-based, inclusive) of every place `old_texts` occurs;
/// `None` means the whole file (a write without replaced text).
fn edit_spans(source: &str, old_texts: &[String]) -> Option<Vec<(usize, usize)>> {
    if old_texts.is_empty() {
        return None;
    }
    let mut spans = Vec::new();
    for text in old_texts {
        let extra = text.matches('\n').count();
        for (offset, _) in source.match_indices(text.as_str()) {
            let start = source[..offset].matches('\n').count() + 1;
            spans.push((start, start + extra));
        }
    }
    Some(spans)
}

/// The innermost symbols overlapping `spans` (all symbols when `None`).
fn touched(symbols: Vec<SymbolRecord>, spans: Option<&[(usize, usize)]>) -> Vec<SymbolRecord> {
    let overlapping: Vec<SymbolRecord> = symbols
        .into_iter()
        .filter(|s| {
            spans.is_none_or(|spans| {
                spans
                    .iter()
                    .any(|&(start, end)| s.start_line <= end && s.end_line >= start)
            })
        })
        .collect();
    overlapping
        .iter()
        .filter(|outer| {
            !overlapping.iter().any(|inner| {
                inner.id != outer.id
                    && inner.start_line >= outer.start_line
                    && inner.end_line <= outer.end_line
                    && (inner.start_line, inner.end_line) != (outer.start_line, outer.end_line)
            })
        })
        .cloned()
        .collect()
}

/// One line about `symbol`'s uses in other files, with how many there are.
fn note(conn: &Connection, symbol: &SymbolRecord) -> Result<Option<(usize, String)>> {
    let sites: Vec<String> = graph::incoming_refs(conn, &[symbol.id], &symbol.name)?
        .into_iter()
        .filter(|(reference, confidence, _)| {
            *confidence != Confidence::Name && reference.path != symbol.path
        })
        .map(|(reference, _, _)| format!("{}:{}", reference.path, reference.line))
        .collect();
    if sites.is_empty() {
        return Ok(None);
    }
    let mut line = format!(
        "`{}` ({}:{}) has {} use(s) in other files: {}",
        symbol.qualified_name,
        symbol.path,
        symbol.start_line,
        sites.len(),
        sites[..sites.len().min(NOTE_SITES)].join(", ")
    );
    if sites.len() > NOTE_SITES {
        line.push_str(&format!(" and {} more", sites.len() - NOTE_SITES));
    }
    let mut tests: Vec<String> = graph::tests_reaching(conn, symbol.id, TEST_DEPTH, TEST_SCAN_CAP)?
        .into_iter()
        .map(|t| t.path)
        .collect();
    tests.dedup();
    if !tests.is_empty() {
        tests.truncate(NOTE_TESTS);
        line.push_str(&format!("; tests: {}", tests.join(", ")));
    }
    line.push('.');
    Ok(Some((sites.len(), line)))
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_from_every_agent() {
        for name in ["PreToolUse", "preToolUse", "BeforeTool", "pre-edit"] {
            assert_eq!(Event::parse(name), Some(Event::PreEdit), "{name}");
        }
        for name in ["PostToolUse", "postToolUse", "AfterTool", "afterFileEdit"] {
            assert_eq!(Event::parse(name), Some(Event::PostEdit), "{name}");
        }
        for name in ["Stop", "agentStop", "AfterAgent", "stop"] {
            assert_eq!(Event::parse(name), Some(Event::Stop), "{name}");
        }
        assert_eq!(Event::parse("SubagentStop"), None);
    }

    #[test]
    fn spans_cover_each_occurrence() {
        let source = "a\nfoo()\nb\nfoo()\n";
        assert_eq!(
            edit_spans(source, &["foo()\nb".to_string()]),
            Some(vec![(2, 3)])
        );
        assert_eq!(
            edit_spans(source, &["foo()".to_string()]),
            Some(vec![(2, 2), (4, 4)])
        );
        assert_eq!(edit_spans(source, &[]), None);
    }

    #[test]
    fn unknown_agents_get_nothing() {
        assert_eq!(Agent::parse("windsurf"), None);
        assert_eq!(
            render(
                Agent::Cursor,
                Event::PostEdit,
                "afterFileEdit",
                Reply::Context("x".into())
            ),
            None
        );
        assert_eq!(
            render(
                Agent::Copilot,
                Event::PreEdit,
                "preToolUse",
                Reply::Context("x".into())
            ),
            None
        );
    }
}
