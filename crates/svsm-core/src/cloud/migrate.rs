//! Moving a cloud identity from the first Firebase project to the current one (port of 2.3.0's
//! `FirebaseModlistMigrationService`, made reversible). Runs only when the user asks.
//!
//! 1. Read the player's modlists from the old project's slots (nothing is written there).
//! 2. Keep the old identity: `firebase-auth.json` -> `firebase-auth.legacy[.N].json`, and the
//!    `SVSM Backup` copy -> `firebase-auth.legacy[.N].json` next to it.
//! 3. Create an account on the new project (new `firebase-auth.json` + backup copy).
//! 4. Publish the modlists to the same slots on the new project.
//!
//! If 3 or 4 fails, the new identity files are removed and the old ones put back, so the user
//! ends up exactly where they started (an unused anonymous account may remain in the cloud).

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use super::auth::{read_identity, Authenticator, IDENTITY_FILE, SESSION_FILE};
use super::store::{CloudStore, PlayerIdentity};
use super::{CloudConfig, CloudError, CloudResult};

pub const LEGACY_PROJECT: &str = "simple-vs-manager";
const LEGACY_SESSION_FILE: &str = "svsm3-firebase-session-legacy.json";

/// The Firebase project an id token was issued for (its `aud`/`iss`), without verifying it.
pub fn token_project(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("aud")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| v.get("iss").and_then(Value::as_str).and_then(|i| i.rsplit('/').next()).map(String::from))
}

/// The saved identity belongs to the old project.
pub fn is_legacy_identity(manager_dir: &Path) -> bool {
    read_identity(&manager_dir.join(IDENTITY_FILE)).and_then(|a| token_project(&a.id_token)).as_deref() == Some(LEGACY_PROJECT)
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    /// Names of the modlists moved to the new project.
    pub moved: Vec<String>,
    /// Where the old identity was kept.
    pub legacy_file: String,
}

fn free_name(dir: &Path) -> PathBuf {
    let first = dir.join("firebase-auth.legacy.json");
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("firebase-auth.legacy.{n}.json"))).find(|p| !p.exists()).expect("some name is free")
}

fn io(path: &Path, e: std::io::Error) -> CloudError {
    CloudError::Io { path: path.display().to_string(), message: e.to_string() }
}

/// Moves the identity and its modlists to the new project. `new_config` is the current
/// project (tests pass a fake one), `legacy_config` the old one.
pub async fn migrate(
    manager_dir: &Path,
    backup_file: Option<PathBuf>,
    player: &PlayerIdentity,
    legacy_config: CloudConfig,
    new_config: CloudConfig,
    cache_dir: &Path,
) -> CloudResult<MigrationReport> {
    let identity_path = manager_dir.join(IDENTITY_FILE);
    if read_identity(&identity_path).is_none() {
        return Err(CloudError::Invalid("There's no cloud identity to move.".into()));
    }

    // 1. Read the old slots.
    let legacy_auth = Authenticator::new(manager_dir, None, legacy_config.clone())?.with_session_file(LEGACY_SESSION_FILE);
    let legacy = CloudStore::new(&legacy_auth, legacy_config, cache_dir)?;
    let lists = legacy.slot_contents(player).await?;

    // 2. Keep the old identity (and its backup copy) under new names.
    let legacy_file = free_name(manager_dir);
    std::fs::rename(&identity_path, &legacy_file).map_err(|e| io(&identity_path, e))?;
    let backup_moved: Option<(PathBuf, PathBuf)> = backup_file.as_ref().filter(|b| b.exists()).and_then(|b| {
        let dest = free_name(b.parent()?);
        std::fs::rename(b, &dest).ok().map(|_| (b.clone(), dest))
    });
    let restore = || {
        let _ = std::fs::remove_file(&identity_path);
        let _ = std::fs::rename(&legacy_file, &identity_path);
        if let Some((original, kept)) = &backup_moved {
            let _ = std::fs::remove_file(original);
            let _ = std::fs::rename(kept, original);
        }
        let _ = std::fs::remove_file(manager_dir.join(SESSION_FILE));
    };

    // 3 + 4. New account, then re-publish.
    let result = async {
        let _ = std::fs::remove_file(manager_dir.join(SESSION_FILE));
        let new_auth = Authenticator::new(manager_dir, backup_file.clone(), new_config.clone())?;
        new_auth.session(false).await?;
        let store = CloudStore::new(&new_auth, new_config, cache_dir)?;
        let mut moved = Vec::new();
        for (slot, content) in &lists {
            store.publish(player, slot, content, None).await?;
            moved.push(content.get("name").and_then(Value::as_str).unwrap_or(slot).to_string());
        }
        Ok::<_, CloudError>(moved)
    }
    .await;

    match result {
        Ok(moved) => {
            let _ = std::fs::remove_file(manager_dir.join(LEGACY_SESSION_FILE));
            Ok(MigrationReport { moved, legacy_file: legacy_file.display().to_string() })
        }
        Err(e) => {
            restore();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    fn token(project: &str) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(r#"{{"aud":"{project}","iss":"https://securetoken.google.com/{project}"}}"#));
        format!("h.{payload}.sig")
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    fn player() -> PlayerIdentity {
        PlayerIdentity::new(Some("P1".into()), Some("Martin".into())).unwrap()
    }

    fn setup(dir: &Path) -> String {
        let identity = format!(r#"{{"idToken":"{}","refreshToken":"old-r","expirationUtc":"2999-01-01T00:00:00Z","userId":"old"}}"#, token(LEGACY_PROJECT));
        std::fs::write(dir.join(IDENTITY_FILE), &identity).unwrap();
        identity
    }

    #[test]
    fn detects_legacy_tokens() {
        assert_eq!(token_project(&token("simple-vs-manager")).as_deref(), Some(LEGACY_PROJECT));
        assert_eq!(token_project("garbage"), None);
        let t = tempfile::tempdir().unwrap();
        setup(t.path());
        assert!(is_legacy_identity(t.path()));
    }

    #[test]
    fn moves_lists_and_keeps_the_old_identity() {
        let mut old = mockito::Server::new();
        let mut new = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        let mgr = t.path().join("mgr");
        std::fs::create_dir_all(&mgr).unwrap();
        let old_identity = setup(&mgr);
        let backup = t.path().join("SVSM Backup").join(IDENTITY_FILE);
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, &old_identity).unwrap();

        old.mock("GET", "/db/owners/P1.json").match_query(Matcher::Any).with_body("\"old\"").create();
        old.mock("GET", "/db/users/P1.json")
            .match_query(Matcher::Any)
            .with_body(r#"{"slot1":{"content":{"name":"A","mods":[]}},"slot3":{"content":{"name":"B","mods":[]}}}"#)
            .create();
        let old_writes = old.mock("PATCH", Matcher::Any).expect(0).create();
        new.mock("POST", "/signUp")
            .match_query(Matcher::Any)
            .with_body(format!(r#"{{"idToken":"{}","refreshToken":"nr","expiresIn":"3600","localId":"new"}}"#, token("simplevsmanager")))
            .expect(1)
            .create();
        new.mock("GET", "/db/owners/P1.json").match_query(Matcher::Any).with_body("null").create();
        new.mock("PUT", "/db/owners/P1.json").match_query(Matcher::Any).with_body("\"new\"").create();
        new.mock("GET", "/db/users/P1.json").match_query(Matcher::Any).with_body("null").create();
        let publishes = new.mock("PATCH", "/db.json").match_query(Matcher::Any).with_body("{}").expect(2).create();

        let report = rt()
            .block_on(migrate(&mgr, Some(backup.clone()), &player(), CloudConfig::for_tests(&old.url()), CloudConfig::for_tests(&new.url()), t.path()))
            .unwrap();
        assert_eq!(report.moved, ["A", "B"]);
        publishes.assert();
        old_writes.assert();
        assert_eq!(std::fs::read_to_string(mgr.join("firebase-auth.legacy.json")).unwrap(), old_identity);
        assert_eq!(read_identity(&mgr.join(IDENTITY_FILE)).unwrap().user_id, "new");
        assert_eq!(std::fs::read_to_string(backup.parent().unwrap().join("firebase-auth.legacy.json")).unwrap(), old_identity);
        assert_eq!(read_identity(&backup).unwrap().user_id, "new");
        assert!(!is_legacy_identity(&mgr));
    }

    #[test]
    fn failure_puts_everything_back() {
        let mut old = mockito::Server::new();
        let mut new = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        let mgr = t.path().join("mgr");
        std::fs::create_dir_all(&mgr).unwrap();
        let old_identity = setup(&mgr);
        let backup = t.path().join("SVSM Backup").join(IDENTITY_FILE);
        std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
        std::fs::write(&backup, &old_identity).unwrap();

        old.mock("GET", "/db/owners/P1.json").match_query(Matcher::Any).with_body("\"old\"").create();
        old.mock("GET", "/db/users/P1.json").match_query(Matcher::Any).with_body(r#"{"slot1":{"content":{"name":"A"}}}"#).create();
        new.mock("POST", "/signUp")
            .match_query(Matcher::Any)
            .with_body(format!(r#"{{"idToken":"{}","refreshToken":"nr","expiresIn":"3600","localId":"new"}}"#, token("simplevsmanager")))
            .create();
        // The player's slots on the new project belong to someone else: publishing fails.
        new.mock("GET", "/db/owners/P1.json").match_query(Matcher::Any).with_body("\"someone\"").create();

        let err = rt()
            .block_on(migrate(&mgr, Some(backup.clone()), &player(), CloudConfig::for_tests(&old.url()), CloudConfig::for_tests(&new.url()), t.path()))
            .unwrap_err();
        assert!(err.to_string().contains("another installation"), "{err}");
        assert_eq!(std::fs::read_to_string(mgr.join(IDENTITY_FILE)).unwrap(), old_identity);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), old_identity);
        assert!(!mgr.join("firebase-auth.legacy.json").exists());
        assert!(!backup.parent().unwrap().join("firebase-auth.legacy.json").exists());
    }
}
