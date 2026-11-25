//! The one modlist file format, shared by modlists, presets and automatic backups.
//!
//! This is Simple VS Manager 2.3.0's `SerializablePreset`: camelCase JSON, null fields left out.
//! A preset is a modlist without metadata (name, description, ...) and without versions; a
//! backup is a modlist the app saved by itself. The same JSON is embedded in PDF exports.
//!
//! Reading is lenient like 2.x (case-insensitive keys, BOM, comments and trailing commas), and
//! per-mod config content compressed by 2.x (`br64:` + base64 Brotli) is decoded on load.

use std::io::{Read, Write};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Modlist {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The modlist's own version (set by its author), not a game version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    /// Old presets only: `disabledMods` entries to put into clientsettings.json as they are.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_entries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mods: Option<Vec<ModlistMod>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configurations: Option<Vec<ModlistConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_mod_status: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_mod_versions: Option<bool>,
    /// Loading it should remove mods that aren't in the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive: Option<bool>,
    /// This file is a local copy of an online modlist (SVSM-Dev format). Never published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_subscription: Option<OnlineSubscription>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OnlineSubscription {
    pub registry_id: Option<String>,
    pub synced_revision: f64,
    pub last_known_remote_revision: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribed_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at_utc: Option<String>,
}

impl OnlineSubscription {
    pub fn update_available(&self) -> bool {
        self.last_known_remote_revision > self.synced_revision
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModlistMod {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    /// 2.x stored one config file inline per mod.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration_file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration_content: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModlistConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// Path under `VintagestoryData/ModConfig`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// `{"configurations": [...]}`: the config list stored separately in PDF exports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigList {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configurations: Option<Vec<ModlistConfig>>,
}

/// One config file to write into `ModConfig`, after merging the inline and list forms.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigFile {
    pub mod_id: String,
    pub file_name: String,
    pub relative_path: Option<String>,
    pub content: String,
}

const BROTLI_PREFIX: &str = "br64:";

/// Parses modlist JSON. Errors are short messages meant for the user.
pub fn parse_modlist(text: &str) -> Result<Modlist, String> {
    let value = parse_lenient(text)?;
    if !value.is_object() {
        return Err("The file is not a modlist.".into());
    }
    let mut list: Modlist = serde_json::from_value(value).map_err(|e| format!("The modlist is damaged: {e}"))?;
    for m in list.mods.iter_mut().flatten() {
        if let Some(c) = m.configuration_content.as_mut() {
            *c = decode_config(c);
        }
    }
    Ok(list)
}

pub fn parse_config_list(text: &str) -> Result<ConfigList, String> {
    serde_json::from_value(parse_lenient(text)?).map_err(|e| format!("The config list is damaged: {e}"))
}

fn parse_lenient(text: &str) -> Result<Value, String> {
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return Err("The file is empty.".into());
    }
    let v: Value = serde_json::from_str(text)
        .or_else(|_| json5::from_str(text))
        .map_err(|e| format!("The file is not valid JSON: {e}"))?;
    Ok(camel_keys(v))
}

/// 2.x read keys case-insensitively. Our fields are camelCase, so lower-case the first letter.
fn camel_keys(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let mut chars = k.chars();
                    let key = match chars.next() {
                        Some(first) => first.to_lowercase().chain(chars).collect(),
                        None => k,
                    };
                    (key, camel_keys(v))
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(camel_keys).collect()),
        other => other,
    }
}

/// Serializes like 2.x: indented camelCase JSON, nulls left out, inline configs compressed.
pub fn to_json(list: &Modlist) -> String {
    let mut out = list.clone();
    for m in out.mods.iter_mut().flatten() {
        if let Some(c) = m.configuration_content.as_mut() {
            *c = encode_config(c);
        }
    }
    serde_json::to_string_pretty(&out).expect("modlists always serialize")
}

/// Compact JSON for PDF metadata (2.x didn't indent there).
pub fn to_compact_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("modlists always serialize")
}

/// Decodes `br64:` Brotli content; anything else (or a broken payload) is returned as is.
pub fn decode_config(content: &str) -> String {
    let Some(b64) = content.strip_prefix(BROTLI_PREFIX) else {
        return content.to_string();
    };
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
        return content.to_string();
    };
    let mut out = Vec::new();
    match brotli::Decompressor::new(raw.as_slice(), 4096).read_to_end(&mut out) {
        Ok(_) => String::from_utf8(out).unwrap_or_else(|_| content.to_string()),
        Err(_) => content.to_string(),
    }
}

/// Compresses config content the way 2.x did, but only when that makes it smaller.
pub fn encode_config(content: &str) -> String {
    let plain = decode_config(content);
    if plain.is_empty() {
        return plain;
    }
    let mut compressed = Vec::new();
    {
        let mut w = brotli::CompressorWriter::new(&mut compressed, 4096, 11, 22);
        w.write_all(plain.as_bytes()).expect("writing to a Vec cannot fail");
    }
    if compressed.len() + BROTLI_PREFIX.len() < plain.len() {
        format!("{BROTLI_PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(compressed))
    } else {
        plain
    }
}

fn trimmed(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl Modlist {
    pub fn display_name(&self) -> Option<&str> {
        trimmed(&self.name)
    }

    /// Mods with a usable id, in file order, first entry per id (case-insensitive).
    pub fn entries(&self) -> Vec<&ModlistMod> {
        let mut seen = std::collections::HashSet::new();
        self.mods
            .iter()
            .flatten()
            .filter(|m| m.id().is_some_and(|id| seen.insert(id.to_ascii_lowercase())))
            .collect()
    }

    /// Whether the list records on/off state (2.x: `includeModStatus`, or any `isActive`).
    pub fn has_status(&self) -> bool {
        self.include_mod_status.unwrap_or_else(|| self.mods.iter().flatten().any(|m| m.is_active.is_some()))
    }

    /// Whether the list records versions (2.x: `includeModVersions`, or any version).
    pub fn has_versions(&self) -> bool {
        self.include_mod_versions.unwrap_or_else(|| self.mods.iter().flatten().any(|m| trimmed(&m.version).is_some()))
    }

    /// Every config file in the list. For a mod that appears in `configurations`, those win
    /// over the inline field (as in 2.x). Duplicates are dropped.
    pub fn config_files(&self) -> Vec<ConfigFile> {
        let mut out: Vec<ConfigFile> = Vec::new();
        let mut push = |f: ConfigFile| {
            if !f.content.is_empty() && !out.iter().any(|o| o == &f) {
                out.push(f);
            }
        };
        let listed: Vec<&ModlistConfig> = self.configurations.iter().flatten().filter(|c| trimmed(&c.mod_id).is_some()).collect();
        for m in self.entries() {
            let id = m.id().expect("entries have ids");
            let from_list: Vec<_> = listed.iter().filter(|c| trimmed(&c.mod_id).is_some_and(|c| c.eq_ignore_ascii_case(id))).collect();
            if from_list.is_empty() {
                if let Some(content) = m.configuration_content.as_deref() {
                    push(ConfigFile {
                        mod_id: id.to_string(),
                        file_name: safe_config_file_name(m.configuration_file_name.as_deref(), id),
                        relative_path: None,
                        content: decode_config(content),
                    });
                }
            }
        }
        // Configs listed separately (also those for mods not in the list).
        for c in listed {
            let id = trimmed(&c.mod_id).expect("filtered");
            push(ConfigFile {
                mod_id: id.to_string(),
                file_name: safe_config_file_name(c.file_name.as_deref(), id),
                relative_path: trimmed(&c.relative_path).map(String::from),
                content: c.content.clone().unwrap_or_default(),
            });
        }
        out
    }

    /// Adds the PDF's separate config list, skipping configs already in the list.
    pub fn merge_config_list(&mut self, list: ConfigList) {
        let existing = self.configurations.get_or_insert_with(Vec::new);
        for c in list.configurations.into_iter().flatten() {
            if trimmed(&c.mod_id).is_none() || c.content.is_none() {
                continue;
            }
            if !existing.iter().any(|e| e == &c) {
                existing.push(c);
            }
        }
        if existing.is_empty() {
            self.configurations = None;
        }
    }
}

impl ModlistMod {
    pub fn id(&self) -> Option<&str> {
        trimmed(&self.mod_id)
    }

    pub fn version(&self) -> Option<&str> {
        trimmed(&self.version)
    }
}

/// Port of 2.x `GetSafeConfigFileName`: keeps .json/.yaml/.yml, otherwise `.json`.
pub fn safe_config_file_name(candidate: Option<&str>, mod_id: &str) -> String {
    let fallback = crate::moddb::cache::sanitize_file_name(mod_id, "modconfig");
    let candidate = candidate.map(str::trim).filter(|c| !c.is_empty());
    let file = candidate.and_then(|c| c.rsplit(['/', '\\']).next()).unwrap_or("");
    let (stem, ext) = match file.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (file, String::new()),
    };
    let ext = if [".json", ".yaml", ".yml"].iter().any(|e| e.eq_ignore_ascii_case(&ext)) { ext } else { ".json".to_string() };
    let stem = crate::moddb::cache::sanitize_file_name(stem, "");
    let stem = if stem.is_empty() { fallback } else { stem };
    format!("{stem}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/modlists/backup-2.3.0.json");

    #[test]
    fn reads_a_2_3_0_backup() {
        let l = parse_modlist(FIXTURE).unwrap();
        assert_eq!(l.entries().len(), 5);
        assert!(l.has_status() && l.has_versions());
        assert_eq!(l.exclusive, Some(true));
        let configs = l.config_files();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].mod_id, "showcraftable");
        assert_eq!(configs[0].file_name, "ShowCraftable.json");
        assert!(configs[0].content.contains("\"EnableFetchButton\": true"));
    }

    #[test]
    fn round_trips_and_matches_2x_shape() {
        let l = parse_modlist(FIXTURE).unwrap();
        let json = to_json(&l);
        assert_eq!(parse_modlist(&json).unwrap(), l);
        let v: Value = serde_json::from_str(&json).unwrap();
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["name", "mods", "includeModStatus", "includeModVersions", "exclusive"]);
        assert!(v["mods"][0].get("configurationContent").is_none());
    }

    #[test]
    fn lenient_keys_and_syntax() {
        let l = parse_modlist("\u{feff}{ Name: 'x', Mods: [ { ModId: 'a', IsActive: false, }, ], }").unwrap();
        assert_eq!(l.display_name(), Some("x"));
        assert_eq!(l.entries()[0].id(), Some("a"));
        assert_eq!(l.entries()[0].is_active, Some(false));
        assert!(parse_modlist("[]").is_err());
        assert!(parse_modlist("").is_err());
    }

    #[test]
    fn brotli_configs_round_trip() {
        let big = "{\n  \"setting\": true,\n".repeat(40) + "}";
        let enc = encode_config(&big);
        assert!(enc.starts_with(BROTLI_PREFIX) && enc.len() < big.len());
        assert_eq!(decode_config(&enc), big);
        assert_eq!(encode_config("{}"), "{}", "small content stays plain");
        assert_eq!(decode_config("br64:not base64!"), "br64:not base64!");
    }

    #[test]
    fn config_list_wins_over_inline() {
        let l = parse_modlist(
            r#"{"mods":[{"modId":"a","configurationFileName":"A.json","configurationContent":"old"},{"modId":"b","configurationFileName":"b.txt","configurationContent":"b1"}],
               "configurations":[{"modId":"A","fileName":"A.json","relativePath":"sub/A.json","content":"new"},{"modId":"orphan","fileName":"o.yml","content":"o"}]}"#,
        )
        .unwrap();
        let c = l.config_files();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].file_name, "b.json");
        assert_eq!((c[1].content.as_str(), c[1].relative_path.as_deref()), ("new", Some("sub/A.json")));
        assert_eq!(c[2].file_name, "o.yml");
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_config_file_name(Some("x/y/Foo.YAML"), "m"), "Foo.YAML");
        assert_eq!(safe_config_file_name(None, "my:mod"), "my_mod.json");
        assert_eq!(safe_config_file_name(Some("a?.cfg"), "m"), "a_.json");
    }
}
