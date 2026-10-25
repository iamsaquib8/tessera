//! Findings a team has chosen to live with: a baseline of what was already
//! there when Tessera was adopted (`.tessera/baseline.json`), and accepted
//! breaks with a reason and an expiry (`.tessera/accepted.toml`). A waived
//! finding is reported, never blocking. A file that cannot be read waives
//! nothing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::verify::Finding;

pub const BASELINE_FILE: &str = "baseline.json";
pub const ACCEPTED_FILE: &str = "accepted.toml";
/// Bumped when fingerprints change meaning; an older baseline then waives
/// nothing instead of waiving the wrong findings.
pub const FINGERPRINT_VERSION: u32 = 1;

/// Why a finding does not block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Waiver {
    /// `baseline` or `accepted`.
    pub by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Who accepted it, when the entry says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_by: Option<String>,
    /// An acceptance past its date: shown, but the finding blocks again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineFile {
    pub fingerprints: u32,
    pub findings: Vec<BaselineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BaselineEntry {
    pub fingerprint: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Default, Deserialize)]
struct AcceptedFile {
    #[serde(default)]
    accept: Vec<Accepted>,
}

#[derive(Debug, Clone, Deserialize)]
struct Accepted {
    fingerprint: String,
    reason: String,
    expires: Option<String>,
    by: Option<String>,
}

#[derive(Debug, Default)]
pub struct Waivers {
    baseline: HashSet<String>,
    accepted: HashMap<String, Accepted>,
}

pub fn dir(root: &Path) -> PathBuf {
    root.join(".tessera")
}

impl Waivers {
    /// Waives nothing: `verify --no-baseline`, and building a baseline.
    pub fn none() -> Self {
        Self::default()
    }

    /// The repository's waivers. A file that is missing waives nothing; one
    /// that cannot be read is named on stderr and waives nothing.
    pub fn load(root: &Path) -> Self {
        let mut waivers = Self::default();
        let baseline = dir(root).join(BASELINE_FILE);
        if let Ok(text) = fs::read_to_string(&baseline) {
            match serde_json::from_str::<BaselineFile>(&text) {
                Ok(file) if file.fingerprints == FINGERPRINT_VERSION => {
                    waivers.baseline = file.findings.into_iter().map(|e| e.fingerprint).collect();
                }
                Ok(_) => eprintln!(
                    "tessera: {} was written with older fingerprints and is ignored; run `tessera baseline` again",
                    baseline.display()
                ),
                Err(error) => eprintln!(
                    "tessera: {} is ignored, it is not a baseline: {error}",
                    baseline.display()
                ),
            }
        }
        let accepted = dir(root).join(ACCEPTED_FILE);
        if let Ok(text) = fs::read_to_string(&accepted) {
            match toml::from_str::<AcceptedFile>(&text) {
                Ok(file) => {
                    waivers.accepted = file
                        .accept
                        .into_iter()
                        .map(|a| (a.fingerprint.clone(), a))
                        .collect();
                }
                Err(error) => eprintln!(
                    "tessera: {} is ignored, it is not valid TOML: {}",
                    accepted.display(),
                    error.message()
                ),
            }
        }
        waivers
    }

    /// Marks `finding` waived, and not blocking, when a waiver covers it.
    pub fn apply(&self, finding: &mut Finding) {
        if let Some(accepted) = self.accepted.get(&finding.fingerprint) {
            let expired = accepted
                .expires
                .as_deref()
                .is_some_and(|date| date < today().as_str());
            finding.waived = Some(Waiver {
                by: "accepted".to_string(),
                reason: Some(accepted.reason.clone()),
                expires: accepted.expires.clone(),
                accepted_by: accepted.by.clone(),
                expired,
            });
            if !expired {
                finding.blocking = false;
            }
        } else if finding.blocking && self.baseline.contains(&finding.fingerprint) {
            finding.waived = Some(Waiver {
                by: "baseline".to_string(),
                reason: None,
                expires: None,
                accepted_by: None,
                expired: false,
            });
            finding.blocking = false;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineReport {
    pub path: String,
    /// Entries in the baseline now.
    pub findings: usize,
    /// Entries dropped because they no longer fire (with `--prune`).
    pub pruned: usize,
}

impl std::fmt::Display for BaselineReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::output::AgentText;
        f.write_str(&self.agent_text())
    }
}

impl crate::output::AgentText for BaselineReport {
    fn agent_text(&self) -> String {
        let pruned = if self.pruned > 0 {
            format!(", {} dropped that no longer fire", self.pruned)
        } else {
            String::new()
        };
        format!(
            "Baseline: {} finding(s) in {}{pruned}. Only new findings block from now on.\n",
            self.findings, self.path
        )
    }
}

/// Every blocking finding the index holds now, in every file.
fn everything(conn: &rusqlite::Connection, root: &Path) -> Result<Vec<Finding>> {
    let mut stmt = conn.prepare("SELECT path FROM files")?;
    let paths: HashSet<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let changes = super::verify::Changes::WorkingTree;
    Ok(
        super::verify::problems_with(conn, root, &paths, &changes, &Waivers::none())?
            .into_iter()
            .map(|p| p.finding)
            .filter(|f| f.blocking)
            .collect(),
    )
}

/// `tessera baseline`: today's blocking findings become the baseline. With
/// `prune`, only entries that still fire are kept and none are added.
pub fn baseline(conn: &rusqlite::Connection, root: &Path, prune: bool) -> Result<BaselineReport> {
    let now = everything(conn, root)?;
    let before = baseline_fingerprints(root);
    let kept: Vec<Finding> = if prune {
        now.into_iter()
            .filter(|f| before.contains(&f.fingerprint))
            .collect()
    } else {
        now
    };
    let findings = write_baseline(root, &kept)?;
    let pruned = if prune {
        before.len().saturating_sub(findings)
    } else {
        0
    };
    Ok(BaselineReport {
        path: format!(".tessera/{BASELINE_FILE}"),
        findings,
        pruned,
    })
}

/// Writes the baseline of `findings` (the blocking ones), sorted, and
/// returns how many it holds.
pub fn write_baseline(root: &Path, findings: &[Finding]) -> Result<usize> {
    let mut unique: BTreeMap<String, BaselineEntry> = BTreeMap::new();
    for finding in findings.iter().filter(|f| f.blocking) {
        unique.insert(
            finding.fingerprint.clone(),
            BaselineEntry {
                fingerprint: finding.fingerprint.clone(),
                id: finding.id.clone(),
                path: finding.path.clone(),
                message: finding.message.clone(),
            },
        );
    }
    let file = BaselineFile {
        fingerprints: FINGERPRINT_VERSION,
        findings: unique.into_values().collect(),
    };
    let count = file.findings.len();
    let dir = dir(root);
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = dir.join(BASELINE_FILE);
    let temp = dir.join(format!(".{BASELINE_FILE}.tmp"));
    fs::write(&temp, format!("{}\n", serde_json::to_string_pretty(&file)?))?;
    fs::rename(&temp, &path).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(count)
}

#[derive(Debug, Clone, Default)]
pub struct WaiverSummary {
    pub baseline: usize,
    pub accepted: usize,
    pub expired: usize,
}

/// How many findings the baseline and the accepted breaks cover.
pub fn summary(root: &Path) -> WaiverSummary {
    let accepted: Vec<Accepted> = fs::read_to_string(dir(root).join(ACCEPTED_FILE))
        .ok()
        .and_then(|text| toml::from_str::<AcceptedFile>(&text).ok())
        .map(|file| file.accept)
        .unwrap_or_default();
    let today = today();
    WaiverSummary {
        baseline: baseline_fingerprints(root).len(),
        expired: accepted
            .iter()
            .filter(|a| a.expires.as_deref().is_some_and(|d| d < today.as_str()))
            .count(),
        accepted: accepted.len(),
    }
}

/// The fingerprints the current baseline holds, if there is one.
pub fn baseline_fingerprints(root: &Path) -> HashSet<String> {
    fs::read_to_string(dir(root).join(BASELINE_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<BaselineFile>(&text).ok())
        .filter(|file| file.fingerprints == FINGERPRINT_VERSION)
        .map(|file| file.findings.into_iter().map(|e| e.fingerprint).collect())
        .unwrap_or_default()
}

/// Appends an accepted break to `.tessera/accepted.toml`, keeping the rest
/// of the file as it is.
pub fn accept(
    root: &Path,
    fingerprint: &str,
    reason: &str,
    expires: Option<&str>,
    by: Option<&str>,
) -> Result<PathBuf> {
    if fingerprint.len() != 16 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("`{fingerprint}` is not a finding fingerprint (16 hex digits, from `verify --json`)");
    }
    if reason.trim().is_empty() {
        bail!("an accepted break needs a --reason");
    }
    if let Some(date) = expires {
        let valid = date.len() == 10
            && date.char_indices().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            });
        if !valid {
            bail!("--expires takes a date like 2026-12-31, not `{date}`");
        }
    }
    let dir = dir(root);
    fs::create_dir_all(&dir)?;
    let path = dir.join(ACCEPTED_FILE);
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() {
        text.push('\n');
    }
    // JSON string escapes are valid TOML basic-string escapes.
    let quote = |value: &str| serde_json::to_string(value).unwrap_or_default();
    text.push_str(&format!(
        "[[accept]]\nfingerprint = {}\nreason = {}\n",
        quote(fingerprint),
        quote(reason)
    ));
    if let Some(date) = expires {
        text.push_str(&format!("expires = {}\n", quote(date)));
    }
    if let Some(by) = by {
        text.push_str(&format!("by = {}\n", quote(by)));
    }
    toml::from_str::<AcceptedFile>(&text)
        .map_err(|e| anyhow::anyhow!("{} would not parse: {}", path.display(), e.message()))?;
    fs::write(&path, text)?;
    Ok(path)
}

/// Today as `YYYY-MM-DD`, in UTC.
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_is_an_iso_date() {
        let date = today();
        assert_eq!(date.len(), 10);
        assert!(date.as_str() > "2026-01-01", "{date}");
    }
}
