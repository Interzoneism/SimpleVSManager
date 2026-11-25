//! User reports ("does this mod version work?"), shared with 2.x under
//! `compatVotes/{mod key}/{version key}/users/{firebase user}`. Keys are URL-safe base64 of the
//! trimmed id/version. Reading is public; writing needs the user's own sign-in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::auth::Authenticator;
use super::{encode_component, http_client, now_iso, CloudConfig, CloudError, CloudResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum VoteOption {
    FullyFunctional,
    NoIssuesSoFar,
    SomeIssuesButWorks,
    NotFunctional,
    CrashesOrFreezesGame,
}

impl VoteOption {
    /// 2.x also accepted these older names.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "fullyfunctional" | "workingperfectly" => Self::FullyFunctional,
            "noissuessofar" => Self::NoIssuesSoFar,
            "someissuesbutworks" => Self::SomeIssuesButWorks,
            "notfunctional" | "notworking" => Self::NotFunctional,
            "crashesorfreezesgame" => Self::CrashesOrFreezesGame,
            _ => return None,
        })
    }

    pub fn needs_comment(self) -> bool {
        matches!(self, Self::NotFunctional | Self::CrashesOrFreezesGame)
    }

    fn key(self) -> &'static str {
        match self {
            Self::FullyFunctional => "fullyFunctional",
            Self::NoIssuesSoFar => "noIssuesSoFar",
            Self::SomeIssuesButWorks => "someIssuesButWorks",
            Self::NotFunctional => "notFunctional",
            Self::CrashesOrFreezesGame => "crashesOrFreezesGame",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct VoteSummary {
    pub fully_functional: u32,
    pub no_issues_so_far: u32,
    pub some_issues_but_works: u32,
    pub not_functional: u32,
    pub crashes_or_freezes_game: u32,
    /// Comments from "not functional" and "crashes" reports.
    pub problem_comments: Vec<String>,
    pub my_vote: Option<VoteOption>,
    pub my_comment: Option<String>,
    pub fetched_ms: f64,
}

impl VoteSummary {
    pub fn total(&self) -> u32 {
        self.fully_functional + self.no_issues_so_far + self.some_issues_but_works + self.not_functional + self.crashes_or_freezes_game
    }
}

/// URL-safe base64 without padding, like 2.x `SanitizeKey`.
pub fn key(value: &str) -> String {
    let t = value.trim();
    if t.is_empty() {
        return "_".into();
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(t.as_bytes())
}

pub fn summarize(users: &Value, me: Option<&str>) -> VoteSummary {
    let mut s = VoteSummary { fetched_ms: crate::moddb::dates::now_ms(), ..Default::default() };
    let Some(map) = users.as_object() else { return s };
    for (user, rec) in map {
        let Some(option) = rec.get("option").and_then(Value::as_str).and_then(VoteOption::parse) else { continue };
        let comment = rec.get("comment").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty()).map(String::from);
        match option {
            VoteOption::FullyFunctional => s.fully_functional += 1,
            VoteOption::NoIssuesSoFar => s.no_issues_so_far += 1,
            VoteOption::SomeIssuesButWorks => s.some_issues_but_works += 1,
            VoteOption::NotFunctional => s.not_functional += 1,
            VoteOption::CrashesOrFreezesGame => s.crashes_or_freezes_game += 1,
        }
        if option.needs_comment() {
            if let Some(c) = &comment {
                if !s.problem_comments.contains(c) {
                    s.problem_comments.push(c.clone());
                }
            }
        }
        if me == Some(user.as_str()) {
            s.my_vote = Some(option);
            s.my_comment = comment;
        }
    }
    s
}

pub struct Votes<'a> {
    auth: &'a Authenticator,
    config: CloudConfig,
    http: reqwest::Client,
    cache_file: PathBuf,
}

impl<'a> Votes<'a> {
    pub fn new(auth: &'a Authenticator, config: CloudConfig, cache_dir: &Path) -> CloudResult<Self> {
        Ok(Self { auth, config, http: http_client()?, cache_file: cache_dir.join("svsm3-votes.json") })
    }

    fn url(&self, mod_id: &str, version: &str, user: Option<&str>, token: Option<&str>) -> String {
        let mut u = format!("{}/compatVotes/{}/{}/users", self.config.db_url.trim_end_matches('/'), key(mod_id), key(version));
        if let Some(user) = user {
            u.push('/');
            u.push_str(&encode_component(user));
        }
        u.push_str(".json");
        if let Some(t) = token {
            u.push_str("?auth=");
            u.push_str(&encode_component(t));
        }
        u
    }

    fn cache_key(mod_id: &str, version: &str) -> String {
        format!("{}@{}", mod_id.trim().to_lowercase(), version.trim())
    }

    pub fn cached(&self) -> HashMap<String, VoteSummary> {
        std::fs::read_to_string(&self.cache_file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn store(&self, mod_id: &str, version: &str, s: &VoteSummary) {
        let mut all = self.cached();
        all.insert(Self::cache_key(mod_id, version), s.clone());
        if let Some(dir) = self.cache_file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.cache_file, serde_json::to_string(&all).unwrap_or_default());
    }

    /// Reports for one mod version (public read; marks the user's own vote when known).
    pub async fn summary(&self, mod_id: &str, version: &str) -> CloudResult<VoteSummary> {
        let resp = self.http.get(self.url(mod_id, version, None, None)).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let users: Value = match status.as_u16() {
            200 => serde_json::from_str(&text).unwrap_or(Value::Null),
            404 => Value::Null,
            code => return Err(CloudError::BadResponse(format!("Reading reports: HTTP {code}"))),
        };
        let s = summarize(&users, self.auth.user_id().as_deref());
        self.store(mod_id, version, &s);
        Ok(s)
    }

    pub async fn submit(&self, mod_id: &str, version: &str, game_version: &str, option: VoteOption, comment: Option<&str>) -> CloudResult<VoteSummary> {
        let comment = comment.map(str::trim).filter(|c| !c.is_empty());
        if option.needs_comment() && comment.is_none() {
            return Err(CloudError::Invalid("Say briefly what goes wrong, so others know what to expect.".into()));
        }
        if mod_id.trim().is_empty() || version.trim().is_empty() || game_version.trim().is_empty() {
            return Err(CloudError::Invalid("A report needs the mod, its version and the game version.".into()));
        }
        let session = self.auth.session(false).await?;
        let mut rec = json!({ "option": option.key(), "vintageStoryVersion": game_version.trim(), "updatedUtc": now_iso() });
        if let Some(c) = comment {
            rec["comment"] = Value::String(c.to_string());
        }
        let resp = self
            .http
            .put(self.url(mod_id, version, Some(&session.user_id), Some(&session.id_token)))
            .header("Content-Type", "application/json")
            .body(rec.to_string())
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(CloudError::Denied(format!("Sending the report: HTTP {}", resp.status())));
        }
        self.summary(mod_id, version).await
    }

    pub async fn remove(&self, mod_id: &str, version: &str) -> CloudResult<VoteSummary> {
        let session = self.auth.session(false).await?;
        let resp = self.http.delete(self.url(mod_id, version, Some(&session.user_id), Some(&session.id_token))).send().await?;
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            return Err(CloudError::Denied(format!("Removing the report: HTTP {}", resp.status())));
        }
        self.summary(mod_id, version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_2x() {
        assert_eq!(key(" carryon "), "Y2Fycnlvbg");
        assert_eq!(key("1.2.0?>"), "MS4yLjA_Pg");
        assert_eq!(key("  "), "_");
    }

    #[test]
    fn summarizes_and_finds_my_vote() {
        let users = json!({
            "a": {"option": "fullyFunctional"}, "b": {"option": "workingPerfectly"},
            "me": {"option": "notWorking", "comment": " crashes on load "}, "c": {"option": "bogus"}
        });
        let s = summarize(&users, Some("me"));
        assert_eq!((s.fully_functional, s.not_functional, s.total()), (2, 1, 3));
        assert_eq!(s.my_vote, Some(VoteOption::NotFunctional));
        assert_eq!(s.problem_comments, ["crashes on load"]);
    }
}
