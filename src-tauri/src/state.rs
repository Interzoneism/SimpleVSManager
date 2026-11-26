use svsm_core::sync::{LockExt as _, RwLockExt as _};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use svsm_core::clientsettings::ClientSettings;
use svsm_core::moddb::ModDb;
use svsm_core::mods::{discover_mods, InstalledMod};
use svsm_core::{DetectedEnvironment, HostEnv, Settings};

use crate::error::{AppError, AppResult};
use crate::watcher::ModWatcher;

/// Latest scan of installed mods.
#[derive(Default)]
pub struct ModSnapshot {
    pub mods: Vec<InstalledMod>,
    pub icons: HashMap<String, Arc<Vec<u8>>>,
    pub search_paths: Vec<PathBuf>,
}

/// Finds a running Vintage Story client.
pub type GameProbe = Arc<dyn Fn() -> Option<svsm_core::game_process::RunningGame> + Send + Sync>;

/// Shared app state. Long-lived services hang off this.
pub struct AppState {
    pub host: HostEnv,
    /// Firebase endpoints (tests point these at a local server).
    pub cloud_config: svsm_core::cloud::CloudConfig,
    /// The old Firebase project (only for moving identities off it).
    pub legacy_cloud_config: svsm_core::cloud::CloudConfig,
    pub environment: RwLock<Option<DetectedEnvironment>>,
    pub mods: RwLock<ModSnapshot>,
    /// Serialises changes to the manager's settings file.
    settings_lock: Mutex<()>,
    /// Serialises writes to clientsettings.json and scans.
    pub mods_lock: Mutex<()>,
    pub watcher: Mutex<Option<ModWatcher>>,
    /// Mod DB client + caches, created on first use for the current manager folder.
    moddb: Mutex<Option<(PathBuf, Arc<ModDb>)>>,
    /// Cloud sign-in for the current manager folder.
    cloud: Mutex<Option<(PathBuf, Arc<svsm_core::cloud::Authenticator>)>>,
    /// Serialises update/install runs.
    pub update_lock: tokio::sync::Mutex<()>,
    pub cancel_updates: AtomicBool,
    /// Until when the watcher ignores clientsettings.json changes, because we wrote it
    /// ourselves and already returned the rescanned list.
    pub own_settings_write_until: Mutex<Option<std::time::Instant>>,
    /// Finds a running Vintage Story client (tests replace this).
    pub game_probe: RwLock<GameProbe>,
    /// Leftovers of interrupted updates are cleaned up once, on the first scan.
    recovered: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self::with_host(HostEnv::current())
    }

    /// State for a given machine view (tests point this at a temporary folder).
    pub fn with_host(host: HostEnv) -> Self {
        Self {
            host,
            cloud_config: svsm_core::cloud::CloudConfig::default(),
            legacy_cloud_config: svsm_core::cloud::CloudConfig::legacy(),
            environment: RwLock::new(None),
            mods: RwLock::new(ModSnapshot::default()),
            mods_lock: Mutex::new(()),
            settings_lock: Mutex::new(()),
            watcher: Mutex::new(None),
            moddb: Mutex::new(None),
            cloud: Mutex::new(None),
            update_lock: tokio::sync::Mutex::new(()),
            cancel_updates: AtomicBool::new(false),
            own_settings_write_until: Mutex::new(None),
            game_probe: RwLock::new(Arc::new(svsm_core::game_process::find_running_game)),
            recovered: AtomicBool::new(false),
        }
    }

    /// Runs detection and caches the result.
    pub fn refresh_environment(&self) -> DetectedEnvironment {
        let env = svsm_core::detect_environment(&self.host);
        *self.environment.write_safe() = Some(env.clone());
        env
    }

    pub fn environment(&self) -> DetectedEnvironment {
        if let Some(env) = self.environment.read_safe().clone() {
            return env;
        }
        self.refresh_environment()
    }

    /// Errors while Vintage Story is running. Call before changing mods, clientsettings.json
    /// or the data folder, and again before each batch of a long run.
    pub fn ensure_game_closed(&self) -> AppResult<()> {
        let probe = self.game_probe.read_safe().clone();
        match probe() {
            Some(g) => {
                tracing::info!("Blocked a change because the game is running ({} pid {})", g.name, g.pid);
                Err(AppError::msg(svsm_core::game_process::GAME_RUNNING_MESSAGE))
            }
            None => Ok(()),
        }
    }

    /// Call after saving clientsettings.json ourselves: the watcher's echo of that save
    /// would only rescan again and make the list refresh twice.
    pub fn note_own_settings_write(&self) {
        *self.own_settings_write_until.lock_safe() = Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
    }

    pub fn is_own_settings_write(&self) -> bool {
        self.own_settings_write_until.lock_safe().is_some_and(|t| std::time::Instant::now() < t)
    }

    pub fn manager_dir(&self) -> PathBuf {
        PathBuf::from(self.environment().manager_dir.path)
    }

    /// Changes and saves the settings. Serialised, so two quick changes can't overwrite each
    /// other. Detection (folder probing, reading game files) only runs again when a setting it
    /// depends on changed.
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> AppResult<DetectedEnvironment> {
        let _guard = self.settings_lock.lock_safe();
        let mut env = self.environment();
        let dir = PathBuf::from(&env.manager_dir.path);
        let mut settings = env.settings.clone();
        let paths_before = (settings.data_dir(), settings.game_dir(), settings.active_profile.clone());
        change(&mut settings);
        settings.save(&dir)?;
        if (settings.data_dir(), settings.game_dir(), settings.active_profile.clone()) != paths_before {
            return Ok(self.refresh_environment());
        }
        env.settings = settings;
        *self.environment.write_safe() = Some(env.clone());
        Ok(env)
    }

    /// The Mod DB service for the current manager folder, with offline mode from settings.
    pub fn moddb(&self) -> AppResult<Arc<ModDb>> {
        let env = self.environment();
        let dir = PathBuf::from(&env.manager_dir.path);
        let mut slot = self.moddb.lock_safe();
        if let Some((d, db)) = slot.as_ref() {
            if *d == dir {
                db.set_offline(env.settings.offline_mode);
                return Ok(db.clone());
            }
        }
        let db = Arc::new(ModDb::new(&dir, env.settings.offline_mode).map_err(|e| AppError::msg(e.to_string()))?);
        *slot = Some((dir, db.clone()));
        Ok(db)
    }

    /// Cloud sign-in (reads the 2.x `firebase-auth.json`). Errors in offline mode.
    pub fn cloud_auth(&self) -> AppResult<Arc<svsm_core::cloud::Authenticator>> {
        let env = self.environment();
        if env.settings.offline_mode {
            return Err(AppError::msg(svsm_core::cloud::CloudError::Offline.to_string()));
        }
        let dir = PathBuf::from(&env.manager_dir.path);
        let mut slot = self.cloud.lock_safe();
        if let Some((d, a)) = slot.as_ref() {
            if *d == dir {
                return Ok(a.clone());
            }
        }
        let a = Arc::new(
            svsm_core::cloud::Authenticator::new(&dir, svsm_core::cloud::Authenticator::default_backup_file(), self.cloud_config.clone())
                .map_err(|e| AppError::msg(e.to_string()))?,
        );
        *slot = Some((dir, a.clone()));
        Ok(a)
    }

    /// Forget the cached sign-in (after the identity file changed).
    pub fn reset_cloud(&self) {
        *self.cloud.lock_safe() = None;
    }

    /// Data folder and game folder for mod operations.
    pub fn mod_roots(&self) -> AppResult<(PathBuf, Option<PathBuf>)> {
        let env = self.environment();
        let data = env
            .data_dir
            .filter(|d| d.exists)
            .ok_or_else(|| AppError::msg("Data folder not found. Set it in Settings."))?;
        Ok((PathBuf::from(data.path), env.game_dir.map(|g| PathBuf::from(g.path))))
    }

    /// Rescans mods and replaces the snapshot.
    pub fn rescan(&self) -> AppResult<Vec<PathBuf>> {
        let _guard = self.mods_lock.lock_safe();
        let (data, game) = self.mod_roots()?;
        let settings = ClientSettings::load(&data)?;
        let mut result = discover_mods(&settings, game.as_deref());
        if !self.recovered.swap(true, std::sync::atomic::Ordering::SeqCst) {
            // Only folders mods are installed into, never the game's own.
            let dirs: Vec<PathBuf> =
                result.search_paths.iter().filter(|p| game.as_ref().is_none_or(|g| !p.starts_with(g))).cloned().collect();
            let fixed = svsm_core::mods::recover_interrupted_installs(&dirs);
            if !fixed.is_empty() {
                for line in &fixed {
                    tracing::warn!("{line}");
                }
                result = discover_mods(&settings, game.as_deref());
            }
        }
        let mut snap = self.mods.write_safe();
        snap.mods = result.mods;
        snap.icons = result.icons.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        snap.search_paths = result.search_paths.clone();
        Ok(result.search_paths)
    }

    pub fn find_mod(&self, key: &str) -> AppResult<InstalledMod> {
        self.mods
            .read_safe()
            .mods
            .iter()
            .find(|m| m.key == key)
            .cloned()
            .ok_or_else(|| AppError::msg("That mod is no longer installed. The list will refresh."))
    }
}
