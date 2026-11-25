//! Cloud modlists and community reports, on the same Firebase project as Simple VS Manager 2.x.
//! Ports `FirebaseAnonymousAuthenticator`, `FirebaseModlistStore` (with the subscription work
//! from SVSM-Dev 8f81f69), `ModVersionVoteService` and `OnlineModlistContentValidator`.
//!
//! Not ported: the legacy Firebase project migration (users update to 2.3 first) and the
//! best-effort `adminRegistry` write (the database rules reject it anyway).

pub mod auth;
pub mod migrate;
pub mod store;
pub mod validator;
pub mod votes;

use std::time::Duration;

pub use auth::{Authenticator, Session};
pub use store::{CloudStore, PlayerIdentity, RegistryEntry, SlotInfo, SubscriptionSnapshot};

/// Where the cloud lives. Firebase web API keys are public client keys by design; access is
/// controlled by the database rules (`firebase.rules.json` in SVSM-Dev).
#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub db_url: String,
    pub api_key: String,
    pub sign_up_url: String,
    pub refresh_url: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            db_url: "https://simplevsmanager-default-rtdb.europe-west1.firebasedatabase.app".into(),
            api_key: "AIzaSyBjIEr_JbB-Fx9hMa9nAOwaUyPP82HEeG4".into(),
            sign_up_url: "https://identitytoolkit.googleapis.com/v1/accounts:signUp".into(),
            refresh_url: "https://securetoken.googleapis.com/v1/token".into(),
        }
    }
}

impl CloudConfig {
    /// The first Firebase project (before 2.3.0). Only used to move identities off it.
    pub fn legacy() -> Self {
        Self {
            db_url: "https://simple-vs-manager-default-rtdb.europe-west1.firebasedatabase.app".into(),
            api_key: "AIzaSyCmDJ9yC1ccUEUf41fC-SI8fuXFJzWWlHY".into(),
            ..Self::default()
        }
    }

    /// Everything on one test server.
    pub fn for_tests(base: &str) -> Self {
        let b = base.trim_end_matches('/');
        Self { db_url: format!("{b}/db"), api_key: "test-key".into(), sign_up_url: format!("{b}/signUp"), refresh_url: format!("{b}/token") }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum CloudError {
    #[error("Offline mode is on. Turn it off to use online modlists.")]
    Offline,
    #[error("Couldn't reach the cloud: {0}")]
    Http(String),
    #[error("The cloud refused this ({0}). It may not be allowed, or the sign-in expired.")]
    Denied(String),
    #[error("The cloud sent something unexpected: {0}")]
    BadResponse(String),
    #[error("Couldn't sign in to the cloud: {0}. Your saved identity (firebase-auth.json) was kept as it is.")]
    SignIn(String),
    #[error("Your cloud identity is from the old Simple VS Manager cloud project. Publishing and reports need the one-time move to the new project.")]
    LegacyIdentity,
    #[error("{0}")]
    Invalid(String),
    #[error("Couldn't write {path}: {message}")]
    Io { path: String, message: String },
}

pub type CloudResult<T> = Result<T, CloudError>;

impl From<reqwest::Error> for CloudError {
    fn from(e: reqwest::Error) -> Self {
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        CloudError::Http(msg)
    }
}

pub(crate) fn http_client() -> CloudResult<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("SimpleVSManager/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()?)
}

/// Minimal `application/x-www-form-urlencoded` / URL component encoding.
pub(crate) fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Firebase keys can't contain `. $ # [ ] /` or control characters (2.x replaced them with `_`).
pub fn sanitize_key(s: &str) -> String {
    s.chars().map(|c| if ".$#[]/".contains(c) || (c as u32) < 32 || c as u32 == 127 { '_' } else { c }).collect()
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub(crate) fn parse_iso_ms(s: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.timestamp_millis() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_encoding() {
        assert_eq!(sanitize_key("a.b$c#d[e]f/g"), "a_b_c_d_e_f_g");
        assert_eq!(encode_component("a b/+="), "a%20b%2F%2B%3D");
        assert!(parse_iso_ms("2025-11-29T19:28:21.2797773+00:00").is_some());
        assert!(parse_iso_ms(&now_iso()).is_some());
    }
}
