//! In-app updates from GitHub Releases.
//!
//! The release workflow uploads `latest.json` (Tauri's updater manifest) next to the installers
//! when the update signing key is set. Each platform entry holds a download URL and a minisign
//! signature made with that key; the public half is `updater.pub` in this crate.
//!
//! - Windows: the app is one exe (portable, or installed by the NSIS installer). An update
//!   downloads the new portable exe, checks its signature, swaps the running exe for it and
//!   restarts. Settings and data live in the manager folder, so nothing else changes.
//! - macOS and the Linux AppImage: Tauri's updater plugin replaces the app bundle / AppImage.
//! - .deb / .rpm installs (and releases without `latest.json`): a notice with a link only.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const REPO: &str = "Interzoneism/SimpleVSManager";
/// Tauri updater public key (base64 of a minisign public key file).
pub const PUBKEY: &str = include_str!("../updater.pub");
/// `latest.json` key for the Windows portable exe. Not one of Tauri's own keys, because the
/// plugin would run an .exe as an installer.
const WINDOWS_KEY: &str = "windows-x86_64-portable";
const MANIFEST_ASSET: &str = "latest.json";
/// GitHub allows 60 unauthenticated API calls an hour; one check an hour is plenty.
const CHECK_TTL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagerUpdate {
    pub version: String,
    /// The release page, for notes and manual downloads.
    pub page_url: Option<String>,
    /// Release notes (Markdown), if any.
    pub notes: Option<String>,
    /// True when "Update and restart" can do it in place; false = download it yourself.
    pub can_install: bool,
}

#[derive(Debug, Clone)]
struct Found {
    update: ManagerUpdate,
    #[cfg_attr(windows, allow(dead_code))]
    manifest_url: Option<String>,
    #[cfg_attr(not(windows), allow(dead_code))]
    manifest: Option<Manifest>,
}

static LAST_CHECK: Mutex<Option<(Instant, Option<Found>)>> = Mutex::new(None);

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    html_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    platforms: std::collections::BTreeMap<String, PlatformEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct PlatformEntry {
    signature: String,
    url: String,
}

fn http() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("SimpleVSManager/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::msg(e.to_string()))
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches(['v', 'V'])).ok()
}

/// The newest release that is newer than `current`. Pre-releases only count when the running
/// version is itself a pre-release (beta testers get betas, everyone else stable releases).
fn pick_release(releases: Vec<GhRelease>, current: &semver::Version) -> Option<(semver::Version, GhRelease)> {
    let allow_pre = !current.pre.is_empty();
    releases
        .into_iter()
        .filter(|r| !r.draft && (allow_pre || !r.prerelease))
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| (allow_pre || v.pre.is_empty()) && v > current)
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// Whether this install can update itself from `manifest`.
fn can_install(manifest: &Manifest) -> bool {
    let has = |k: &str| manifest.platforms.contains_key(k);
    if cfg!(windows) {
        has(WINDOWS_KEY)
    } else if cfg!(target_os = "macos") {
        has(if cfg!(target_arch = "aarch64") { "darwin-aarch64" } else { "darwin-x86_64" })
    } else {
        // Only an AppImage can replace itself; .deb/.rpm are updated by downloading the package.
        std::env::var_os("APPIMAGE").is_some() && has("linux-x86_64")
    }
}

async fn find_update(current: &str) -> AppResult<Option<Found>> {
    let current = parse_version(current).ok_or_else(|| AppError::msg(format!("Bad app version {current}")))?;
    let client = http()?;
    let releases: Vec<GhRelease> = client
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=20"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| AppError::msg(format!("Could not check for updates: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::msg(format!("Could not read the release list: {e}")))?;
    let Some((version, release)) = pick_release(releases, &current) else { return Ok(None) };

    let manifest_url = release.assets.iter().find(|a| a.name == MANIFEST_ASSET).map(|a| a.browser_download_url.clone());
    let manifest = match &manifest_url {
        Some(url) => match client.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => resp.json::<Manifest>().await.ok(),
            Err(e) => {
                tracing::warn!("Could not read {url}: {e}");
                None
            }
        },
        None => None,
    };
    let can_install = manifest.as_ref().is_some_and(|m| parse_version(&m.version).as_ref() == Some(&version) && can_install(m));
    Ok(Some(Found {
        update: ManagerUpdate {
            version: version.to_string(),
            page_url: release.html_url,
            notes: release.body.filter(|b| !b.trim().is_empty()),
            can_install,
        },
        manifest_url,
        manifest,
    }))
}

async fn cached_find(app: &AppHandle, force: bool) -> AppResult<Option<Found>> {
    if !force {
        if let Some((at, found)) = LAST_CHECK.lock().unwrap().as_ref() {
            if at.elapsed() < CHECK_TTL {
                return Ok(found.clone());
            }
        }
    }
    let found = find_update(&app.package_info().version.to_string()).await?;
    *LAST_CHECK.lock().unwrap() = Some((Instant::now(), found.clone()));
    Ok(found)
}

/// A newer Simple VS Manager on GitHub, if there is one. Quietly `None` in offline mode or
/// when GitHub can't be reached (an update check must never get in the way).
#[tauri::command]
#[specta::specta]
pub async fn manager_update(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<ManagerUpdate>> {
    if state.environment().settings.offline_mode {
        return Ok(None);
    }
    match cached_find(&app, false).await {
        Ok(found) => Ok(found.map(|f| f.update)),
        Err(e) => {
            tracing::info!("{e}");
            Ok(None)
        }
    }
}

/// Downloads and installs the update found by [`manager_update`], then restarts the app.
#[tauri::command]
#[specta::specta]
pub async fn install_manager_update(app: AppHandle) -> AppResult<()> {
    let found = cached_find(&app, false).await?.ok_or_else(|| AppError::msg("No update is available."))?;
    if !found.update.can_install {
        return Err(AppError::msg("This update has to be downloaded from the release page."));
    }
    tracing::info!("Installing Simple VS Manager {}", found.update.version);
    install(&app, &found).await?;
    app.restart();
}

#[cfg(windows)]
async fn install(_app: &AppHandle, found: &Found) -> AppResult<()> {
    let entry = found
        .manifest
        .as_ref()
        .and_then(|m| m.platforms.get(WINDOWS_KEY))
        .ok_or_else(|| AppError::msg("The update has no Windows download."))?;
    let bytes = http()?
        .get(&entry.url)
        .timeout(Duration::from_secs(600))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| AppError::msg(format!("Download failed: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::msg(format!("Download failed: {e}")))?;
    verify_signature(&bytes, &entry.signature, PUBKEY)?;

    let exe = std::env::current_exe().map_err(|e| AppError::msg(e.to_string()))?;
    let new = exe.with_extension("update.exe");
    std::fs::write(&new, &bytes).map_err(|e| AppError::msg(format!("Could not write {}: {e}", new.display())))?;
    let replaced = self_replace::self_replace(&new);
    let _ = std::fs::remove_file(&new);
    replaced.map_err(|e| AppError::msg(format!("Could not replace {}: {e}", exe.display())))
}

#[cfg(not(windows))]
async fn install(app: &AppHandle, found: &Found) -> AppResult<()> {
    use tauri_plugin_updater::UpdaterExt as _;
    let url = found.manifest_url.as_deref().ok_or_else(|| AppError::msg("The update has no manifest."))?;
    let url = url.parse().map_err(|e| AppError::msg(format!("Bad update URL {url}: {e}")))?;
    let err = |e: tauri_plugin_updater::Error| AppError::msg(format!("Update failed: {e}"));
    let update = app
        .updater_builder()
        .pubkey(PUBKEY.trim())
        .endpoints(vec![url])
        .map_err(err)?
        .build()
        .map_err(err)?
        .check()
        .await
        .map_err(err)?
        .ok_or_else(|| AppError::msg("The update is no longer available."))?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(err)
}

/// Checks a Tauri updater signature: both strings are base64 of minisign text files.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn verify_signature(data: &[u8], signature_b64: &str, pubkey_b64: &str) -> AppResult<()> {
    let b64 = |s: &str| -> AppResult<String> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .map_err(|e| AppError::msg(format!("Bad base64: {e}")))?;
        String::from_utf8(raw).map_err(|e| AppError::msg(e.to_string()))
    };
    let bad = |e: minisign_verify::Error| AppError::msg(format!("The update's signature is not valid: {e}"));
    let pk = minisign_verify::PublicKey::decode(&b64(pubkey_b64)?).map_err(bad)?;
    let sig = minisign_verify::Signature::decode(&b64(signature_b64)?).map_err(bad)?;
    pk.verify(data, &sig, true).map_err(bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, pre: bool, draft: bool) -> GhRelease {
        GhRelease { tag_name: tag.into(), draft, prerelease: pre, html_url: None, body: None, assets: vec![] }
    }

    fn pick(current: &str, rels: Vec<GhRelease>) -> Option<String> {
        pick_release(rels, &parse_version(current).unwrap()).map(|(v, _)| v.to_string())
    }

    #[test]
    fn stable_users_only_get_stable_releases() {
        let rels = || vec![rel("v3.1.0-beta.1", true, false), rel("v3.0.1", false, false), rel("v3.2.0", false, true)];
        assert_eq!(pick("3.0.0", rels()), Some("3.0.1".into()));
        assert_eq!(pick("3.0.1", rels()), None);
    }

    #[test]
    fn beta_users_get_betas_and_stable() {
        let rels = vec![rel("v3.0.0-beta.2", true, false), rel("v3.0.0-beta.1", true, false), rel("garbage", false, false)];
        assert_eq!(pick("3.0.0-beta.1", rels), Some("3.0.0-beta.2".into()));
        let rels = vec![rel("v3.0.0-beta.2", true, false), rel("v3.0.0", false, false)];
        assert_eq!(pick("3.0.0-beta.1", rels), Some("3.0.0".into()));
    }

    #[test]
    fn signature_check() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/updater/payload.bin")).unwrap();
        let pubkey = include_str!("../tests/fixtures/updater/test.pub");
        let sig = include_str!("../tests/fixtures/updater/payload.bin.sig");
        verify_signature(&data, sig, pubkey).unwrap();
        assert!(verify_signature(b"tampered", sig, pubkey).is_err());
        // The real key must at least parse.
        let pk = base64::engine::general_purpose::STANDARD.decode(PUBKEY.trim()).unwrap();
        minisign_verify::PublicKey::decode(std::str::from_utf8(&pk).unwrap()).unwrap();
    }
}
