//! The new app's own settings file: `<manager dir>/svsm3-settings.json`.
//!
//! Kept separate from the WPF app's `SimpleVSManagerConfiguration.json` so the two apps can run
//! side by side without overwriting each other. Unknown fields are ignored and missing ones get
//! defaults, so older and newer versions of this app can share the file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

pub const SETTINGS_FILE_NAME: &str = "svsm3-settings.json";
const CURRENT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    /// Game folder picked by the user. `None` = auto-detect.
    pub game_directory: Option<String>,
    /// Data folder picked by the user. `None` = auto-detect.
    pub data_directory: Option<String>,
    /// Close the manager after launching the game.
    pub close_after_launch: bool,
    /// Never touch the network (Mod DB lookups, downloads).
    pub offline_mode: bool,
    /// Game version to match releases against. `None` = the installed game version,
    /// [`TARGET_LATEST`] = ignore compatibility and always offer the newest release.
    pub target_game_version: Option<String>,
    /// Only treat a release as compatible if it lists the exact game version.
    pub require_exact_version_match: bool,
    /// Size cap for downloaded and replaced mod versions kept under `Temp Cache/Cached Mods`
    /// (for rollback and offline installs). 0 = keep nothing.
    pub mod_cache_limit_mb: u32,
    /// Lower-case mod id -> version the user chose to skip.
    pub skipped_mod_versions: BTreeMap<String, String>,
    /// Lower-case mod ids left out of "Update all".
    pub update_exclusions: Vec<String>,
    /// Lower-case mod ids starred in the mod browser.
    pub favourite_mods: Vec<String>,
    /// Set once the 2.x preferences (skips, exclusions, ...) have been copied over.
    pub legacy_preferences_imported: bool,
    /// Online modlists (registry ids) the user hid.
    pub hidden_online_modlists: Vec<String>,
    /// Online modlist authors (uploader names, lower-case) the user hid.
    pub hidden_online_authors: Vec<String>,
    /// Count long play sessions and ask to confirm which mods work.
    pub mod_usage_tracking: bool,
    /// Set after the first log scan (sessions from before that don't count).
    pub mod_usage_scanned: bool,
    pub mod_usage: crate::sessions::UsageState,
    /// Back up the data folder before every launch.
    pub auto_data_backups: bool,
    /// Data folder backups kept per data folder and game version (0 = all).
    pub max_data_backups: u32,
    /// Where data folder backups go. `None` = `<manager dir>/Data Folder Backups`.
    pub data_backup_location: Option<String>,
    /// `darkVs`, `dark`, `light` or `custom`.
    pub theme: String,
    /// CSS variable name -> color, for `custom` (imported from 2.x palettes).
    pub custom_palette: BTreeMap<String, String>,
    /// Game profiles: each its own game folder, data folder and target version.
    pub profiles: Vec<Profile>,
    pub active_profile: String,
    /// More detailed log output (takes effect at the next start).
    pub verbose_logging: bool,
    /// Set once the 2.x backups, theme and profiles were copied over.
    pub legacy_m5_imported: bool,
    /// Shortcut (or other file) opened instead of the game executable when launching, as in
    /// 2.x's "Set custom Vintage Story shortcut". Per profile.
    pub custom_shortcut_path: Option<String>,
    /// Set once the 2.x custom shortcut was copied over.
    pub legacy_shortcut_imported: bool,
}

/// A game profile. The active profile's values live in the top-level settings fields; the
/// others are stored here and swapped in when switching.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub game_directory: Option<String>,
    pub data_directory: Option<String>,
    pub target_game_version: Option<String>,
    pub custom_shortcut_path: Option<String>,
}

/// Sentinel for [`Settings::target_game_version`]: offer the newest release regardless of game version.
pub const TARGET_LATEST: &str = "latest";
pub const DEFAULT_MOD_CACHE_LIMIT_MB: u32 = 2048;
pub const DEFAULT_PROFILE: &str = "Default";

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA,
            game_directory: None,
            data_directory: None,
            close_after_launch: false,
            offline_mode: false,
            target_game_version: None,
            require_exact_version_match: false,
            mod_cache_limit_mb: DEFAULT_MOD_CACHE_LIMIT_MB,
            skipped_mod_versions: BTreeMap::new(),
            update_exclusions: Vec::new(),
            favourite_mods: Vec::new(),
            legacy_preferences_imported: false,
            hidden_online_modlists: Vec::new(),
            hidden_online_authors: Vec::new(),
            mod_usage_tracking: true,
            mod_usage_scanned: false,
            mod_usage: crate::sessions::UsageState::default(),
            auto_data_backups: false,
            max_data_backups: 0,
            data_backup_location: None,
            theme: "darkVs".into(),
            custom_palette: BTreeMap::new(),
            profiles: Vec::new(),
            active_profile: DEFAULT_PROFILE.into(),
            verbose_logging: false,
            legacy_m5_imported: false,
            custom_shortcut_path: None,
            legacy_shortcut_imported: false,
        }
    }
}

impl Settings {
    pub fn path(manager_dir: &Path) -> PathBuf {
        manager_dir.join(SETTINGS_FILE_NAME)
    }

    /// Loads settings; a missing file gives defaults. A corrupt file is reported, not replaced.
    pub fn load(manager_dir: &Path) -> CoreResult<Self> {
        let path = Self::path(manager_dir);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .map_err(|e| CoreError::Parse { path, message: e.to_string() }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    pub fn save(&self, manager_dir: &Path) -> CoreResult<()> {
        std::fs::create_dir_all(manager_dir).map_err(|e| CoreError::io(manager_dir, e))?;
        let path = Self::path(manager_dir);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).expect("settings always serialize");
        crate::fsutil::write_synced(&tmp, text.as_bytes())?;
        crate::fsutil::rename_with_retry(&tmp, &path)
    }

    pub fn game_dir(&self) -> Option<PathBuf> {
        non_empty(&self.game_directory)
    }

    pub fn data_dir(&self) -> Option<PathBuf> {
        non_empty(&self.data_directory)
    }

    pub fn custom_shortcut(&self) -> Option<PathBuf> {
        non_empty(&self.custom_shortcut_path)
    }

    /// Copies the 2.x custom shortcut once (only for the profile that was active in 2.x).
    pub fn import_legacy_shortcut(&mut self, legacy_profile: &str, shortcut: Option<&Path>) {
        if self.legacy_shortcut_imported {
            return;
        }
        self.legacy_shortcut_imported = true;
        if self.custom_shortcut_path.is_none() && legacy_profile.eq_ignore_ascii_case(&self.active_profile) {
            self.custom_shortcut_path = shortcut.map(|p| p.display().to_string());
        }
    }

    /// The game version releases are matched against, or `None` to accept any release.
    pub fn effective_target_version(&self, installed: Option<&str>) -> Option<String> {
        match self.target_game_version.as_deref().map(str::trim) {
            Some(t) if t.eq_ignore_ascii_case(TARGET_LATEST) => None,
            Some(t) if !t.is_empty() => Some(t.to_string()),
            _ => installed.map(str::to_string),
        }
    }

    pub fn skipped_version(&self, mod_id: &str) -> Option<&str> {
        self.skipped_mod_versions.get(&mod_id.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn is_update_excluded(&self, mod_id: &str) -> bool {
        self.update_exclusions.iter().any(|m| m.eq_ignore_ascii_case(mod_id))
    }

    pub fn is_favourite(&self, mod_id: &str) -> bool {
        self.favourite_mods.iter().any(|m| m.eq_ignore_ascii_case(mod_id))
    }

    /// Every profile name, the active one included, in order.
    pub fn profile_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.profiles.iter().map(|p| p.name.clone()).collect();
        if !names.iter().any(|n| n == &self.active_profile) {
            names.insert(0, self.active_profile.clone());
        }
        names
    }

    fn current_profile(&self) -> Profile {
        Profile {
            name: self.active_profile.clone(),
            game_directory: self.game_directory.clone(),
            data_directory: self.data_directory.clone(),
            target_game_version: self.target_game_version.clone(),
            custom_shortcut_path: self.custom_shortcut_path.clone(),
        }
    }

    fn store_current_profile(&mut self) {
        let current = self.current_profile();
        match self.profiles.iter_mut().find(|p| p.name == current.name) {
            Some(p) => *p = current,
            None => self.profiles.push(current),
        }
    }

    /// Switches profile (creating an empty one, i.e. auto-detected folders, if new).
    pub fn switch_profile(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the profile a name.".into());
        }
        self.store_current_profile();
        let target = self.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name)).cloned().unwrap_or(Profile { name: name.to_string(), ..Default::default() });
        if !self.profiles.iter().any(|p| p.name == target.name) {
            self.profiles.push(target.clone());
        }
        self.active_profile = target.name;
        self.game_directory = target.game_directory;
        self.data_directory = target.data_directory;
        self.target_game_version = target.target_game_version;
        self.custom_shortcut_path = target.custom_shortcut_path;
        Ok(())
    }

    pub fn rename_profile(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("Give the profile a name.".into());
        }
        self.store_current_profile();
        if self.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(new) && !p.name.eq_ignore_ascii_case(old)) {
            return Err(format!("There's already a profile called \"{new}\"."));
        }
        let p = self.profiles.iter_mut().find(|p| p.name == old).ok_or("That profile doesn't exist.")?;
        p.name = new.to_string();
        if self.active_profile == old {
            self.active_profile = new.to_string();
        }
        Ok(())
    }

    /// Deletes a profile that isn't active.
    pub fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        if self.active_profile == name {
            return Err("Switch to another profile before deleting this one.".into());
        }
        self.profiles.retain(|p| p.name != name);
        Ok(())
    }

    /// Copies the 2.x backup, theme and profile settings once.
    pub fn import_legacy_m5(&mut self, legacy: &crate::legacy_config::LegacyPreferences) {
        if self.legacy_m5_imported {
            return;
        }
        self.legacy_m5_imported = true;
        if let Some(v) = legacy.auto_data_backups {
            self.auto_data_backups = v;
        }
        if let Some(v) = legacy.max_data_backups {
            self.max_data_backups = v;
        }
        if self.data_backup_location.is_none() {
            self.data_backup_location = legacy.data_backup_location.clone();
        }
        if let Some(t) = &legacy.theme {
            self.theme = t.clone();
        }
        if !legacy.palette.is_empty() && self.custom_palette.is_empty() {
            self.custom_palette = legacy.palette.clone();
        }
        for p in &legacy.profiles {
            if p.name != self.active_profile && !self.profiles.iter().any(|x| x.name.eq_ignore_ascii_case(&p.name)) {
                self.profiles.push(p.clone());
            }
        }
    }

    /// Copies the 2.x preferences once. Folders are not copied; they are read live instead.
    pub fn import_legacy(&mut self, legacy: &crate::legacy_config::LegacyPreferences) {
        if self.legacy_preferences_imported {
            return;
        }
        self.legacy_preferences_imported = true;
        for (id, ver) in &legacy.skipped_mod_versions {
            self.skipped_mod_versions.entry(id.to_ascii_lowercase()).or_insert_with(|| ver.clone());
        }
        for id in &legacy.update_exclusions {
            set_flag(&mut self.update_exclusions, id, true);
        }
        if let Some(v) = legacy.require_exact_version_match {
            self.require_exact_version_match = v;
        }
        if let Some(v) = legacy.offline_mode {
            self.offline_mode = v;
        }
        if legacy.cache_all_versions == Some(false) {
            self.mod_cache_limit_mb = 0;
        }
        if self.target_game_version.is_none() {
            self.target_game_version = legacy.target_game_version.clone();
        }
    }
}

/// Adds or removes a case-insensitive id from a list. Stores ids lower-case.
pub fn set_flag(list: &mut Vec<String>, id: &str, on: bool) {
    list.retain(|m| !m.eq_ignore_ascii_case(id));
    if on {
        list.push(id.to_ascii_lowercase());
        list.sort();
    }
}

fn non_empty(v: &Option<String>) -> Option<PathBuf> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(t.path()).unwrap(), Settings::default());
        let s = Settings { game_directory: Some("C:\\VS".into()), close_after_launch: true, ..Default::default() };
        s.save(t.path()).unwrap();
        assert_eq!(Settings::load(t.path()).unwrap(), s);
    }

    #[test]
    fn tolerates_unknown_and_missing_fields() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(Settings::path(t.path()), r#"{"gameDirectory":"/g","futureThing":1}"#).unwrap();
        let s = Settings::load(t.path()).unwrap();
        assert_eq!(s.game_dir(), Some(PathBuf::from("/g")));
        assert!(!s.close_after_launch);
    }

    #[test]
    fn target_version_rules() {
        let mut s = Settings::default();
        assert_eq!(s.effective_target_version(Some("1.21.0")), Some("1.21.0".into()));
        s.target_game_version = Some("latest".into());
        assert_eq!(s.effective_target_version(Some("1.21.0")), None);
        s.target_game_version = Some("1.20.12".into());
        assert_eq!(s.effective_target_version(Some("1.21.0")), Some("1.20.12".into()));
    }

    #[test]
    fn imports_legacy_once() {
        let mut s = Settings::default();
        let legacy = crate::legacy_config::LegacyPreferences {
            skipped_mod_versions: [("CarryOn".to_string(), "1.2.0".to_string())].into_iter().collect(),
            update_exclusions: vec!["FooMod".into()],
            require_exact_version_match: Some(true),
            offline_mode: Some(false),
            cache_all_versions: Some(false),
            target_game_version: None,
            ..Default::default()
        };
        s.import_legacy(&legacy);
        assert_eq!(s.skipped_version("carryon"), Some("1.2.0"));
        assert!(s.is_update_excluded("foomod"));
        assert!(s.require_exact_version_match);
        assert_eq!(s.mod_cache_limit_mb, 0);
        s.update_exclusions.clear();
        s.import_legacy(&legacy);
        assert!(s.update_exclusions.is_empty(), "second import is a no-op");
    }

    #[test]
    fn imports_legacy_shortcut_once_for_the_active_profile() {
        let mut s = Settings::default();
        s.import_legacy_shortcut("Other", Some(Path::new("C:/vs.lnk")));
        assert_eq!(s.custom_shortcut_path, None);
        let mut s = Settings::default();
        s.import_legacy_shortcut("default", Some(Path::new("C:/vs.lnk")));
        assert_eq!(s.custom_shortcut_path.as_deref(), Some("C:/vs.lnk"));
        s.custom_shortcut_path = None;
        s.import_legacy_shortcut("default", Some(Path::new("C:/vs.lnk")));
        assert_eq!(s.custom_shortcut_path, None, "clearing it sticks");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(Settings::path(t.path()), "{").unwrap();
        assert!(Settings::load(t.path()).is_err());
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn switching_profiles_swaps_folders() {
        let mut s = Settings { data_directory: Some("A".into()), custom_shortcut_path: Some("A.lnk".into()), ..Default::default() };
        s.switch_profile("Modded").unwrap();
        assert_eq!((s.active_profile.as_str(), s.data_directory.as_deref()), ("Modded", None));
        assert_eq!(s.custom_shortcut_path, None);
        s.data_directory = Some("B".into());
        s.switch_profile("default").unwrap();
        assert_eq!(s.data_directory.as_deref(), Some("A"));
        assert_eq!(s.custom_shortcut_path.as_deref(), Some("A.lnk"));
        assert_eq!(s.profile_names(), ["Default", "Modded"]);
        s.rename_profile("Modded", "Heavy").unwrap();
        assert!(s.rename_profile("Heavy", "DEFAULT").is_err());
        assert!(s.delete_profile("Default").is_err());
        s.switch_profile("Heavy").unwrap();
        assert_eq!(s.data_directory.as_deref(), Some("B"));
        s.delete_profile("Default").unwrap();
        assert_eq!(s.profile_names(), ["Heavy"]);
    }
}
