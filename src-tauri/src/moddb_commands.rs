//! Mod DB, updates, the version cache and the mod browser.

use svsm_core::sync::{LockExt as _, RwLockExt as _};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;

use svsm_core::clientsettings::ClientSettings;
use svsm_core::moddb::{
    self, parse, updates, BrowsePage, BrowseQuery, CachedMod, CachedVersion, InstallPlan, InstallTarget, ModDb,
    ModDbInfo, ModDbSummary,
};
use svsm_core::mods::{is_base_game_mod, InstalledMod, ModSourceKind};
use svsm_core::settings::{set_flag, Settings};
use svsm_core::DetectedEnvironment;

use crate::commands::{current_list, ModList};
use crate::error::{AppError, AppResult};
use crate::events::UpdateProgress;
use crate::state::AppState;

fn target_and_exact(env: &DetectedEnvironment) -> (Option<String>, bool) {
    (env.settings.effective_target_version(env.game_version.as_deref()), env.settings.require_exact_version_match)
}

pub(crate) fn info_from(db_mod: &CachedMod, mod_id: &str, env: &DetectedEnvironment) -> Option<ModDbInfo> {
    let (target, exact) = target_and_exact(env);
    db_mod.raw.as_ref().map(|raw| parse::build_info(mod_id, raw, target.as_deref(), exact, db_mod.fetched_ms))
}

fn lookup_ids(mods: &[InstalledMod]) -> Vec<String> {
    let mut seen = HashSet::new();
    mods.iter()
        .map(|m| m.mod_id.trim())
        .filter(|id| !id.is_empty() && !is_base_game_mod(id))
        .filter(|id| seen.insert(id.to_ascii_lowercase()))
        .map(String::from)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Status of installed mods
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LookupMode {
    /// Cached data only (instant, used at startup before the network answers).
    CacheOnly,
    /// Ask the Mod DB for anything older than 30 minutes.
    Refresh,
    /// Ask the Mod DB for everything.
    Force,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDbEntry {
    /// Installed mod key.
    pub key: String,
    pub summary: ModDbSummary,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDbStatus {
    pub entries: Vec<ModDbEntry>,
    /// Installed mod keys the Mod DB doesn't know.
    pub not_on_mod_db: Vec<String>,
    /// Installed mod keys with no data yet (offline and never looked up).
    pub unchecked: Vec<String>,
    pub offline: bool,
    /// The first network error, if some lookups failed (cached data is used for those).
    pub error: Option<String>,
    /// Game version releases are matched against; `null` = any (target "latest").
    pub target_game_version: Option<String>,
    pub updates_available: u32,
}

/// Mod DB data for every installed mod: latest version, update offers, page links.
#[tauri::command]
#[specta::specta]
pub async fn moddb_status(state: State<'_, AppState>, mode: LookupMode) -> AppResult<ModDbStatus> {
    let db = state.moddb()?;
    let env = state.environment();
    let mods = state.mods.read_safe().mods.clone();
    let ids = lookup_ids(&mods);
    let (found, error) = match mode {
        LookupMode::CacheOnly => (db.cached(&ids), None),
        LookupMode::Refresh | LookupMode::Force => {
            let l = db.lookup(&ids, matches!(mode, LookupMode::Force)).await;
            (l.mods, l.error)
        }
    };
    Ok(build_status(&env, &mods, &found, db.is_offline(), error))
}

fn build_status(
    env: &DetectedEnvironment,
    mods: &[InstalledMod],
    found: &std::collections::HashMap<String, CachedMod>,
    offline: bool,
    error: Option<String>,
) -> ModDbStatus {
    let (target, exact) = target_and_exact(env);
    let mut status = ModDbStatus {
        entries: Vec::new(),
        not_on_mod_db: Vec::new(),
        unchecked: Vec::new(),
        offline,
        error,
        target_game_version: target.clone(),
        updates_available: 0,
    };
    for m in mods {
        let id = m.mod_id.trim();
        if id.is_empty() || is_base_game_mod(id) {
            continue;
        }
        match found.get(&id.to_ascii_lowercase()) {
            None => status.unchecked.push(m.key.clone()),
            Some(c) => match info_from(c, id, env) {
                None => status.not_on_mod_db.push(m.key.clone()),
                Some(info) => {
                    let summary = updates::summarize(
                        &info,
                        m.version.as_deref(),
                        target.as_deref(),
                        exact,
                        env.settings.skipped_version(id),
                        env.settings.is_update_excluded(id),
                    );
                    if summary.update.is_some() {
                        status.updates_available += 1;
                    }
                    status.entries.push(ModDbEntry { key: m.key.clone(), summary });
                }
            },
        }
    }
    status
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModDbDetails {
    pub info: Option<ModDbInfo>,
    /// The mod's Mod DB description as HTML. Sanitize before rendering.
    pub description_html: Option<String>,
    /// Versions kept in the local cache (installable offline / for rollback).
    pub cached_versions: Vec<CachedVersion>,
    pub error: Option<String>,
}

/// Full Mod DB info for one mod (all releases with changelogs), refreshed if stale.
#[tauri::command]
#[specta::specta]
pub async fn moddb_details(state: State<'_, AppState>, mod_id: String, force: bool) -> AppResult<ModDbDetails> {
    let db = state.moddb()?;
    let env = state.environment();
    let (cached, error) = db.get_mod(&mod_id, force).await;
    let info = cached.as_ref().and_then(|c| info_from(c, &mod_id, &env));
    let description_html = cached
        .as_ref()
        .and_then(|c| c.raw.as_ref())
        .and_then(|r| r.get("text"))
        .and_then(|t| t.as_str())
        .map(String::from);
    Ok(ModDbDetails { info, description_html, cached_versions: db.cached_mods().versions(&mod_id), error })
}

// ---------------------------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------------------------

/// Changes to the update / Mod DB preferences. Fields left `null` are unchanged.
#[derive(Debug, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PreferencesPatch {
    pub offline_mode: Option<bool>,
    /// `""` = installed game version, `"latest"` = any, or a version like `"1.21.0"`.
    pub target_game_version: Option<String>,
    pub require_exact_version_match: Option<bool>,
    pub mod_cache_limit_mb: Option<u32>,
    /// Skip (`version` set) or un-skip (`version` null) a mod's offered version.
    pub skip: Option<SkipChange>,
    pub update_excluded: Option<FlagChange>,
    pub favourite: Option<FlagChange>,
    /// Ask to confirm working mods after long play sessions.
    pub mod_usage_tracking: Option<bool>,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkipChange {
    pub mod_id: String,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FlagChange {
    pub mod_ids: Vec<String>,
    pub on: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn update_preferences(state: State<'_, AppState>, patch: PreferencesPatch) -> AppResult<DetectedEnvironment> {
    let env = state.update_settings(|s: &mut Settings| {
        if let Some(v) = patch.offline_mode {
            s.offline_mode = v;
        }
        if let Some(t) = patch.target_game_version {
            let t = t.trim();
            s.target_game_version = (!t.is_empty()).then(|| t.to_string());
        }
        if let Some(v) = patch.require_exact_version_match {
            s.require_exact_version_match = v;
        }
        if let Some(v) = patch.mod_cache_limit_mb {
            s.mod_cache_limit_mb = v;
        }
        if let Some(v) = patch.mod_usage_tracking {
            s.mod_usage_tracking = v;
        }
        if let Some(skip) = patch.skip {
            let id = skip.mod_id.trim().to_ascii_lowercase();
            match skip.version.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
                Some(v) => {
                    s.skipped_mod_versions.insert(id, v);
                }
                None => {
                    s.skipped_mod_versions.remove(&id);
                }
            }
        }
        if let Some(f) = patch.update_excluded {
            for id in &f.mod_ids {
                set_flag(&mut s.update_exclusions, id, f.on);
            }
        }
        if let Some(f) = patch.favourite {
            for id in &f.mod_ids {
                set_flag(&mut s.favourite_mods, id, f.on);
            }
        }
    })?;
    // Keeps the client's offline flag in sync.
    let db = state.moddb()?;
    if patch.mod_cache_limit_mb.is_some() {
        prune_cache(&state, &db, &env);
    }
    Ok(env)
}

// ---------------------------------------------------------------------------------------------
// Version cache
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCacheStats {
    pub bytes: f64,
    pub limit_mb: u32,
    pub path: String,
}

fn installed_versions(state: &AppState) -> Vec<(String, String)> {
    state
        .mods
        .read_safe()
        .mods
        .iter()
        .filter_map(|m| Some((m.mod_id.clone(), m.version.clone()?)))
        .collect()
}

pub(crate) fn prune_cache(state: &AppState, db: &ModDb, env: &DetectedEnvironment) {
    let limit = env.settings.mod_cache_limit_mb as u64 * 1024 * 1024;
    // The cache is also the mod library: never drop a version a saved modlist uses.
    let mut keep = installed_versions(state);
    keep.extend(crate::modlist_commands::referenced_versions(state));
    let freed = db.cached_mods().prune(limit, &keep);
    if freed > 0 {
        tracing::info!("Mod cache over its limit; freed {freed} bytes");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn mod_cache_stats(state: State<'_, AppState>) -> AppResult<ModCacheStats> {
    let db = state.moddb()?;
    let env = state.environment();
    Ok(ModCacheStats {
        bytes: db.cached_mods().total_bytes() as f64,
        limit_mb: env.settings.mod_cache_limit_mb,
        path: db.cached_mods().dir().display().to_string(),
    })
}

/// Deletes every cached mod version and the cached Mod DB data.
#[tauri::command]
#[specta::specta]
pub async fn clear_mod_cache(state: State<'_, AppState>) -> AppResult<ModCacheStats> {
    let db = state.moddb()?;
    let _guard = state.update_lock.lock().await;
    db.cached_mods().clear()?;
    db.cache().clear()?;
    drop(_guard);
    mod_cache_stats(state).await
}

// ---------------------------------------------------------------------------------------------
// Updating and installing
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    /// Installed mod key.
    pub key: String,
    /// Version to install; `null` = the offered update. Older versions roll back.
    pub version: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    pub key: String,
    pub mod_id: String,
    pub name: String,
    pub ok: bool,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    pub outcomes: Vec<UpdateOutcome>,
    pub cancelled: bool,
    pub list: ModList,
}

/// Stage of one mod's update, for [`UpdateProgress`].
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStage {
    Queued,
    Downloading,
    Installing,
    Done,
    Failed,
}

pub(crate) fn emit<R: tauri::Runtime>(app: &AppHandle<R>, key: &str, stage: UpdateStage, percent: Option<f64>, message: Option<String>) {
    let _ = UpdateProgress { key: key.to_string(), stage, percent, message }.emit(app);
}

/// Updates (or rolls back) installed mods, one after another. Emits `UpdateProgress`.
#[tauri::command]
#[specta::specta]
pub async fn update_mods(app: AppHandle, state: State<'_, AppState>, requests: Vec<UpdateRequest>) -> AppResult<UpdateReport> {
    let _run = state.update_lock.lock().await;
    state.ensure_game_closed()?;
    state.cancel_updates.store(false, Ordering::Relaxed);
    let db = state.moddb()?;
    let env = state.environment();
    if !requests.is_empty() {
        crate::modlist_commands::snapshot_installed(&state, "ModsUpdated", Vec::new());
    }
    for r in &requests {
        emit(&app, &r.key, UpdateStage::Queued, None, None);
    }
    let mut outcomes = Vec::new();
    let mut cancelled = false;
    for r in requests {
        if state.cancel_updates.load(Ordering::Relaxed) {
            cancelled = true;
            emit(&app, &r.key, UpdateStage::Failed, None, Some("Cancelled.".into()));
            continue;
        }
        // The game may have been started during a long run.
        let m = match state.ensure_game_closed().and_then(|()| state.find_mod(&r.key)) {
            Ok(m) => m,
            Err(e) => {
                emit(&app, &r.key, UpdateStage::Failed, None, Some(e.message.clone()));
                outcomes.push(UpdateOutcome {
                    key: r.key,
                    mod_id: String::new(),
                    name: String::new(),
                    ok: false,
                    from_version: None,
                    to_version: None,
                    message: Some(e.message),
                });
                continue;
            }
        };
        let result = update_one(&app, &state, &db, &env, &m, r.version.as_deref()).await;
        let (ok, to_version, message) = match result {
            Ok(v) => (true, Some(v), None),
            Err(e) => (false, None, Some(e.message)),
        };
        emit(&app, &m.key, if ok { UpdateStage::Done } else { UpdateStage::Failed }, None, message.clone());
        if !ok && message.as_deref() == Some("Cancelled.") {
            cancelled = true;
        }
        outcomes.push(UpdateOutcome {
            key: m.key.clone(),
            mod_id: m.mod_id.clone(),
            name: m.name.clone(),
            ok,
            from_version: m.version.clone(),
            to_version,
            message,
        });
    }
    state.rescan()?;
    prune_cache(&state, &db, &env);
    Ok(UpdateReport { outcomes, cancelled, list: current_list(&state)? })
}

/// Stops queued updates after the current download chunk.
#[tauri::command]
#[specta::specta]
pub fn cancel_updates(state: State<'_, AppState>) {
    state.cancel_updates.store(true, Ordering::Relaxed);
}

async fn release_for(
    db: &ModDb,
    env: &DetectedEnvironment,
    mod_id: &str,
    installed_version: Option<&str>,
    version: Option<&str>,
) -> AppResult<moddb::ModDbRelease> {
    let (cached, error) = db.get_mod(mod_id, false).await;
    let info = cached.as_ref().and_then(|c| info_from(c, mod_id, env)).ok_or_else(|| {
        AppError::msg(error.unwrap_or_else(|| format!("{mod_id} isn't on the Mod DB, or it hasn't been looked up yet.")))
    })?;
    let release = match version {
        Some(v) => info.find_release(v).cloned(),
        None => {
            let (target, exact) = target_and_exact(env);
            let s = updates::summarize(&info, installed_version, target.as_deref(), exact, None, false);
            match s.update {
                Some(offer) => info.find_release(&offer.version).cloned(),
                None if installed_version.is_none() => {
                    updates::update_candidate(&info).or(info.latest_release()).cloned()
                }
                None => None,
            }
        }
    };
    release.ok_or_else(|| AppError::msg("No matching release was found on the Mod DB."))
}

async fn fetch<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    db: &ModDb,
    env: &DetectedEnvironment,
    key: &str,
    mod_id: &str,
    release: &moddb::ModDbRelease,
) -> AppResult<(PathBuf, bool)> {
    emit(app, key, UpdateStage::Downloading, Some(0.0), None);
    let mut last = 0.0f64;
    let keep = env.settings.mod_cache_limit_mb > 0;
    db.fetch_release(
        mod_id,
        release,
        keep,
        |p| {
            if let Some(total) = p.total.filter(|t| *t > 0) {
                let pct = (p.received as f64 / total as f64 * 100.0).min(100.0);
                if pct - last >= 2.0 || pct >= 100.0 {
                    last = pct;
                    emit(app, key, UpdateStage::Downloading, Some(pct), None);
                }
            }
        },
        || state.cancel_updates.load(Ordering::Relaxed),
    )
    .await
    .map_err(|e| AppError::msg(e.to_string()))
}

/// Gets the archive for a mod version: straight from the mod library when that exact version is
/// there (works offline and for mods that aren't on the Mod DB), otherwise via the Mod DB.
/// Returns (archive, is temporary, version, file name).
#[allow(clippy::too_many_arguments)]
async fn obtain<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    db: &ModDb,
    env: &DetectedEnvironment,
    key: &str,
    mod_id: &str,
    installed_version: Option<&str>,
    version: Option<&str>,
) -> AppResult<(PathBuf, bool, String, Option<String>)> {
    if let Some(v) = version {
        if let Some(path) = db.cached_mods().locate(mod_id, v, None) {
            if moddb::install::validate_archive(&path).is_ok() {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string());
                return Ok((path, false, v.to_string(), name));
            }
        }
    }
    let release = release_for(db, env, mod_id, installed_version, version).await?;
    let (archive, temporary) = fetch(app, state, db, env, key, mod_id, &release).await?;
    Ok((archive, temporary, release.version, release.file_name))
}

pub(crate) async fn update_one<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    db: &ModDb,
    env: &DetectedEnvironment,
    m: &InstalledMod,
    version: Option<&str>,
) -> AppResult<String> {
    let target = match m.source_kind {
        ModSourceKind::Zip => InstallTarget::ReplaceFile { existing: PathBuf::from(&m.source_path) },
        ModSourceKind::Folder => InstallTarget::ReplaceFolder { existing: PathBuf::from(&m.source_path) },
        _ => return Err(AppError::msg("Loose .cs and .dll mods can't be updated automatically.")),
    };
    let (archive, temporary, new_version, file_name) = obtain(app, state, db, env, &m.key, &m.mod_id, m.version.as_deref(), version).await?;
    emit(app, &m.key, UpdateStage::Installing, None, None);
    let plan = InstallPlan {
        mod_id: m.mod_id.clone(),
        installed_version: m.version.clone(),
        new_version: new_version.clone(),
        release_file_name: file_name,
        target,
        cache_replaced: env.settings.mod_cache_limit_mb > 0,
    };
    let result = {
        let _guard = state.mods_lock.lock_safe();
        let r = moddb::install_archive(&plan, &archive, db.cached_mods());
        // Keep a disabled mod disabled under its new version.
        if r.is_ok() && !m.is_active {
            if let Ok((data, _)) = state.mod_roots() {
                if let Ok(mut cs) = ClientSettings::load(&data) {
                    cs.set_active(&m.mod_id, m.version.as_deref(), true);
                    cs.set_active(&m.mod_id, Some(&new_version), false);
                    let _ = cs.save();
                }
            }
        }
        r
    };
    if temporary {
        let _ = std::fs::remove_file(&archive);
    }
    result?;
    Ok(new_version)
}

/// Downloads and installs a mod that isn't installed yet into `<data>/Mods`. Emits progress
/// under `key`. Doesn't rescan.
pub(crate) async fn install_new<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    db: &ModDb,
    env: &DetectedEnvironment,
    key: &str,
    mod_id: &str,
    version: Option<&str>,
) -> AppResult<String> {
    let (data, _) = state.mod_roots()?;
    let (archive, temporary, new_version, file_name) = obtain(app, state, db, env, key, mod_id, None, version).await?;
    emit(app, key, UpdateStage::Installing, None, None);
    let plan = InstallPlan {
        mod_id: mod_id.to_string(),
        installed_version: None,
        new_version: new_version.clone(),
        release_file_name: file_name,
        target: InstallTarget::NewFile { mods_dir: data.join("Mods") },
        cache_replaced: false,
    };
    let r = {
        let _guard = state.mods_lock.lock_safe();
        moddb::install_archive(&plan, &archive, db.cached_mods())
    };
    if temporary {
        let _ = std::fs::remove_file(&archive);
    }
    r?;
    Ok(new_version)
}

/// Installs a mod from the Mod DB into the data folder's Mods folder. `version` null = the
/// newest release compatible with the target game version.
#[tauri::command]
#[specta::specta]
pub async fn install_mod(app: AppHandle, state: State<'_, AppState>, mod_id: String, version: Option<String>) -> AppResult<ModList> {
    let _run = state.update_lock.lock().await;
    state.ensure_game_closed()?;
    state.cancel_updates.store(false, Ordering::Relaxed);
    let db = state.moddb()?;
    let env = state.environment();
    let key = format!("install:{mod_id}");
    let outcome = install_new(&app, &state, &db, &env, &key, &mod_id, version.as_deref()).await;
    match &outcome {
        Ok(_) => emit(&app, &key, UpdateStage::Done, None, None),
        Err(e) => emit(&app, &key, UpdateStage::Failed, None, Some(e.message.clone())),
    }
    outcome?;
    state.rescan()?;
    prune_cache(&state, &db, &env);
    current_list(&state)
}

// ---------------------------------------------------------------------------------------------
// Mod browser
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowseFacets {
    pub tags: Vec<TagCount>,
    /// Stable game versions, newest first.
    pub game_versions: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn browse_facets(state: State<'_, AppState>) -> AppResult<BrowseFacets> {
    let db = state.moddb()?;
    let (entries, _) = db.catalogue(false).await.map_err(|e| AppError::msg(e.to_string()))?;
    let tags = moddb::browse::tag_counts(&entries).into_iter().map(|(name, count)| TagCount { name, count }).collect();
    let mut game_versions: Vec<String> = db
        .game_versions()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|g| g.name)
        .filter(|n| !n.contains('-'))
        .collect();
    game_versions.sort_by(|a, b| {
        if svsm_core::version::is_candidate_newer(Some(a), Some(b)) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    game_versions.dedup();
    Ok(BrowseFacets { tags, game_versions })
}

/// Searches the Mod DB catalogue (cached locally; `refresh` downloads it again).
/// `gameVersion` limits results to mods with a release for that game version.
#[tauri::command]
#[specta::specta]
pub async fn browse_mods(
    state: State<'_, AppState>,
    query: BrowseQuery,
    game_version: Option<String>,
    refresh: bool,
) -> AppResult<BrowsePage> {
    let db = state.moddb()?;
    let env = state.environment();
    let (mut entries, fetched_ms) = db.catalogue(refresh).await.map_err(|e| AppError::msg(e.to_string()))?;
    if let Some(gv) = game_version.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        let tag = db
            .game_versions()
            .await
            .map_err(|e| AppError::msg(e.to_string()))?
            .into_iter()
            .find(|g| g.name == gv)
            .ok_or_else(|| AppError::msg(format!("Unknown game version {gv}.")))?;
        let ids: HashSet<u32> = db
            .catalogue_ids_for_game_version(&tag.id, refresh)
            .await
            .map_err(|e| AppError::msg(e.to_string()))?
            .into_iter()
            .collect();
        entries.retain(|e| ids.contains(&e.numeric_id));
    }
    let installed: HashSet<String> =
        state.mods.read_safe().mods.iter().map(|m| m.mod_id.to_ascii_lowercase()).collect();
    let settings = &env.settings;
    Ok(moddb::browse(
        &entries,
        &query,
        |e| e.mod_ids.iter().any(|id| installed.contains(&id.to_ascii_lowercase())),
        |e| e.mod_ids.iter().any(|id| settings.is_favourite(id)) || settings.is_favourite(&e.mod_id),
        fetched_ms,
    ))
}

/// The Mod DB's own card thumbnails for browser cards, by lower-case mod id. The catalogue's
/// `logo` is often a different picture; this looks up each mod's `logofiledb` (cached for a
/// week). Mods that couldn't be looked up are left out; `null` = the mod has no picture.
#[tauri::command]
#[specta::specta]
pub async fn browse_thumbnails(state: State<'_, AppState>, mod_ids: Vec<String>) -> AppResult<HashMap<String, Option<String>>> {
    let db = state.moddb()?;
    Ok(db.thumbnails(&mod_ids).await)
}

// ---------------------------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagerUpdate {
    pub version: String,
    pub page_url: Option<String>,
}

/// A newer Simple VS Manager on the Mod DB, if there is one.
#[tauri::command]
#[specta::specta]
pub async fn manager_update(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<ManagerUpdate>> {
    let db = state.moddb()?;
    let (cached, _) = db.get_mod(moddb::MANAGER_MOD_ID, false).await;
    let current = app.package_info().version.to_string();
    let Some(raw) = cached.and_then(|c| c.raw) else { return Ok(None) };
    let info = parse::build_info(moddb::MANAGER_MOD_ID, &raw, None, false, 0.0);
    Ok(info
        .latest_release()
        .filter(|r| svsm_core::version::is_candidate_newer(Some(&r.version), Some(&current)))
        .map(|r| ManagerUpdate { version: r.version.clone(), page_url: info.page_url.clone() }))
}

/// Opens an http(s) link (Mod DB pages, links inside descriptions) in the default browser.
#[tauri::command]
#[specta::specta]
pub fn open_url(app: AppHandle, url: String) -> AppResult<()> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::msg("Only web links can be opened."));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::msg(e.to_string()))
}

