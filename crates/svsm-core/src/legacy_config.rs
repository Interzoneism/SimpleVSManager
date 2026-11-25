//! Read-only access to the WPF app's `SimpleVSManagerConfiguration.json`.
//!
//! The new app never writes this file, so the WPF app keeps working side by side. Folders are
//! read live; update preferences (skipped versions, exclusions, ...) are imported once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{CoreError, CoreResult};

pub const CONFIGURATION_FILE_NAME: &str = "SimpleVSManagerConfiguration.json";
const DEFAULT_PROFILE: &str = "Default";

/// The folders the WPF app has stored for the active game profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyPaths {
    pub active_profile: String,
    pub data_directory: Option<PathBuf>,
    pub game_directory: Option<PathBuf>,
    pub custom_shortcut_path: Option<PathBuf>,
    pub preferences: LegacyPreferences,
}

/// Update-related preferences from 2.x, imported once into [`crate::Settings`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyPreferences {
    pub skipped_mod_versions: BTreeMap<String, String>,
    pub update_exclusions: Vec<String>,
    pub require_exact_version_match: Option<bool>,
    pub offline_mode: Option<bool>,
    pub cache_all_versions: Option<bool>,
    /// `Some("latest")` or a concrete version; `None` = installed version.
    pub target_game_version: Option<String>,
    pub auto_data_backups: Option<bool>,
    pub max_data_backups: Option<u32>,
    pub data_backup_location: Option<String>,
    /// `darkVs`, `dark`, `light` or `custom`.
    pub theme: Option<String>,
    /// CSS variable -> color, from 2.x `themePalette` / `customThemePalette`.
    pub palette: BTreeMap<String, String>,
    /// Profiles other than the active one (their folders and target version).
    pub profiles: Vec<crate::settings::Profile>,
}

/// `Palette.BaseSurface.Raised` -> `--palette-base-surface-raised`.
pub fn palette_var(key: &str) -> String {
    let mut out = String::from("--");
    for (i, part) in key.split('.').enumerate() {
        if i > 0 {
            out.push('-');
        }
        let mut prev_lower = false;
        for c in part.chars() {
            if c.is_ascii_uppercase() && prev_lower {
                out.push('-');
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// WPF `#AARRGGBB` / `#RRGGBB` -> CSS color.
pub fn css_color(argb: &str) -> Option<String> {
    let h = argb.trim().trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        6 => Some(format!("#{}", h.to_lowercase())),
        8 => {
            let a = u8::from_str_radix(&h[0..2], 16).ok()?;
            let rgb = &h[2..];
            if a == 255 {
                Some(format!("#{}", rgb.to_lowercase()))
            } else {
                let r = u8::from_str_radix(&rgb[0..2], 16).ok()?;
                let g = u8::from_str_radix(&rgb[2..4], 16).ok()?;
                let b = u8::from_str_radix(&rgb[4..6], 16).ok()?;
                Some(format!("rgba({r}, {g}, {b}, {:.3})", a as f64 / 255.0))
            }
        }
        _ => None,
    }
}

/// Loads the legacy config from `manager_dir`. `Ok(None)` if the file doesn't exist.
pub fn load(manager_dir: &Path) -> CoreResult<Option<LegacyPaths>> {
    let path = manager_dir.join(CONFIGURATION_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::io(&path, e)),
    };
    parse(&text)
        .map(Some)
        .map_err(|message| CoreError::Parse { path, message })
}

/// Parses the legacy config text. Mirrors `UserConfigurationService`: values live under
/// `gameProfiles.<active>`; older files kept them at the root.
pub fn parse(text: &str) -> Result<LegacyPaths, String> {
    let text = text.trim_start_matches('\u{feff}');
    let root: Value = serde_json::from_str(text)
        .or_else(|_| json5::from_str(text))
        .map_err(|e| e.to_string())?;

    let active = str_at(&root, "activeGameProfile").unwrap_or_else(|| DEFAULT_PROFILE.to_string());

    let profiles = root.get("gameProfiles").and_then(Value::as_object);
    let profile = profiles.and_then(|p| {
        p.get(&active)
            .or_else(|| p.iter().find(|(k, _)| k.eq_ignore_ascii_case(&active)).map(|(_, v)| v))
            .or_else(|| p.get(DEFAULT_PROFILE))
    });

    let pick = |key: &str| -> Option<PathBuf> {
        profile
            .and_then(|p| str_at(p, key))
            .or_else(|| str_at(&root, key))
            .map(PathBuf::from)
    };

    let pick_obj = |key: &str| -> Option<&serde_json::Map<String, Value>> {
        profile
            .and_then(|p| p.get(key))
            .and_then(Value::as_object)
            .filter(|o| !o.is_empty())
            .or_else(|| root.get(key).and_then(Value::as_object))
    };
    let flag = |key: &str| root.get(key).and_then(Value::as_bool);

    let mut preferences = LegacyPreferences {
        require_exact_version_match: flag("requireExactVsVersionMatch"),
        offline_mode: flag("disableInternetAccess"),
        cache_all_versions: flag("cacheAllVersionsLocally"),
        target_game_version: profile
            .and_then(|p| str_at(p, "targetGameVersionOverride"))
            .or_else(|| str_at(&root, "targetGameVersionOverride"))
            .map(|t| if t == "__target_latest__" { "latest".to_string() } else { t }),
        ..Default::default()
    };
    if let Some(skips) = pick_obj("skippedModVersions") {
        for (id, v) in skips {
            if let Some(v) = v.as_str().map(str::trim).filter(|v| !v.is_empty()) {
                if !id.trim().is_empty() {
                    preferences.skipped_mod_versions.insert(id.trim().to_ascii_lowercase(), v.to_string());
                }
            }
        }
    }
    preferences.auto_data_backups = flag("automaticDataBackupsEnabled");
    preferences.max_data_backups = root.get("maximumDataBackups").and_then(Value::as_u64).map(|v| v.min(u32::MAX as u64) as u32);
    preferences.data_backup_location = str_at(&root, "customDataBackupLocation");
    preferences.theme = str_at(&root, "colorTheme").map(|t| match t.to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "dark" => "dark".to_string(),
        "custom" => "custom".to_string(),
        _ => "darkVs".to_string(),
    });
    let palette_src = if preferences.theme.as_deref() == Some("custom") {
        root.get("customThemePalette").or_else(|| root.get("themePalette"))
    } else {
        root.get("themePalette")
    };
    if let Some(map) = palette_src.and_then(Value::as_object) {
        for (k, v) in map {
            if let Some(c) = v.as_str().and_then(css_color) {
                preferences.palette.insert(palette_var(k), c);
            }
        }
    }
    if let Some(all) = profiles {
        for (name, p) in all {
            if name.eq_ignore_ascii_case(&active) {
                continue;
            }
            preferences.profiles.push(crate::settings::Profile {
                name: name.clone(),
                game_directory: str_at(p, "gameDirectory"),
                data_directory: str_at(p, "dataDirectory"),
                target_game_version: str_at(p, "targetGameVersionOverride").map(|t| if t == "__target_latest__" { "latest".to_string() } else { t }),
                custom_shortcut_path: str_at(p, "customShortcutPath"),
            });
        }
    }
    if let Some(ex) = pick_obj("bulkUpdateModExclusions") {
        for (id, v) in ex {
            if v.as_bool() == Some(true) && !id.trim().is_empty() {
                preferences.update_exclusions.push(id.trim().to_ascii_lowercase());
            }
        }
    }

    Ok(LegacyPaths {
        preferences,
        data_directory: pick("dataDirectory"),
        game_directory: pick("gameDirectory"),
        custom_shortcut_path: pick("customShortcutPath"),
        active_profile: active,
    })
}

fn str_at(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_active_profile() {
        let json = r#"{
            "activeGameProfile": "Modded",
            "gameProfiles": {
                "Default": { "dataDirectory": "C:\\VS\\Data", "gameDirectory": "C:\\VS\\Game" },
                "Modded":  { "dataDirectory": "D:\\Modded\\Data" }
            }
        }"#;
        let p = parse(json).unwrap();
        assert_eq!(p.active_profile, "Modded");
        assert_eq!(p.data_directory, Some(PathBuf::from("D:\\Modded\\Data")));
        assert_eq!(p.game_directory, None);
    }

    #[test]
    fn reads_legacy_root_values() {
        let p = parse(r#"{ "dataDirectory": "/home/me/.config/VintagestoryData", "gameDirectory": "" }"#).unwrap();
        assert_eq!(p.active_profile, "Default");
        assert_eq!(p.data_directory, Some(PathBuf::from("/home/me/.config/VintagestoryData")));
        assert_eq!(p.game_directory, None);
    }

    #[test]
    fn tolerates_bom_and_trailing_commas() {
        let p = parse("\u{feff}{ \"dataDirectory\": \"x\", }").unwrap();
        assert_eq!(p.data_directory, Some(PathBuf::from("x")));
    }

    #[test]
    fn reads_update_preferences() {
        let json = r#"{
            "activeGameProfile": "Default",
            "requireExactVsVersionMatch": true,
            "cacheAllVersionsLocally": false,
            "gameProfiles": { "Default": {
                "skippedModVersions": { "CarryOn": "1.10.0", "bad": "" },
                "bulkUpdateModExclusions": { "Foo": true, "Bar": false },
                "targetGameVersionOverride": "__target_latest__"
            } }
        }"#;
        let p = parse(json).unwrap().preferences;
        assert_eq!(p.skipped_mod_versions.get("carryon").map(String::as_str), Some("1.10.0"));
        assert_eq!(p.skipped_mod_versions.len(), 1);
        assert_eq!(p.update_exclusions, vec!["foo".to_string()]);
        assert_eq!(p.require_exact_version_match, Some(true));
        assert_eq!(p.cache_all_versions, Some(false));
        assert_eq!(p.offline_mode, None);
        assert_eq!(p.target_game_version.as_deref(), Some("latest"));
    }

    #[test]
    fn reads_m5_values_and_palettes() {
        let json = r##"{
            "activeGameProfile": "Default", "automaticDataBackupsEnabled": true, "maximumDataBackups": 5,
            "colorTheme": "VintageStory", "themePalette": { "Palette.BaseSurface.Raised": "#FF4D3D2D", "Palette.Overlay.HoverTint": "#10FFFFFF" },
            "gameProfiles": { "Default": {}, "Modded": { "dataDirectory": "D:\\M", "targetGameVersionOverride": "__target_latest__" } }
        }"##;
        let p = parse(json).unwrap().preferences;
        assert_eq!((p.auto_data_backups, p.max_data_backups), (Some(true), Some(5)));
        assert_eq!(p.theme.as_deref(), Some("darkVs"));
        assert_eq!(p.palette["--palette-base-surface-raised"], "#4d3d2d");
        assert_eq!(p.palette["--palette-overlay-hover-tint"], "rgba(255, 255, 255, 0.063)");
        assert_eq!(p.profiles.len(), 1);
        assert_eq!(p.profiles[0].target_game_version.as_deref(), Some("latest"));
        assert_eq!(palette_var("Palette.Text.Primary"), "--palette-text-primary");
        assert_eq!(css_color("nope"), None);
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).unwrap().is_none());
    }
}
