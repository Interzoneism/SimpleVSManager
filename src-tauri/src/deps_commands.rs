//! "Fix dependencies": installs, updates and enables whatever a mod needs to load.
//! Port of 2.x `MainWindow.FixModButton_OnClick`.

use svsm_core::sync::{LockExt as _, RwLockExt as _};
use std::collections::HashSet;
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use svsm_core::clientsettings::ClientSettings;
use svsm_core::moddb::ModDb;
use svsm_core::mods::{plan_dependency_fixes, release_for_dependency, DependencyAction};
use svsm_core::DetectedEnvironment;

use crate::commands::{current_list, ModList};
use crate::error::{AppError, AppResult};
use crate::moddb_commands::{emit, info_from, install_new, prune_cache, update_one, UpdateStage};
use crate::state::AppState;

/// Rounds of plan, act, rescan. Each round can reveal the dependencies of what it installed.
const MAX_ROUNDS: usize = 10;

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DependencyFixReport {
    /// What was done, one line each ("Installed carryonlib 1.0.0").
    pub done: Vec<String>,
    /// What couldn't be fixed, one line each.
    pub failures: Vec<String>,
    pub list: ModList,
}

/// Installs missing dependencies, updates ones that are too old, and enables disabled ones, for
/// the mod with `key` and, in turn, for those dependencies. Emits `UpdateProgress` under `key`.
#[tauri::command]
#[specta::specta]
pub async fn fix_dependencies(app: AppHandle, state: State<'_, AppState>, key: String) -> AppResult<DependencyFixReport> {
    let _run = state.update_lock.lock().await;
    state.ensure_game_closed()?;
    fix(&app, &state, &key).await
}

pub(crate) async fn fix<R: Runtime>(app: &AppHandle<R>, state: &AppState, key: &str) -> AppResult<DependencyFixReport> {
    state.cancel_updates.store(false, Ordering::Relaxed);
    let db = state.moddb()?;
    let env = state.environment();
    state.rescan()?;

    let mut done = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut attempted = HashSet::new();
    let mut snapshotted = false;

    for _ in 0..MAX_ROUNDS {
        let plan = current_plan(state, key)?;
        let fresh: Vec<_> = plan.actions.into_iter().filter(|a| attempted.insert(a.attempt_key())).collect();
        if fresh.is_empty() {
            break;
        }
        if !snapshotted && fresh.iter().any(|a| !matches!(a, DependencyAction::Enable { .. })) {
            crate::modlist_commands::snapshot_installed(state, "ModsUpdated", Vec::new());
            snapshotted = true;
        }
        for action in fresh {
            if state.cancel_updates.load(Ordering::Relaxed) {
                failures.push((action.mod_id().to_string(), "Cancelled.".into()));
                continue;
            }
            match run(app, state, &db, &env, key, &action).await {
                Ok(line) => done.push(line),
                Err(e) => failures.push((action.mod_id().to_string(), e.message)),
            }
        }
        state.rescan()?;
    }

    // Whatever the final plan still lists wasn't fixed.
    let plan = current_plan(state, key)?;
    let failed: HashSet<String> = failures.iter().map(|(id, _)| id.to_lowercase()).collect();
    let mut lines: Vec<String> = failures.into_iter().map(|(id, msg)| format!("{id}: {msg}")).collect();
    for p in plan.problems {
        lines.push(format!("{}: {}", p.mod_id, p.message));
    }
    for a in plan.actions {
        if !failed.contains(&a.mod_id().to_lowercase()) {
            lines.push(format!("{}: still not satisfied after the fix.", a.mod_id()));
        }
    }

    let stage = if lines.is_empty() { UpdateStage::Done } else { UpdateStage::Failed };
    emit(app, key, stage, None, None);
    if !done.is_empty() {
        prune_cache(state, &db, &env);
    }
    Ok(DependencyFixReport { done, failures: lines, list: current_list(state)? })
}

fn current_plan(state: &AppState, key: &str) -> AppResult<svsm_core::mods::DependencyPlan> {
    let mods = state.mods.read_safe().mods.clone();
    plan_dependency_fixes(&mods, key).ok_or_else(|| AppError::msg("That mod is no longer installed. The list will refresh."))
}

async fn run<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    db: &ModDb,
    env: &DetectedEnvironment,
    root_key: &str,
    action: &DependencyAction,
) -> AppResult<String> {
    match action {
        DependencyAction::Enable { key, mod_id } => {
            let m = state.find_mod(key)?;
            let _guard = state.mods_lock.lock_safe();
            let (data, _) = state.mod_roots()?;
            let mut settings = ClientSettings::load(&data)?;
            if settings.set_active(&m.mod_id, m.version.as_deref(), true) {
                settings.save()?;
            }
            Ok(format!("Enabled {mod_id}"))
        }
        DependencyAction::Install { mod_id, min_version } => {
            emit(app, root_key, UpdateStage::Downloading, None, Some(format!("Installing {mod_id}…")));
            let version = pick_release(db, env, mod_id, min_version).await?;
            let v = install_new(app, state, db, env, root_key, mod_id, Some(&version)).await?;
            Ok(format!("Installed {mod_id} {v}"))
        }
        DependencyAction::Update { key, mod_id, from_version, min_version } => {
            emit(app, root_key, UpdateStage::Downloading, None, Some(format!("Updating {mod_id}…")));
            let version = pick_release(db, env, mod_id, min_version).await?;
            let m = state.find_mod(key)?;
            let v = update_one(app, state, db, env, &m, Some(&version)).await?;
            let from = from_version.as_deref().unwrap_or("?");
            Ok(format!("Updated {mod_id} {from} → {v}"))
        }
    }
}

async fn pick_release(db: &ModDb, env: &DetectedEnvironment, mod_id: &str, min_version: &str) -> AppResult<String> {
    let (cached, error) = db.get_mod(mod_id, false).await;
    let info = cached.as_ref().and_then(|c| info_from(c, mod_id, env)).ok_or_else(|| {
        AppError::msg(error.unwrap_or_else(|| "Not found on the Mod DB.".to_string()))
    })?;
    let release = release_for_dependency(&info, min_version).ok_or_else(|| {
        let min = min_version.trim();
        if min.is_empty() || min == "*" {
            AppError::msg("The Mod DB has no release for your game version.")
        } else {
            AppError::msg(format!("The Mod DB has no release of v{min} or newer for your game version."))
        }
    })?;
    Ok(release.version.clone())
}
