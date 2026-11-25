//! IPC commands. Keep these thin: validate input, call `svsm-core`, map errors.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use svsm_core::clientsettings::ClientSettings;
use svsm_core::mods::InstalledMod;
use svsm_core::{paths, DetectedEnvironment};

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::watcher::ModWatcher;

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// App name and version for the title bar and About box.
#[tauri::command]
#[specta::specta]
pub fn app_info(app: AppHandle) -> AppInfo {
    let pkg = app.package_info();
    AppInfo { name: pkg.name.clone(), version: pkg.version.to_string() }
}

// ---------------------------------------------------------------------------------------------
// Environment and settings
// ---------------------------------------------------------------------------------------------

/// Returns the detected folders and game version (cached after the first call).
#[tauri::command]
#[specta::specta]
pub async fn get_environment(state: State<'_, AppState>) -> AppResult<DetectedEnvironment> {
    Ok(state.environment())
}

/// Runs detection again, e.g. after the user installs the game.
#[tauri::command]
#[specta::specta]
pub async fn refresh_environment(state: State<'_, AppState>) -> AppResult<DetectedEnvironment> {
    Ok(state.refresh_environment())
}

/// Sets (or with `null`, clears back to auto-detect) the game folder.
#[tauri::command]
#[specta::specta]
pub async fn set_game_directory(state: State<'_, AppState>, path: Option<String>) -> AppResult<DetectedEnvironment> {
    if let Some(p) = path.as_deref() {
        let dir = Path::new(p);
        if paths::find_game_executable(state.host.os, dir).is_none() {
            return Err(AppError::msg(format!(
                "That folder doesn't contain Vintage Story ({}).",
                state.host.os.game_executable_candidates()[0].display()
            )));
        }
    }
    state.update_settings(|s| s.game_directory = path)
}

/// Sets (or with `null`, clears back to auto-detect) the data folder.
#[tauri::command]
#[specta::specta]
pub async fn set_data_directory(state: State<'_, AppState>, path: Option<String>) -> AppResult<DetectedEnvironment> {
    if let Some(p) = path.as_deref() {
        if !Path::new(p).is_dir() {
            return Err(AppError::msg("That folder doesn't exist."));
        }
    }
    state.update_settings(|s| s.data_directory = path)
}

#[tauri::command]
#[specta::specta]
pub async fn set_close_after_launch(state: State<'_, AppState>, enabled: bool) -> AppResult<DetectedEnvironment> {
    state.update_settings(|s| s.close_after_launch = enabled)
}

/// Which known folder to open. The frontend never passes raw paths to open.
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KnownFolder {
    Manager,
    Data,
    Game,
    Mods,
    Logs,
    Modlists,
    ModConfig,
    DataBackups,
}

/// Opens one of the detected folders in the system file manager.
#[tauri::command]
#[specta::specta]
pub fn open_known_folder(app: AppHandle, state: State<'_, AppState>, folder: KnownFolder) -> AppResult<()> {
    let env = state.environment();
    let data = env.data_dir.as_ref().map(|d| PathBuf::from(&d.path));
    let path: Option<PathBuf> = match folder {
        KnownFolder::Manager => Some(PathBuf::from(env.manager_dir.path)),
        KnownFolder::Data => data,
        KnownFolder::Game => env.game_dir.map(|g| PathBuf::from(g.path)),
        KnownFolder::Mods => data.map(|d| d.join("Mods")),
        KnownFolder::Logs => data.map(|d| d.join("Logs")),
        KnownFolder::ModConfig => data.map(|d| d.join("ModConfig")),
        KnownFolder::DataBackups => {
            let dir = crate::m5_commands::data_backups(&state).root().to_path_buf();
            let _ = std::fs::create_dir_all(&dir);
            Some(dir)
        }
        KnownFolder::Modlists => {
            let dir = PathBuf::from(&env.manager_dir.path).join(svsm_core::modlists::store::MODLISTS_DIR);
            let _ = std::fs::create_dir_all(&dir);
            Some(dir)
        }
    };
    let path = path.ok_or_else(|| AppError::msg("That folder hasn't been found yet."))?;
    if !path.exists() {
        return Err(AppError::msg(format!("The folder doesn't exist: {}", path.display())));
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| AppError::msg(format!("Could not open {}: {e}", path.display())))
}

// ---------------------------------------------------------------------------------------------
// Installed mods
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModList {
    pub mods: Vec<InstalledMod>,
    pub search_paths: Vec<String>,
    pub data_dir: String,
    /// False when clientsettings.json is missing (the game hasn't been started yet).
    pub client_settings_found: bool,
}

pub(crate) fn current_list(state: &AppState) -> AppResult<ModList> {
    let (data, _) = state.mod_roots()?;
    let found = ClientSettings::load(&data).map(|s| s.loaded_from_disk()).unwrap_or(false);
    let snap = state.mods.read().expect("mods lock poisoned");
    Ok(ModList {
        mods: snap.mods.clone(),
        search_paths: snap.search_paths.iter().map(|p| p.display().to_string()).collect(),
        data_dir: data.display().to_string(),
        client_settings_found: found,
    })
}

fn restart_watcher(app: &AppHandle, state: &AppState) {
    let Ok((data, _)) = state.mod_roots() else { return };
    let paths = state.mods.read().expect("mods lock poisoned").search_paths.clone();
    let mut slot = state.watcher.lock().expect("watcher lock poisoned");
    let unchanged = slot
        .as_ref()
        .is_some_and(|w| w.watched.iter().filter(|p| **p != data).eq(paths.iter().filter(|p| p.is_dir())));
    if unchanged {
        return;
    }
    *slot = match ModWatcher::start(app.clone(), &paths, &data) {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::warn!("Could not watch mod folders: {e}");
            None
        }
    };
}

/// Scans the mod folders and returns the installed mods.
#[tauri::command]
#[specta::specta]
pub async fn list_mods(app: AppHandle, state: State<'_, AppState>) -> AppResult<ModList> {
    state.rescan()?;
    restart_watcher(&app, &state);
    current_list(&state)
}

/// Enables or disables mods (by key) in clientsettings.json, then rescans.
#[tauri::command]
#[specta::specta]
pub async fn set_mods_active(state: State<'_, AppState>, keys: Vec<String>, active: bool) -> AppResult<ModList> {
    {
        let _guard = state.mods_lock.lock().expect("mods lock poisoned");
        let (data, _) = state.mod_roots()?;
        let mut settings = ClientSettings::load(&data)?;
        let mut changed = false;
        for key in &keys {
            let m = state.find_mod(key)?;
            if m.mod_id.trim().is_empty() {
                continue;
            }
            changed |= settings.set_active(&m.mod_id, m.version.as_deref(), active);
        }
        if changed {
            settings.save()?;
            state.note_own_settings_write();
        }
    }
    state.rescan()?;
    current_list(&state)
}

/// Moves mods to the system trash, then rescans.
#[tauri::command]
#[specta::specta]
pub async fn delete_mods(state: State<'_, AppState>, keys: Vec<String>) -> AppResult<ModList> {
    crate::modlist_commands::snapshot_installed(&state, "ModsDeleted", Vec::new());
    {
        let _guard = state.mods_lock.lock().expect("mods lock poisoned");
        let search_paths = state.mods.read().expect("mods lock poisoned").search_paths.clone();
        for key in &keys {
            let m = state.find_mod(key)?;
            svsm_core::mods::delete_mod(Path::new(&m.source_path), &search_paths)?;
        }
    }
    state.rescan()?;
    current_list(&state)
}

/// Shows a mod's file or folder in the system file manager.
#[tauri::command]
#[specta::specta]
pub fn reveal_mod(app: AppHandle, state: State<'_, AppState>, key: String) -> AppResult<()> {
    let m = state.find_mod(&key)?;
    app.opener()
        .reveal_item_in_dir(&m.source_path)
        .map_err(|e| AppError::msg(format!("Could not show {}: {e}", m.source_path)))
}

/// Opens a mod's website. Only http(s) links are allowed.
#[tauri::command]
#[specta::specta]
pub fn open_mod_website(app: AppHandle, state: State<'_, AppState>, key: String) -> AppResult<()> {
    let m = state.find_mod(&key)?;
    let url = m.website.as_deref().map(str::trim).unwrap_or_default();
    let url = if url.contains("://") { url.to_string() } else { format!("https://{url}") };
    if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() <= "https://".len() {
        return Err(AppError::msg("This mod has no website link."));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::msg(e.to_string()))
}

// ---------------------------------------------------------------------------------------------
// Game
// ---------------------------------------------------------------------------------------------

/// Starts Vintage Story with `--dataPath`, or through the custom shortcut when one is set (as
/// 2.x does). Closes the manager afterwards if that setting is on.
#[tauri::command]
#[specta::specta]
pub async fn launch_game<R: tauri::Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> AppResult<()> {
    let env = state.environment();
    let shortcut = env.settings.custom_shortcut();
    if let Some(sc) = &shortcut {
        if !sc.is_file() {
            state.update_settings(|s| s.custom_shortcut_path = None)?;
            return Err(AppError::msg(format!(
                "The custom Vintage Story shortcut wasn't found ({}), so it was cleared. Set it again under Settings, or press Launch to start the game directly.",
                sc.display()
            )));
        }
    }
    let exe = match &shortcut {
        Some(_) => None,
        None => Some(env.game_executable.clone().ok_or_else(|| AppError::msg("Vintage Story wasn't found. Pick the game folder under Folders."))?),
    };
    let data = match &shortcut {
        Some(_) => None,
        None => Some(state.mod_roots()?.0),
    };
    if env.settings.auto_data_backups {
        let _run = state.update_lock.lock().await;
        crate::m5_commands::run_backup(&app, &state)
            .await
            .map_err(|e| AppError::msg(format!("The automatic backup failed, so the game wasn't started: {}", e.message)))?;
    }
    match (&shortcut, exe, data) {
        (Some(sc), _, _) => app
            .opener()
            .open_path(sc.display().to_string(), None::<&str>)
            .map_err(|e| AppError::msg(format!("Couldn't start Vintage Story through the shortcut: {e}")))?,
        (None, Some(exe), Some(data)) => svsm_core::launch::launch_game(state.host.os, Path::new(&exe), &data)?,
        _ => unreachable!("exe and data are set when there is no shortcut"),
    }
    if env.settings.close_after_launch {
        app.exit(0);
    }
    Ok(())
}

/// Sets (or with `None` clears) the custom shortcut used to start the game.
#[tauri::command]
#[specta::specta]
pub async fn set_custom_shortcut(state: State<'_, AppState>, path: Option<String>) -> AppResult<DetectedEnvironment> {
    let path = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    if let Some(p) = &path {
        if !Path::new(p).is_file() {
            return Err(AppError::msg("That file doesn't exist."));
        }
    }
    state.update_settings(|s| s.custom_shortcut_path = path)
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagerFolderMove {
    pub folder: String,
    /// Some files of the old folder were in use and stayed behind (the copy is complete).
    pub old_folder_left: bool,
}

/// Moves the manager's settings and cache folder into `<parent>/Simple VS Manager`, or back to
/// the default location when `parent` is `None`. Restart the app afterwards (`restart_app`).
#[tauri::command]
#[specta::specta]
pub async fn move_manager_folder(state: State<'_, AppState>, parent: Option<String>) -> AppResult<ManagerFolderMove> {
    let from = state.manager_dir();
    let to = match parent.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => paths::manager_dir_in(&state.host, Path::new(p)),
        None => paths::default_manager_dir(&state.host),
    };
    let _run = state.update_lock.lock().await;
    let host = state.host.clone();
    let moved = tauri::async_runtime::spawn_blocking(move || paths::move_manager_dir(&host, &from, &to))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
        .map_err(|e| AppError::msg(format!("The folder wasn't moved: {e}")))?;
    Ok(ManagerFolderMove { folder: moved.to.display().to_string(), old_folder_left: moved.old_folder_left })
}

/// Restarts the manager (after moving its folder).
#[tauri::command]
#[specta::specta]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
