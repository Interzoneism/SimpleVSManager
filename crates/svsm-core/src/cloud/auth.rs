//! Anonymous Firebase sign-in that keeps the user's 2.x cloud identity.
//!
//! The identity is `firebase-auth.json` in the manager folder (2.x owns it). Losing it loses
//! ownership of the user's cloud modlists, so this module:
//! * never changes or deletes an existing `firebase-auth.json`; refreshed tokens go to our own
//!   `svsm3-firebase-session.json`,
//! * restores it from `%LOCALAPPDATA%/SVSM Backup/firebase-auth.json` when it's missing,
//! * never signs up a new account when an identity exists but can't be refreshed (it reports an
//!   error instead), and only creates `firebase-auth.json` (plus the backup) when there was none.

use crate::sync::LockExt as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{encode_component, http_client, parse_iso_ms, CloudConfig, CloudError, CloudResult};

pub const IDENTITY_FILE: &str = "firebase-auth.json";
pub const SESSION_FILE: &str = "svsm3-firebase-session.json";
/// Refresh this long before the token expires.
const SKEW_MS: f64 = 2.0 * 60_000.0;

/// The 2.x file format (camelCase, ISO expiration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthFile {
    pub id_token: String,
    pub refresh_token: String,
    pub expiration_utc: String,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id_token: String,
    /// Firebase user id (`auth.uid` in the database rules).
    pub user_id: String,
    pub expires_ms: f64,
}

impl Session {
    fn fresh(&self, now: f64) -> bool {
        self.expires_ms - SKEW_MS > now
    }
}

pub struct Authenticator {
    manager_dir: PathBuf,
    backup_file: Option<PathBuf>,
    config: CloudConfig,
    http: reqwest::Client,
    cached: Mutex<Option<Session>>,
    session_file: &'static str,
}

fn now_ms() -> f64 {
    crate::moddb::dates::now_ms()
}

fn io_err(path: &Path, e: impl std::fmt::Display) -> CloudError {
    CloudError::Io { path: path.display().to_string(), message: e.to_string() }
}

/// Reads the 2.x identity file, also accepting the older shapes 2.x migrated from.
pub fn read_identity(path: &Path) -> Option<AuthFile> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let s = |keys: &[&str]| keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str)).map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    Some(AuthFile {
        id_token: s(&["idToken"])?,
        refresh_token: s(&["refreshToken"])?,
        user_id: s(&["userId", "uid", "localId"])?,
        expiration_utc: s(&["expirationUtc", "expiration", "expiresAt"]).unwrap_or_default(),
    })
}

impl Authenticator {
    /// `backup_file`: 2.x's `%LOCALAPPDATA%/SVSM Backup/firebase-auth.json` (None in tests).
    pub fn new(manager_dir: &Path, backup_file: Option<PathBuf>, config: CloudConfig) -> CloudResult<Self> {
        Ok(Self { manager_dir: manager_dir.to_path_buf(), backup_file, config, http: http_client()?, cached: Mutex::new(None), session_file: SESSION_FILE })
    }

    /// Same identity file, separate token cache (for talking to the old cloud project).
    pub fn with_session_file(mut self, name: &'static str) -> Self {
        self.session_file = name;
        self
    }

    pub fn default_backup_file() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("SVSM Backup").join(IDENTITY_FILE))
    }

    pub fn identity_path(&self) -> PathBuf {
        self.manager_dir.join(IDENTITY_FILE)
    }

    fn session_path(&self) -> PathBuf {
        self.manager_dir.join(self.session_file)
    }

    /// The saved identity, restoring it from the 2.x backup if the main file is gone.
    /// No network.
    pub fn identity(&self) -> Option<AuthFile> {
        if let Some(a) = read_identity(&self.identity_path()) {
            return Some(a);
        }
        let backup = self.backup_file.as_ref()?;
        let restored = read_identity(backup)?;
        if !self.identity_path().exists() {
            let _ = std::fs::create_dir_all(&self.manager_dir);
            if std::fs::copy(backup, self.identity_path()).is_ok() {
                tracing::info!("Restored firebase-auth.json from the SVSM Backup copy");
            }
        }
        Some(restored)
    }

    /// The Firebase user id, if there is an identity. No network.
    pub fn user_id(&self) -> Option<String> {
        self.identity().map(|a| a.user_id)
    }

    /// A valid session: cached, refreshed from the identity, or (only when there is no identity
    /// at all) a new anonymous account.
    pub async fn session(&self, force_refresh: bool) -> CloudResult<Session> {
        let now = now_ms();
        if !force_refresh {
            if let Some(s) = self.cached.lock_safe().clone().filter(|s| s.fresh(now)) {
                return Ok(s);
            }
        }
        match self.identity() {
            Some(identity) => {
                if !force_refresh {
                    let stored = self.read_session().filter(|s| s.user_id == identity.user_id && s.fresh(now));
                    let from_identity = parse_iso_ms(&identity.expiration_utc)
                        .map(|exp| Session { id_token: identity.id_token.clone(), user_id: identity.user_id.clone(), expires_ms: exp })
                        .filter(|s| s.fresh(now));
                    if let Some(s) = stored.or(from_identity) {
                        *self.cached.lock_safe() = Some(s.clone());
                        return Ok(s);
                    }
                }
                let s = self.refresh(&identity).await?;
                self.remember(&s);
                Ok(s)
            }
            None => {
                let (file, s) = self.sign_up().await?;
                self.write_new_identity(&file)?;
                self.remember(&s);
                Ok(s)
            }
        }
    }

    fn remember(&self, s: &Session) {
        *self.cached.lock_safe() = Some(s.clone());
        let path = self.session_path();
        let text = serde_json::to_string_pretty(s).expect("session serializes");
        if let Err(e) = std::fs::write(&path, text) {
            tracing::warn!("Could not cache the cloud session: {e}");
        }
    }

    fn read_session(&self) -> Option<Session> {
        serde_json::from_str(&std::fs::read_to_string(self.session_path()).ok()?).ok()
    }

    /// Forget the cached token so the next call refreshes (after the server rejected it).
    pub fn invalidate(&self) {
        *self.cached.lock_safe() = None;
        let _ = std::fs::remove_file(self.session_path());
    }

    async fn refresh(&self, identity: &AuthFile) -> CloudResult<Session> {
        let url = format!("{}?key={}", self.config.refresh_url, encode_component(&self.config.api_key));
        let body = format!("grant_type=refresh_token&refresh_token={}", encode_component(&identity.refresh_token));
        let resp = self
            .http
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| CloudError::SignIn(CloudError::from(e).to_string()))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let code = firebase_error(&text);
            if code.contains("PROJECT_NUMBER_MISMATCH") {
                return Err(CloudError::LegacyIdentity);
            }
            return Err(CloudError::SignIn(format!("token refresh returned {status} {code}")));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| CloudError::SignIn(e.to_string()))?;
        let id_token = v.get("id_token").and_then(Value::as_str).ok_or_else(|| CloudError::SignIn("no token in the reply".into()))?;
        let user_id = v.get("user_id").and_then(Value::as_str).unwrap_or(&identity.user_id);
        if user_id != identity.user_id {
            return Err(CloudError::SignIn("the refreshed account doesn't match the saved identity".into()));
        }
        Ok(Session { id_token: id_token.into(), user_id: user_id.into(), expires_ms: now_ms() + expires_in(&v) })
    }

    async fn sign_up(&self) -> CloudResult<(AuthFile, Session)> {
        let url = format!("{}?key={}", self.config.sign_up_url, encode_component(&self.config.api_key));
        let resp = self
            .http
            .post(url)
            .header("Content-Type", "application/json")
            .body(r#"{"returnSecureToken":true}"#)
            .send()
            .await
            .map_err(|e| CloudError::SignIn(CloudError::from(e).to_string()))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(CloudError::SignIn(format!("anonymous sign-up returned {status} {}", firebase_error(&text))));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| CloudError::SignIn(e.to_string()))?;
        let get = |k: &str| v.get(k).and_then(Value::as_str).map(String::from).ok_or_else(|| CloudError::SignIn(format!("no {k} in the reply")));
        let (id_token, refresh_token, user_id) = (get("idToken")?, get("refreshToken")?, get("localId")?);
        let expires_ms = now_ms() + expires_in(&v);
        let expiration_utc = chrono::DateTime::from_timestamp_millis(expires_ms as i64).map(|d| d.to_rfc3339()).unwrap_or_default();
        let file = AuthFile { id_token: id_token.clone(), refresh_token, expiration_utc, user_id: user_id.clone() };
        Ok((file, Session { id_token, user_id, expires_ms }))
    }

    /// Creates `firebase-auth.json` (refusing to overwrite one) and the 2.x backup copy.
    fn write_new_identity(&self, file: &AuthFile) -> CloudResult<()> {
        let path = self.identity_path();
        std::fs::create_dir_all(&self.manager_dir).map_err(|e| io_err(&self.manager_dir, e))?;
        let text = serde_json::to_string_pretty(file).expect("identity serializes");
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| io_err(&path, e))?;
        std::io::Write::write_all(&mut f, text.as_bytes()).map_err(|e| io_err(&path, e))?;
        if let Some(backup) = &self.backup_file {
            if !backup.exists() {
                if let Some(dir) = backup.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::copy(&path, backup);
            }
        }
        Ok(())
    }
}

/// Firebase's error code (`TOKEN_EXPIRED`, `INVALID_REFRESH_TOKEN`, ...), never the token.
fn firebase_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(|m| format!("({m})")))
        .unwrap_or_default()
}

fn expires_in(v: &Value) -> f64 {
    let secs = v
        .get("expires_in")
        .or_else(|| v.get("expiresIn"))
        .and_then(|x| x.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| x.as_f64()))
        .unwrap_or(3600.0);
    secs * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(dir: &Path, server: &mockito::Server, backup: Option<PathBuf>) -> Authenticator {
        Authenticator::new(dir, backup, CloudConfig::for_tests(&server.url())).unwrap()
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    #[test]
    fn refreshes_without_touching_the_identity_file() {
        let mut server = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        let identity = r#"{"idToken":"old","refreshToken":"r/1","expirationUtc":"2020-01-01T00:00:00+00:00","userId":"u1"}"#;
        std::fs::write(t.path().join(IDENTITY_FILE), identity).unwrap();
        let m = server
            .mock("POST", "/token")
            .match_query(mockito::Matcher::UrlEncoded("key".into(), "test-key".into()))
            .match_body("grant_type=refresh_token&refresh_token=r%2F1")
            .with_body(r#"{"id_token":"new","refresh_token":"r2","expires_in":"3600","user_id":"u1"}"#)
            .expect(1)
            .create();
        let a = auth(t.path(), &server, None);
        let rt = rt();
        let s = rt.block_on(a.session(false)).unwrap();
        assert_eq!((s.id_token.as_str(), s.user_id.as_str()), ("new", "u1"));
        // Cached: no second request.
        assert_eq!(rt.block_on(a.session(false)).unwrap().id_token, "new");
        // A fresh instance uses our session file, not the network.
        let b = auth(t.path(), &server, None);
        assert_eq!(rt.block_on(b.session(false)).unwrap().id_token, "new");
        m.assert();
        assert_eq!(std::fs::read_to_string(t.path().join(IDENTITY_FILE)).unwrap(), identity);
    }

    #[test]
    fn failed_refresh_never_creates_a_new_account() {
        let mut server = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        let identity = r#"{"idToken":"old","refreshToken":"r","expirationUtc":"2020-01-01T00:00:00Z","userId":"u1"}"#;
        std::fs::write(t.path().join(IDENTITY_FILE), identity).unwrap();
        server.mock("POST", "/token").match_query(mockito::Matcher::Any).with_status(400).create();
        let signup = server.mock("POST", "/signUp").match_query(mockito::Matcher::Any).expect(0).create();
        let a = auth(t.path(), &server, None);
        assert!(matches!(rt().block_on(a.session(false)), Err(CloudError::SignIn(_))));
        signup.assert();
        assert_eq!(std::fs::read_to_string(t.path().join(IDENTITY_FILE)).unwrap(), identity);
    }

    #[test]
    fn restores_from_backup_and_signs_up_only_when_nothing_exists() {
        let mut server = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        let backup = t.path().join("SVSM Backup").join(IDENTITY_FILE);
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, r#"{"idToken":"b","refreshToken":"r","expirationUtc":"2999-01-01T00:00:00Z","userId":"u9"}"#).unwrap();
        let mgr = t.path().join("mgr");
        let a = auth(&mgr, &server, Some(backup.clone()));
        assert_eq!(rt().block_on(a.session(false)).unwrap().user_id, "u9");
        assert!(mgr.join(IDENTITY_FILE).exists(), "restored");

        let fresh_dir = t.path().join("fresh");
        let new_backup = t.path().join("nb").join(IDENTITY_FILE);
        server
            .mock("POST", "/signUp")
            .match_query(mockito::Matcher::Any)
            .with_body(r#"{"idToken":"i","refreshToken":"r","expiresIn":"3600","localId":"newuser"}"#)
            .create();
        let b = auth(&fresh_dir, &server, Some(new_backup.clone()));
        assert_eq!(rt().block_on(b.session(false)).unwrap().user_id, "newuser");
        let written = read_identity(&fresh_dir.join(IDENTITY_FILE)).unwrap();
        assert_eq!(written.user_id, "newuser");
        assert!(parse_iso_ms(&written.expiration_utc).is_some());
        assert_eq!(read_identity(&new_backup).unwrap().user_id, "newuser");
    }

    #[test]
    fn reads_legacy_identity_shapes() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.json");
        std::fs::write(&p, r#"{"idToken":"i","refreshToken":"r","localId":"x","expiresAt":"2020-01-01T00:00:00Z"}"#).unwrap();
        assert_eq!(read_identity(&p).unwrap().user_id, "x");
        std::fs::write(&p, r#"{"idToken":"i"}"#).unwrap();
        assert!(read_identity(&p).is_none());
    }
}
