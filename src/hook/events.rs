//! A local log of what the hooks did, for `tessera stats`: one line per hook
//! run with the event, the kind of answer, how many problems were open and
//! how many the edit fixed, and how long it took. No paths, names, messages
//! or code are written, and nothing leaves the machine.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::output::AgentText;

pub const EVENTS_FILE: &str = "events.jsonl";
/// Past this size the oldest half of the log is dropped.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// A hash of the agent's session id.
    pub session: String,
    pub agent: String,
    /// `session-start`, `pre-edit`, `post-edit` or `stop`.
    pub event: String,
    /// `none`, `note`, `report` or `block`.
    pub reply: String,
    /// Problems the agent introduced that are still open after this run.
    pub open: u32,
    /// Problems this run found fixed since the last check.
    pub fixed: u32,
    /// Problems set aside at the stop limit, which no longer block.
    pub released: u32,
    pub ms: u64,
}

pub fn path(index_dir: &Path) -> PathBuf {
    index_dir.join(EVENTS_FILE)
}

/// Appends `event`; a failure is ignored, as hooks fail open.
pub fn record(index_dir: &Path, event: &HookEvent) {
    let file = path(index_dir);
    if fs::metadata(&file).is_ok_and(|m| m.len() > MAX_BYTES) {
        if let Ok(text) = fs::read_to_string(&file) {
            let lines: Vec<&str> = text.lines().collect();
            let kept = lines[lines.len() / 2..].join("\n");
            let _ = fs::write(&file, format!("{kept}\n"));
        }
    }
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    if let Ok(mut out) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(out, "{line}");
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Latency {
    pub count: usize,
    pub p50: u64,
    pub p95: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub days: u32,
    pub sessions: usize,
    /// Hook runs by agent.
    pub agents: BTreeMap<String, usize>,
    pub edits_checked: usize,
    pub notes: usize,
    /// Post-edit answers that told the agent about problems without
    /// stopping it.
    pub reports: usize,
    pub post_edit_blocks: usize,
    pub stop_blocks: usize,
    pub problems_fixed: u64,
    pub problems_released: u64,
    /// Problems still open at the last check of each session.
    pub problems_open: u64,
    pub latency_ms: BTreeMap<String, Latency>,
    pub baseline: usize,
    pub accepted: usize,
    pub accepted_expired: usize,
}

/// What the hooks did in the last `days` days.
pub fn stats(index_dir: &Path, root: &Path, days: u32, now_ms: u64) -> Stats {
    let since = now_ms.saturating_sub(u64::from(days) * 86_400_000);
    let events: Vec<HookEvent> = fs::read_to_string(path(index_dir))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|e: &HookEvent| e.at >= since)
        .collect();
    let mut stats = Stats {
        days,
        ..Stats::default()
    };
    let mut sessions = BTreeSet::new();
    let mut last_open: BTreeMap<&str, u32> = BTreeMap::new();
    let mut times: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for event in &events {
        sessions.insert(event.session.as_str());
        *stats.agents.entry(event.agent.clone()).or_default() += 1;
        match (event.event.as_str(), event.reply.as_str()) {
            ("post-edit", reply) => {
                stats.edits_checked += 1;
                match reply {
                    "report" => stats.reports += 1,
                    "block" => stats.post_edit_blocks += 1,
                    _ => {}
                }
            }
            ("pre-edit", "note") => stats.notes += 1,
            ("stop", "block") => stats.stop_blocks += 1,
            _ => {}
        }
        stats.problems_fixed += u64::from(event.fixed);
        stats.problems_released += u64::from(event.released);
        if event.event != "session-start" {
            last_open.insert(&event.session, event.open);
            times
                .entry(event.event.replace('-', "_"))
                .or_default()
                .push(event.ms);
        }
    }
    stats.sessions = sessions.len();
    stats.problems_open = last_open.values().map(|n| u64::from(*n)).sum();
    for (event, mut ms) in times {
        ms.sort_unstable();
        let at = |q: usize| ms[(ms.len() * q / 100).min(ms.len() - 1)];
        stats.latency_ms.insert(
            event,
            Latency {
                count: ms.len(),
                p50: at(50),
                p95: at(95),
            },
        );
    }
    let waivers = crate::query::waivers::summary(root);
    stats.baseline = waivers.baseline;
    stats.accepted = waivers.accepted;
    stats.accepted_expired = waivers.expired;
    stats
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.agent_text())
    }
}

impl AgentText for Stats {
    fn agent_text(&self) -> String {
        if self.sessions == 0 {
            return format!(
                "No hook activity in the last {} days. Run `tessera init` to wire the hooks into your agents.\n",
                self.days
            );
        }
        let agents: Vec<String> = self
            .agents
            .iter()
            .map(|(agent, n)| format!("{agent} {n}"))
            .collect();
        let mut out = format!(
            "Last {} days: {} session(s) ({})\n",
            self.days,
            self.sessions,
            agents.join(", ")
        );
        out.push_str(&format!(
            "  {} edits checked, {} pre-edit notes\n  {} reports, {} post-edit blocks, {} stop blocks\n  {} problems fixed by the agent, {} released at the stop limit, {} still open\n",
            self.edits_checked,
            self.notes,
            self.reports,
            self.post_edit_blocks,
            self.stop_blocks,
            self.problems_fixed,
            self.problems_released,
            self.problems_open
        ));
        for (event, latency) in &self.latency_ms {
            out.push_str(&format!(
                "  {} hook: p50 {} ms, p95 {} ms over {} runs\n",
                event.replace('_', "-"),
                latency.p50,
                latency.p95,
                latency.count
            ));
        }
        out.push_str(&format!(
            "  baseline: {} findings; accepted breaks: {} ({} expired)\n",
            self.baseline, self.accepted, self.accepted_expired
        ));
        out
    }
}
