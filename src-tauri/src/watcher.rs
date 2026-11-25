//! Watches mod folders and clientsettings.json and tells the UI to refresh.

use std::path::PathBuf;
use std::time::Duration;

use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::events::ModsChanged;

pub struct ModWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    pub watched: Vec<PathBuf>,
}

impl ModWatcher {
    /// Watches each existing search path (one level deep is enough: mods are direct children,
    /// but folder mods change inside, so watch recursively) plus the data folder itself for
    /// clientsettings.json. Missing folders are skipped.
    pub fn start(app: AppHandle, search_paths: &[PathBuf], data_dir: &PathBuf) -> notify_debouncer_full::notify::Result<Self> {
        let data_dir_c = data_dir.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(600), None, move |result: DebounceEventResult| {
            let Ok(events) = result else { return };
            let is_settings_file = |p: &std::path::Path| {
                p.parent() == Some(data_dir_c.as_path())
                    && p.file_name().is_some_and(|n| n == svsm_core::clientsettings::FILE_NAME)
            };
            let paths: Vec<&std::path::Path> = events
                .iter()
                .filter(|e| !e.kind.is_access())
                .flat_map(|e| e.paths.iter().map(|p| p.as_path()))
                // Events from the data folder only matter for clientsettings.json.
                .filter(|p| p.parent() != Some(data_dir_c.as_path()) || is_settings_file(p))
                .collect();
            if paths.is_empty() {
                return;
            }
            // Our own save of clientsettings.json already returned a fresh list.
            let own_write = paths.iter().all(|p| is_settings_file(p))
                && app.try_state::<crate::state::AppState>().is_some_and(|s| s.is_own_settings_write());
            if !own_write {
                let _ = ModsChanged.emit(&app);
            }
        })?;

        let mut watched = Vec::new();
        for p in search_paths.iter().filter(|p| p.is_dir()) {
            if debouncer.watch(p, RecursiveMode::Recursive).is_ok() {
                watched.push(p.clone());
            }
        }
        // clientsettings.json is replaced by rename when saved, so watch its folder (not
        // recursively: saves and logs live below it) and filter by name above.
        if data_dir.is_dir() && debouncer.watch(data_dir, RecursiveMode::NonRecursive).is_ok() {
            watched.push(data_dir.clone());
        }
        Ok(Self { _debouncer: debouncer, watched })
    }
}
