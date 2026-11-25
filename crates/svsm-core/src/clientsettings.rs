//! Read/write access to the game's `VintagestoryData/clientsettings.json`.
//!
//! Port of `ClientSettingsStore.cs`, with one deliberate difference: we never rewrite the file
//! just because we opened it. The WPF app "fixed up" `modPaths` on load; here the defaults are
//! applied in memory only, and the file is written only when the user changes something.
//!
//! The file belongs to the game, so writes keep key order and every key we don't know about.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{CoreError, CoreResult};

pub const FILE_NAME: &str = "clientsettings.json";
const TEMP_NAME: &str = "clientsettings.tmp";
const BACKUP_NAME: &str = "clientsettings.bkp";

#[derive(Debug, Clone)]
pub struct ClientSettings {
    data_dir: PathBuf,
    root: Value,
    disabled: Vec<String>,
    mod_paths: Vec<String>,
    /// False when the file didn't exist or couldn't be parsed.
    loaded_from_disk: bool,
}

impl ClientSettings {
    /// Loads the settings for a data folder. A missing or corrupt file yields defaults (the
    /// game does the same); a corrupt file is never overwritten unless the user changes state.
    pub fn load(data_dir: &Path) -> CoreResult<Self> {
        let path = data_dir.join(FILE_NAME);
        let (root, loaded) = match std::fs::read_to_string(&path) {
            Ok(text) => match parse_root(&text) {
                Some(v) => (v, true),
                None => (default_root(), false),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (default_root(), false),
            Err(e) => return Err(CoreError::io(&path, e)),
        };

        let disabled = string_list(&root, "disabledMods");
        let mod_paths = string_list(&root, "modPaths");
        Ok(Self { data_dir: data_dir.to_path_buf(), root, disabled, mod_paths, loaded_from_disk: loaded })
    }

    pub fn path(&self) -> PathBuf {
        self.data_dir.join(FILE_NAME)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn loaded_from_disk(&self) -> bool {
        self.loaded_from_disk
    }

    pub fn disabled_entries(&self) -> &[String] {
        &self.disabled
    }

    /// `modPaths` as stored, with the game's defaults applied when the list is empty.
    pub fn mod_paths(&self) -> Vec<String> {
        if self.mod_paths.is_empty() {
            vec!["Mods".to_string(), self.data_dir.join("Mods").display().to_string()]
        } else {
            self.mod_paths.clone()
        }
    }

    pub fn player_name(&self) -> Option<String> {
        self.string_setting("playername")
    }

    pub fn player_uid(&self) -> Option<String> {
        self.string_setting("playeruid")
    }

    /// A mod is disabled if `modid` or `modid@version` is listed (case-insensitive).
    pub fn is_disabled(&self, mod_id: &str, version: Option<&str>) -> bool {
        let mod_id = mod_id.trim();
        if mod_id.is_empty() {
            return false;
        }
        let versioned = version.map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("{mod_id}@{v}"));
        self.disabled.iter().any(|e| {
            e.eq_ignore_ascii_case(mod_id) || versioned.as_deref().is_some_and(|k| e.eq_ignore_ascii_case(k))
        })
    }

    /// Enables or disables a mod in memory. Returns whether anything changed; call
    /// [`ClientSettings::save`] afterwards.
    ///
    /// Disabling adds `modid@version` (or `modid` when the version is unknown), like the WPF app
    /// and the game. Enabling removes both forms.
    pub fn set_active(&mut self, mod_id: &str, version: Option<&str>, active: bool) -> bool {
        let mod_id = mod_id.trim();
        if mod_id.is_empty() {
            return false;
        }
        let versioned = version.map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("{mod_id}@{v}"));

        if active {
            let before = self.disabled.len();
            self.disabled.retain(|e| {
                !(e.eq_ignore_ascii_case(mod_id) || versioned.as_deref().is_some_and(|k| e.eq_ignore_ascii_case(k)))
            });
            before != self.disabled.len()
        } else {
            let key = versioned.unwrap_or_else(|| mod_id.to_string());
            if self.disabled.iter().any(|e| e.eq_ignore_ascii_case(&key)) {
                false
            } else {
                self.disabled.push(key);
                true
            }
        }
    }

    /// Replaces the whole `disabledMods` list (old presets that store it as is). Returns whether
    /// anything changed; call [`ClientSettings::save`] afterwards.
    pub fn replace_disabled(&mut self, entries: Vec<String>) -> bool {
        if self.disabled == entries {
            return false;
        }
        self.disabled = entries;
        true
    }

    /// Writes the file atomically: write `.tmp`, copy the current file to `.bkp`, then rename
    /// `.tmp` over the original. Only `disabledMods` is updated; everything else is kept as is.
    pub fn save(&mut self) -> CoreResult<()> {
        let list = Value::Array(self.disabled.iter().cloned().map(Value::String).collect());
        let string_lists = object_mut_ci(&mut self.root, "stringListSettings");
        set_ci(string_lists, "disabledMods", list);

        let text = serde_json::to_string_pretty(&self.root).expect("serializing a Value cannot fail");
        let path = self.path();
        let tmp = self.data_dir.join(TEMP_NAME);
        let bkp = self.data_dir.join(BACKUP_NAME);

        std::fs::create_dir_all(&self.data_dir).map_err(|e| CoreError::io(&self.data_dir, e))?;
        crate::fsutil::write_synced(&tmp, text.as_bytes())?;
        if path.exists() {
            std::fs::copy(&path, &bkp).map_err(|e| CoreError::io(&bkp, e))?;
        }
        crate::fsutil::rename_with_retry(&tmp, &path)?;
        self.loaded_from_disk = true;
        Ok(())
    }

    fn string_setting(&self, key: &str) -> Option<String> {
        get_ci(&self.root, "stringSettings")
            .and_then(|s| get_ci(s, key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    }
}

fn parse_root(text: &str) -> Option<Value> {
    let text = text.trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(text).or_else(|_| json5::from_str(text)).ok()?;
    v.is_object().then_some(v)
}

fn default_root() -> Value {
    serde_json::json!({
        "boolSettings": {},
        "intSettings": {},
        "floatSettings": {},
        "stringSettings": {},
        "stringListSettings": { "disabledMods": [], "modPaths": [] }
    })
}

fn string_list(root: &Value, key: &str) -> Vec<String> {
    get_ci(root, "stringListSettings")
        .and_then(|s| get_ci(s, key))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

/// Case-insensitive property lookup (the game's JSON reader is case-insensitive).
fn get_ci<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    let obj = v.as_object()?;
    obj.get(key).or_else(|| obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
}

fn object_mut_ci<'a>(v: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    let obj = v.as_object_mut().expect("root is an object");
    let actual = obj.keys().find(|k| k.eq_ignore_ascii_case(key)).cloned().unwrap_or_else(|| key.to_string());
    let entry = obj.entry(actual).or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("just ensured object")
}

fn set_ci(obj: &mut Map<String, Value>, key: &str, value: Value) {
    let actual = obj.keys().find(|k| k.eq_ignore_ascii_case(key)).cloned().unwrap_or_else(|| key.to_string());
    obj.insert(actual, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "boolSettings": { "fullscreen": true },
  "stringSettings": { "playername": "Martin", "playeruid": "abc" },
  "stringListSettings": {
    "modPaths": [ "Mods", "C:\\Users\\M\\AppData\\Roaming\\VintagestoryData\\Mods" ],
    "disabledMods": [ "betterruins@0.4.1", "carryon" ],
    "someOtherList": [ "x" ]
  },
  "zUnknownTopLevel": 42
}"#;

    fn sample_dir() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(FILE_NAME), SAMPLE).unwrap();
        t
    }

    #[test]
    fn reads_values() {
        let t = sample_dir();
        let s = ClientSettings::load(t.path()).unwrap();
        assert!(s.loaded_from_disk());
        assert_eq!(s.player_name().as_deref(), Some("Martin"));
        assert_eq!(s.mod_paths().len(), 2);
        assert!(s.is_disabled("BetterRuins", Some("0.4.1")));
        assert!(!s.is_disabled("betterruins", Some("0.5.0")));
        assert!(s.is_disabled("carryon", Some("1.0")));
        assert!(s.is_disabled("carryon", None));
    }

    #[test]
    fn toggling_and_saving_preserves_other_keys() {
        let t = sample_dir();
        let mut s = ClientSettings::load(t.path()).unwrap();
        assert!(s.set_active("carryon", Some("1.0"), true));
        assert!(s.set_active("newmod", Some("2.0"), false));
        assert!(!s.set_active("newmod", Some("2.0"), false), "no duplicate entries");
        s.save().unwrap();

        let text = std::fs::read_to_string(t.path().join(FILE_NAME)).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["zUnknownTopLevel"], 42);
        assert_eq!(v["boolSettings"]["fullscreen"], true);
        assert_eq!(v["stringListSettings"]["someOtherList"][0], "x");
        assert_eq!(v["stringListSettings"]["disabledMods"], serde_json::json!(["betterruins@0.4.1", "newmod@2.0"]));
        // Key order kept: boolSettings still first, unknown key still last.
        assert!(text.find("boolSettings").unwrap() < text.find("zUnknownTopLevel").unwrap());
        // Backup of the previous file.
        let bkp = std::fs::read_to_string(t.path().join(BACKUP_NAME)).unwrap();
        assert_eq!(bkp, SAMPLE);
        assert!(!t.path().join(TEMP_NAME).exists());
    }

    #[test]
    fn missing_file_uses_defaults_without_writing() {
        let t = tempfile::tempdir().unwrap();
        let s = ClientSettings::load(t.path()).unwrap();
        assert!(!s.loaded_from_disk());
        assert_eq!(s.mod_paths()[0], "Mods");
        assert!(!t.path().join(FILE_NAME).exists());
    }

    #[test]
    fn corrupt_file_is_not_touched_on_load() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(FILE_NAME), "{ not json").unwrap();
        let s = ClientSettings::load(t.path()).unwrap();
        assert!(!s.loaded_from_disk());
        assert_eq!(std::fs::read_to_string(t.path().join(FILE_NAME)).unwrap(), "{ not json");
    }

    #[test]
    fn enabling_removes_both_forms() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join(FILE_NAME),
            r#"{"stringListSettings":{"disabledMods":["a","a@1.0","b@1.0"]}}"#,
        )
        .unwrap();
        let mut s = ClientSettings::load(t.path()).unwrap();
        assert!(s.set_active("A", Some("1.0"), true));
        assert_eq!(s.disabled_entries(), ["b@1.0"]);
    }
}
