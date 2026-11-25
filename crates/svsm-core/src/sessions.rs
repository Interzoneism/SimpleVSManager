//! Game sessions read from the game's client logs, for the "Confirm working mods" prompt.
//! Port of `GameSessionMonitor` + `ModUsageTracking`, simplified: instead of tailing logs with
//! watchers, the logs are scanned when the manager asks (start-up, window focus). A session runs
//! from "Received level finalize" to "Exiting current game"; sessions of 30+ minutes count.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MIN_SESSION_MINUTES: f64 = 30.0;
/// Long sessions before asking (2.x `GameSessionVoteThreshold`).
pub const SESSIONS_BEFORE_PROMPT: u32 = 5;
const START_MARKER: &str = "Received level finalize";
const END_MARKER: &str = "Exiting current game";

/// Parses the log's leading `d.M.yyyy HH:mm:ss` into a sortable key (seconds, local time).
fn timestamp(line: &str) -> Option<i64> {
    let mut parts = line.split_whitespace();
    let (date, time) = (parts.next()?, parts.next()?);
    let mut d = date.split('.').map(|p| p.parse::<i64>().ok());
    let (day, month, year) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (h, m, s) = (t.next()??, t.next()??, t.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let date = chrono::NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)?;
    let dt = date.and_hms_opt(h as u32, m as u32, s as u32)?;
    Some(dt.and_utc().timestamp())
}

/// (start, end) of each finished session in one log, in seconds.
pub fn sessions_in(text: &str) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut start: Option<i64> = None;
    for line in text.lines() {
        if line.contains(START_MARKER) {
            if start.is_none() {
                start = timestamp(line);
            }
        } else if line.contains(END_MARKER) {
            if let (Some(s), Some(e)) = (start.take(), timestamp(line)) {
                if e >= s {
                    out.push((s, e));
                }
            }
        }
    }
    out
}

/// Long finished sessions in `<data>/Logs/client-main*.log` and `Logs/Archive/client-main*`.
pub fn long_sessions(logs_dir: &Path) -> Vec<i64> {
    let mut files = Vec::new();
    for dir in [logs_dir.to_path_buf(), logs_dir.join("Archive")] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if name.starts_with("client-main") && (name.ends_with(".log") || name.ends_with(".txt")) {
                    files.push(e.path());
                }
            }
        }
    }
    let mut starts = BTreeSet::new();
    for f in files {
        // Logs are text but may contain stray bytes.
        let Ok(bytes) = std::fs::read(&f) else { continue };
        for (s, e) in sessions_in(&String::from_utf8_lossy(&bytes)) {
            if (e - s) as f64 >= MIN_SESSION_MINUTES * 60.0 {
                starts.insert(s);
            }
        }
    }
    starts.into_iter().collect()
}

/// What the manager remembers between runs (stored in settings).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct UsageState {
    /// Session start times already counted (newest 100).
    pub counted: Vec<f64>,
    /// Long sessions since the last prompt.
    pub long_sessions: u32,
    /// `modid|version|game version` -> long sessions it was active in.
    pub mods: BTreeMap<String, u32>,
}

impl UsageState {
    pub fn key(mod_id: &str, version: &str, game_version: &str) -> String {
        format!("{}|{}|{}", mod_id.trim().to_lowercase(), version.trim(), game_version.trim())
    }

    /// Counts sessions not seen before, with the given active mods `(id, version)`. Returns
    /// how many were new. The first scan only remembers what's there (old sessions don't count).
    pub fn record(&mut self, long_session_starts: &[i64], active: &[(String, String)], game_version: &str, first_scan: bool) -> u32 {
        let mut new = 0;
        for s in long_session_starts {
            let s = *s as f64;
            if self.counted.contains(&s) {
                continue;
            }
            self.counted.push(s);
            if first_scan {
                continue;
            }
            new += 1;
            self.long_sessions = (self.long_sessions + 1).min(SESSIONS_BEFORE_PROMPT);
            for (id, v) in active {
                *self.mods.entry(Self::key(id, v, game_version)).or_insert(0) += 1;
            }
        }
        self.counted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let excess = self.counted.len().saturating_sub(100);
        self.counted.drain(..excess);
        new
    }

    pub fn prompt_due(&self) -> bool {
        self.long_sessions >= SESSIONS_BEFORE_PROMPT && !self.mods.is_empty()
    }

    /// Forget mods the user answered (or dismissed). Resets the session count when none remain.
    pub fn clear(&mut self, keys: &[String]) {
        self.mods.retain(|k, _| !keys.contains(k));
        if self.mods.is_empty() {
            self.long_sessions = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "24.11.2026 20:05:21 [Notification] Client logger started.
24.11.2026 20:05:52 [Notification] Received level finalize
24.11.2026 20:11:05 [Notification] Exiting current game to main menu
24.11.2026 21:00:00 [Notification] Received level finalize
24.11.2026 21:45:00 [Notification] Exiting current game to main menu
24.11.2026 23:50:00 [Notification] Received level finalize
25.11.2026 00:30:00 [Notification] Exiting current game
26.11.2026 10:00:00 [Notification] Received level finalize
";

    #[test]
    fn finds_sessions_and_long_ones() {
        let s = sessions_in(LOG);
        assert_eq!(s.len(), 3, "the unfinished one is ignored");
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("Archive")).unwrap();
        std::fs::write(t.path().join("client-main.log"), LOG).unwrap();
        std::fs::write(t.path().join("Archive/client-main-old.txt"), LOG).unwrap();
        std::fs::write(t.path().join("server-main.log"), LOG).unwrap();
        assert_eq!(long_sessions(t.path()).len(), 2, "45 and 40 minutes; duplicates across files collapse");
        assert!(timestamp("31.13.2026 00:00:00 x").is_none());
    }

    #[test]
    fn prompts_after_five_long_sessions() {
        let mut u = UsageState::default();
        let active = vec![("carryon".to_string(), "1.0".to_string())];
        assert_eq!(u.record(&[1, 2], &active, "1.22.7", true), 0, "first scan only remembers");
        for i in 3..8 {
            u.record(&[i], &active, "1.22.7", false);
        }
        assert!(u.prompt_due());
        assert_eq!(u.mods[&UsageState::key("CarryOn", "1.0", "1.22.7")], 5);
        assert_eq!(u.record(&[3, 4], &active, "1.22.7", false), 0, "already counted");
        u.clear(&[UsageState::key("carryon", "1.0", "1.22.7")]);
        assert!(!u.prompt_due() && u.long_sessions == 0);
    }
}
