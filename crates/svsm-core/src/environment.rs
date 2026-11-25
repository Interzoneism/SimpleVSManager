//! One-shot detection of everything the UI shows on the start screen.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::host::HostEnv;
use crate::platform::Os;
use crate::settings::Settings;
use crate::{game_version, legacy_config, paths};

/// How a path was found. Shown in the UI so users can tell guesses from their own choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PathSource {
    /// Chosen by the user in this app.
    UserSetting,
    /// Taken from the WPF app's configuration file.
    LegacyConfig,
    /// `CustomConfigFolder.txt` next to the executable.
    CustomPointer,
    /// Found by searching the usual locations.
    Detected,
    /// Nothing found; this is where it would normally be.
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DetectedPath {
    pub path: String,
    pub source: PathSource,
    pub exists: bool,
}

impl DetectedPath {
    fn new(path: &Path, source: PathSource) -> Self {
        Self { path: path.display().to_string(), source, exists: path.exists() }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DetectedEnvironment {
    pub os: Os,
    pub settings: Settings,
    pub manager_dir: DetectedPath,
    pub data_dir: Option<DetectedPath>,
    pub game_dir: Option<DetectedPath>,
    pub game_executable: Option<String>,
    pub game_version: Option<String>,
    /// Name of the active profile in the WPF config, if one was found.
    pub legacy_profile: Option<String>,
    /// Non-fatal problems worth showing (unreadable legacy config, stale paths, ...).
    pub warnings: Vec<String>,
}

/// Detects the manager, data and game folders plus the game version.
///
/// Priority for data and game folders: this app's settings, then the WPF app's saved choice
/// (if that folder still exists), then the usual locations.
pub fn detect_environment(host: &HostEnv) -> DetectedEnvironment {
    let (manager_dir, _) = paths::resolve_manager_dir(host);
    let mut warnings = Vec::new();
    let settings = match Settings::load(&manager_dir) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("Could not read the settings file, using defaults: {e}"));
            Settings::default()
        }
    };
    detect_environment_with(host, &settings, warnings)
}

/// Same as [`detect_environment`] with settings already loaded.
pub fn detect_environment_with(host: &HostEnv, settings: &Settings, mut warnings: Vec<String>) -> DetectedEnvironment {
    let (manager_dir, manager_kind) = paths::resolve_manager_dir(host);
    let manager_source = match manager_kind {
        paths::ManagerDirKind::Custom => PathSource::CustomPointer,
        paths::ManagerDirKind::Default => PathSource::Default,
    };

    let legacy = match legacy_config::load(&manager_dir) {
        Ok(l) => l,
        Err(e) => {
            warnings.push(format!("Could not read the settings of your earlier Simple VS Manager version: {e}"));
            None
        }
    };
    // 2.x folders only stand in for the profile that was active in 2.x.
    let legacy_folders = legacy.as_ref().filter(|l| l.active_profile.eq_ignore_ascii_case(&settings.active_profile));

    // Data folder.
    let user_data = settings.data_dir();
    let legacy_data = legacy_folders.and_then(|l| l.data_directory.clone());
    let data_dir = if let Some(p) = user_data.as_ref().filter(|p| p.is_dir()) {
        Some(DetectedPath::new(p, PathSource::UserSetting))
    } else {
        if let Some(p) = &user_data {
            warnings.push(format!("The data folder you chose no longer exists: {}", p.display()));
        }
        match legacy_data {
        Some(p) if p.is_dir() => Some(DetectedPath::new(&p, PathSource::LegacyConfig)),
        other => {
            if let Some(p) = other {
                warnings.push(format!(
                    "The data folder from your earlier Simple VS Manager settings no longer exists: {}",
                    p.display()
                ));
            }
            paths::resolve_data_dir(host).map(|(p, found)| {
                DetectedPath::new(&p, if found { PathSource::Detected } else { PathSource::Default })
            })
        }
    }
    };

    // Game folder.
    let user_game = settings.game_dir();
    let user_game_found = user_game.as_ref().and_then(|p| paths::find_game_executable(host.os, p).map(|e| (p.clone(), e)));
    if let (Some(p), None) = (&user_game, &user_game_found) {
        warnings.push(format!("No Vintage Story executable in the game folder you chose: {}", p.display()));
    }
    let legacy_game = legacy_folders.and_then(|l| l.game_directory.clone());
    let (game_dir, game_executable): (Option<DetectedPath>, Option<PathBuf>) = if let Some((dir, exe)) = user_game_found {
        (Some(DetectedPath::new(&dir, PathSource::UserSetting)), Some(exe))
    } else {
        match legacy_game.as_ref().and_then(|p| paths::find_game_executable(host.os, p).map(|e| (p, e))) {
            Some((dir, exe)) => (Some(DetectedPath::new(dir, PathSource::LegacyConfig)), Some(exe)),
            None => {
                if let Some(p) = &legacy_game {
                    warnings.push(format!(
                        "No Vintage Story executable in the game folder from your earlier Simple VS Manager settings: {}",
                        p.display()
                    ));
                }
                match paths::resolve_game_dir(host) {
                    Some((dir, exe)) => (Some(DetectedPath::new(&dir, PathSource::Detected)), Some(exe)),
                    None => (None, None),
                }
            }
        }
    };

    let game_version = game_dir
        .as_ref()
        .and_then(|g| game_version::detect_game_version(Path::new(&g.path)));

    DetectedEnvironment {
        os: host.os,
        settings: {
            let mut s = settings.clone();
            if let Some(l) = &legacy {
                s.import_legacy(&l.preferences);
                s.import_legacy_m5(&l.preferences);
            }
            s
        },
        manager_dir: DetectedPath::new(&manager_dir, manager_source),
        data_dir,
        game_dir,
        game_executable: game_executable.map(|e| e.display().to_string()),
        game_version,
        legacy_profile: legacy.map(|l| l.active_profile),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::tests::fake_host;

    #[test]
    fn empty_machine() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let env = detect_environment(&host);
        assert_eq!(env.manager_dir.source, PathSource::Default);
        assert_eq!(env.data_dir.as_ref().unwrap().source, PathSource::Default);
        assert!(!env.data_dir.as_ref().unwrap().exists);
        assert!(env.game_dir.is_none());
        assert!(env.game_version.is_none());
        assert!(env.warnings.is_empty());
    }

    #[test]
    fn legacy_config_wins_when_valid() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let manager = paths::default_manager_dir(&host);
        let data = t.path().join("elsewhere/data");
        let game = t.path().join("elsewhere/game");
        std::fs::create_dir_all(&manager).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Vintagestory"), b"").unwrap();
        let cfg = serde_json::json!({
            "activeGameProfile": "Default",
            "gameProfiles": { "Default": { "dataDirectory": data, "gameDirectory": game } }
        });
        std::fs::write(manager.join(legacy_config::CONFIGURATION_FILE_NAME), cfg.to_string()).unwrap();

        let env = detect_environment(&host);
        assert_eq!(env.data_dir.unwrap().source, PathSource::LegacyConfig);
        assert_eq!(env.game_dir.unwrap().source, PathSource::LegacyConfig);
        assert_eq!(env.legacy_profile.as_deref(), Some("Default"));
    }

    #[test]
    fn user_setting_beats_legacy() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let manager = paths::default_manager_dir(&host);
        let mine = t.path().join("mine");
        let theirs = t.path().join("theirs");
        for d in [&manager, &mine, &theirs] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(mine.join("Vintagestory"), b"").unwrap();
        std::fs::write(
            manager.join(legacy_config::CONFIGURATION_FILE_NAME),
            serde_json::json!({ "dataDirectory": theirs, "gameDirectory": theirs }).to_string(),
        )
        .unwrap();
        let settings = Settings {
            data_directory: Some(mine.display().to_string()),
            game_directory: Some(mine.display().to_string()),
            ..Default::default()
        };
        settings.save(&manager).unwrap();

        let env = detect_environment(&host);
        assert_eq!(env.data_dir.unwrap().source, PathSource::UserSetting);
        assert_eq!(env.game_dir.unwrap().source, PathSource::UserSetting);
        // The 2.x preferences were imported (in memory) alongside.
        assert_eq!(env.settings, Settings { legacy_preferences_imported: true, legacy_m5_imported: true, ..settings });
    }

    #[test]
    fn stale_legacy_paths_warn_and_fall_back() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let manager = paths::default_manager_dir(&host);
        std::fs::create_dir_all(&manager).unwrap();
        std::fs::write(
            manager.join(legacy_config::CONFIGURATION_FILE_NAME),
            r#"{ "dataDirectory": "/gone/data", "gameDirectory": "/gone/game" }"#,
        )
        .unwrap();
        let env = detect_environment(&host);
        assert_eq!(env.warnings.len(), 2);
        assert_eq!(env.data_dir.unwrap().source, PathSource::Default);
    }
}
