//! Data folder backups, mod config files, game profiles and appearance.

use svsm_core::sync::RwLockExt as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_specta::Event;

use svsm_core::backups::{BackupSummary, DataBackups};
use svsm_core::modlists::configs::{self, ConfigCandidate, ConfigDocument};
use svsm_core::DetectedEnvironment;

use crate::error::{AppError, AppResult};
use crate::events::BackupProgress;
use crate::state::AppState;

// ---------------------------------------------------------------------------------------------
// Data folder backups
// ---------------------------------------------------------------------------------------------

pub(crate) fn data_backups(state: &AppState) -> DataBackups {
    let env = state.environment();
    let custom = env.settings.data_backup_location.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from);
    DataBackups::new(&state.manager_dir(), custom.as_deref())
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DataBackupList {
    pub backups: Vec<BackupSummary>,
    pub location: String,
    /// Backups made from the current data folder (restore makes most sense for those).
    pub data_dir: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn list_data_backups(state: State<'_, AppState>) -> AppResult<DataBackupList> {
    let b = data_backups(&state);
    Ok(DataBackupList {
        backups: b.list(),
        location: b.root().display().to_string(),
        data_dir: state.mod_roots().ok().map(|(d, _)| d.display().to_string()),
    })
}

/// Runs a backup on a worker thread, emitting `BackupProgress`, then prunes.
pub(crate) async fn run_backup<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<BackupSummary> {
    let (data, _) = state.mod_roots()?;
    let env = state.environment();
    let b = data_backups(state);
    let version = env.game_version.clone();
    let max = env.settings.max_data_backups;
    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut last = -1.0f64;
        let mut progress = |done: u64, total: u64, label: &str| {
            let pct = if total == 0 { 0.0 } else { done as f64 * 100.0 / total as f64 };
            if pct - last >= 1.0 || done == total {
                last = pct;
                let _ = BackupProgress { percent: pct, label: label.to_string() }.emit(&app2);
            }
        };
        let made = b.create(&data, version.as_deref(), &mut progress, &|| false)?;
        b.prune(&data, version.as_deref(), max);
        Ok::<_, svsm_core::CoreError>(made)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(result?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_data_backup<R: tauri::Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> AppResult<BackupSummary> {
    let _run = state.update_lock.lock().await;
    run_backup(&app, &state).await
}

/// Replaces the data folder with a backup. With `safety`, the current state is backed up first.
#[tauri::command]
#[specta::specta]
pub async fn restore_data_backup<R: tauri::Runtime>(app: AppHandle<R>, state: State<'_, AppState>, id: String, safety: bool) -> AppResult<Option<BackupSummary>> {
    let _run = state.update_lock.lock().await;
    state.ensure_game_closed()?;
    let safety_backup = if safety { Some(run_backup(&app, &state).await?) } else { None };
    let (data, _) = state.mod_roots()?;
    let b = data_backups(&state);
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut progress = |done: u64, total: u64, label: &str| {
            let pct = if total == 0 { 0.0 } else { done as f64 * 100.0 / total as f64 };
            let _ = BackupProgress { percent: pct, label: label.to_string() }.emit(&app2);
        };
        b.restore(&id, &data, &mut progress)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    let _ = state.rescan();
    Ok(safety_backup)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_data_backup(state: State<'_, AppState>, id: String) -> AppResult<()> {
    Ok(data_backups(&state).delete(&id)?)
}

// ---------------------------------------------------------------------------------------------
// Mod config files
// ---------------------------------------------------------------------------------------------

fn config_backup_dir(state: &AppState) -> PathBuf {
    state.manager_dir().join("Temp Cache").join("Config Backups")
}

/// Config files in `ModConfig`, matched to installed mods where possible.
#[tauri::command]
#[specta::specta]
pub async fn list_mod_configs(state: State<'_, AppState>) -> AppResult<Vec<ConfigCandidate>> {
    let (data, _) = state.mod_roots()?;
    let mods = state.mods.read_safe().mods.clone();
    Ok(configs::candidates(&data, &mods, &configs::legacy_config_paths(&state.manager_dir())))
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub document: ConfigDocument,
    /// The file as it was before the last save here (for undo).
    pub previous: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn read_mod_config(state: State<'_, AppState>, relative_path: String) -> AppResult<ConfigFile> {
    let (data, _) = state.mod_roots()?;
    Ok(ConfigFile {
        document: configs::read_config(&data, &relative_path)?,
        previous: configs::previous_version(&config_backup_dir(&state), &relative_path),
    })
}

/// Saves a config file if it parses; the previous version is kept for undo.
#[tauri::command]
#[specta::specta]
pub async fn save_mod_config(state: State<'_, AppState>, relative_path: String, text: String) -> AppResult<ConfigFile> {
    let (data, _) = state.mod_roots()?;
    let backups = config_backup_dir(&state);
    Ok(ConfigFile {
        document: configs::save_config(&data, &relative_path, &text, &backups)?,
        previous: configs::previous_version(&backups, &relative_path),
    })
}

// ---------------------------------------------------------------------------------------------
// Game profiles
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "action")]
pub enum ProfileChange {
    Switch { name: String },
    Rename { from: String, to: String },
    Delete { name: String },
}

/// Switches, creates (switching to a new name), renames or deletes a game profile.
#[tauri::command]
#[specta::specta]
pub async fn change_profile(state: State<'_, AppState>, change: ProfileChange) -> AppResult<DetectedEnvironment> {
    let mut problem = None;
    let env = state.update_settings(|s| {
        let r = match &change {
            ProfileChange::Switch { name } => s.switch_profile(name),
            ProfileChange::Rename { from, to } => s.rename_profile(from, to),
            ProfileChange::Delete { name } => s.delete_profile(name),
        };
        problem = r.err();
    })?;
    if let Some(p) = problem {
        return Err(AppError::msg(p));
    }
    // Folders may have changed: forget the old scan.
    *state.mods.write_safe() = Default::default();
    Ok(env)
}

// ---------------------------------------------------------------------------------------------
// Appearance and misc settings
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct MorePreferences {
    pub theme: Option<String>,
    pub verbose_logging: Option<bool>,
    pub auto_data_backups: Option<bool>,
    pub max_data_backups: Option<u32>,
    /// `""` = back to the default location.
    pub data_backup_location: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn update_more_preferences(state: State<'_, AppState>, patch: MorePreferences) -> AppResult<DetectedEnvironment> {
    if let Some(t) = &patch.theme {
        if !["darkVs", "dark", "light", "custom"].contains(&t.as_str()) {
            return Err(AppError::msg("Unknown theme."));
        }
    }
    if let Some(loc) = patch.data_backup_location.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        if !Path::new(loc).is_dir() {
            return Err(AppError::msg("That folder doesn't exist."));
        }
    }
    state.update_settings(|s| {
        if let Some(t) = patch.theme {
            s.theme = t;
        }
        if let Some(v) = patch.verbose_logging {
            s.verbose_logging = v;
        }
        if let Some(v) = patch.auto_data_backups {
            s.auto_data_backups = v;
        }
        if let Some(v) = patch.max_data_backups {
            s.max_data_backups = v;
        }
        if let Some(l) = patch.data_backup_location {
            let l = l.trim().to_string();
            s.data_backup_location = (!l.is_empty()).then_some(l);
        }
    })
}
