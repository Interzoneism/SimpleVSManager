//! Working out what loading a modlist will do, before doing it. The Tauri side runs the plan
//! (downloads, installs, clientsettings.json writes); this part is pure and unit tested.
//!
//! One flow for every source (modlist, preset, backup, group, file, later cloud), replacing the
//! four near-identical load paths in 2.x.

use serde::{Deserialize, Serialize};

use super::format::{ConfigFile, Modlist};
use crate::mods::{is_base_game_mod, InstalledMod, ModSourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LoadMode {
    /// Make the installed mods match the list: install missing mods, switch versions, set
    /// on/off, and move mods that aren't in the list to the trash. (2.x "Only Modlist mods".)
    Replace,
    /// Install missing mods and switch versions, set on/off, keep everything else.
    /// (2.x "Add Modlist mods".)
    Add,
    /// Only switch installed mods on or off; nothing is downloaded or removed. (2.x presets.)
    StatesOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PlannedInstall {
    pub mod_id: String,
    /// `null` = newest release for the target game version.
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PlannedVersionChange {
    pub key: String,
    pub mod_id: String,
    pub name: String,
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PlannedModRef {
    pub key: String,
    pub mod_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PlannedState {
    pub mod_id: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ApplyPlan {
    pub install: Vec<PlannedInstall>,
    pub change_version: Vec<PlannedVersionChange>,
    pub remove: Vec<PlannedModRef>,
    /// On/off state per mod id, applied after installs (only for mods that end up installed).
    pub states: Vec<PlannedState>,
    /// Old presets without per-mod state: replaces clientsettings.json `disabledMods`.
    pub disabled_entries: Option<Vec<String>>,
    /// Mods whose version would change but can't be (loose .cs/.dll files).
    pub skipped: Vec<String>,
    /// How many installed mods change on/off state.
    pub toggles: u32,
    #[serde(skip)]
    pub configs: Vec<ConfigFile>,
}

impl ApplyPlan {
    pub fn is_empty(&self) -> bool {
        self.install.is_empty()
            && self.change_version.is_empty()
            && self.remove.is_empty()
            && self.toggles == 0
            && self.disabled_entries.is_none()
            && self.configs.is_empty()
    }
}

pub fn versions_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    a.eq_ignore_ascii_case(b)
        || matches!((crate::version::normalize(a), crate::version::normalize(b)), (Some(x), Some(y)) if x == y)
}

/// Builds the plan for loading `list` over the `installed` mods.
pub fn plan(list: &Modlist, installed: &[InstalledMod], mode: LoadMode, import_configs: bool) -> ApplyPlan {
    let mut p = ApplyPlan::default();
    let find = |id: &str| installed.iter().find(|m| m.mod_id.eq_ignore_ascii_case(id));
    let use_versions = mode != LoadMode::StatesOnly && list.has_versions();
    let has_status = list.has_status();

    for e in list.entries() {
        let id = e.id().expect("entries have ids");
        if is_base_game_mod(id) || ["game", "creative", "survival"].iter().any(|g| g.eq_ignore_ascii_case(id)) {
            continue;
        }
        let wanted_active = if has_status { e.is_active } else { None };
        match find(id) {
            None => {
                if mode == LoadMode::StatesOnly {
                    continue;
                }
                p.install.push(PlannedInstall { mod_id: id.to_string(), version: if use_versions { e.version().map(String::from) } else { None } });
                // Listed mods are wanted on unless the list says otherwise.
                p.states.push(PlannedState { mod_id: id.to_string(), active: wanted_active.unwrap_or(true) });
            }
            Some(m) => {
                if let (true, Some(v)) = (use_versions, e.version()) {
                    if !m.version.as_deref().is_some_and(|iv| versions_match(iv, v)) {
                        if matches!(m.source_kind, ModSourceKind::Zip | ModSourceKind::Folder) {
                            p.change_version.push(PlannedVersionChange {
                                key: m.key.clone(),
                                mod_id: m.mod_id.clone(),
                                name: m.name.clone(),
                                from: m.version.clone(),
                                to: v.to_string(),
                            });
                        } else {
                            p.skipped.push(format!("{} (wants {v}, can't change a loose file)", m.name));
                        }
                    }
                }
                if let Some(active) = wanted_active {
                    if active != m.is_active {
                        p.toggles += 1;
                    }
                    p.states.push(PlannedState { mod_id: m.mod_id.clone(), active });
                }
            }
        }
    }

    if !has_status {
        if let Some(entries) = &list.disabled_entries {
            p.disabled_entries = Some(entries.iter().map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect());
        }
    }

    if mode == LoadMode::Replace {
        let keep: Vec<String> = list.entries().iter().filter_map(|e| e.id()).map(str::to_ascii_lowercase).collect();
        for m in installed {
            let id = m.mod_id.trim();
            if !is_base_game_mod(id) && !keep.contains(&id.to_ascii_lowercase()) {
                p.remove.push(PlannedModRef { key: m.key.clone(), mod_id: m.mod_id.clone(), name: m.name.clone() });
            }
        }
    }

    if import_configs {
        p.configs = list.config_files();
    }
    p
}

/// The installed mods already are this list (same mods, versions and on/off state).
pub fn matches_installed(list: &Modlist, installed: &[InstalledMod]) -> bool {
    !list.entries().is_empty() && plan(list, installed, LoadMode::Replace, false).is_empty()
}

/// How many of the plan's installs and version switches can come from the mod library
/// (`has_cached(mod id, version)`) and how many must be downloaded.
pub fn library_split(plan: &ApplyPlan, has_cached: impl Fn(&str, &str) -> bool) -> (u32, u32) {
    let mut local = 0;
    let mut download = 0;
    let wanted = plan.install.iter().map(|i| (i.mod_id.as_str(), i.version.as_deref())).chain(plan.change_version.iter().map(|c| (c.mod_id.as_str(), Some(c.to.as_str()))));
    for (id, version) in wanted {
        match version {
            Some(v) if has_cached(id, v) => local += 1,
            _ => download += 1,
        }
    }
    (local, download)
}

#[cfg(test)]
pub(crate) mod tests_support {
    use crate::mods::{InstalledMod, ModSourceKind};

    pub fn installed(id: &str, name: &str) -> InstalledMod {
        InstalledMod {
            key: format!("/mods/{id}.zip"),
            mod_id: id.into(),
            name: name.into(),
            manifest_name: None,
            version: Some("1.0.0".into()),
            network_version: None,
            description: None,
            website: None,
            authors: vec![],
            contributors: vec![],
            dependencies: vec![],
            side: None,
            required_on_client: None,
            required_on_server: None,
            source_path: format!("/mods/{id}.zip"),
            source_kind: ModSourceKind::Zip,
            file_name: format!("{id}.zip"),
            size_bytes: None,
            modified_ms: None,
            icon_key: None,
            error: None,
            load_error: None,
            missing_dependencies: vec![],
            dependency_has_errors: false,
            duplicates: vec![],
            is_active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modlists::format::parse_modlist;

    fn installed(id: &str, version: &str, active: bool) -> InstalledMod {
        let mut m = super::tests_support::installed(id, &id.to_uppercase());
        m.version = Some(version.into());
        m.is_active = active;
        m
    }

    const LIST: &str = r#"{"mods":[
        {"modId":"keep","version":"1.0.0","isActive":true},
        {"modId":"Upgrade","version":"2.0.0","isActive":false},
        {"modId":"new","version":"3.0.0","isActive":true},
        {"modId":"newoff","isActive":false},
        {"modId":"game"}],
        "configurations":[{"modId":"keep","fileName":"k.json","content":"{}"}],
        "includeModStatus":true,"includeModVersions":true}"#;

    fn mods() -> Vec<InstalledMod> {
        vec![installed("keep", "v1.0.0", false), installed("upgrade", "1.0.0", true), installed("extra", "1.0.0", true)]
    }

    #[test]
    fn replace_matches_the_list() {
        let p = plan(&parse_modlist(LIST).unwrap(), &mods(), LoadMode::Replace, true);
        assert_eq!(p.install.iter().map(|i| (i.mod_id.as_str(), i.version.as_deref())).collect::<Vec<_>>(), [("new", Some("3.0.0")), ("newoff", None)]);
        assert_eq!(p.change_version.len(), 1);
        assert_eq!((p.change_version[0].mod_id.as_str(), p.change_version[0].to.as_str()), ("upgrade", "2.0.0"));
        assert_eq!(p.remove.iter().map(|r| r.mod_id.as_str()).collect::<Vec<_>>(), ["extra"]);
        assert_eq!(p.toggles, 2, "keep on, upgrade off");
        assert!(p.states.contains(&PlannedState { mod_id: "newoff".into(), active: false }));
        assert_eq!(p.configs.len(), 1);
    }

    #[test]
    fn add_keeps_extras_and_states_only_never_downloads() {
        let l = parse_modlist(LIST).unwrap();
        let add = plan(&l, &mods(), LoadMode::Add, false);
        assert!(add.remove.is_empty() && add.configs.is_empty());
        assert_eq!(add.install.len(), 2);
        let states = plan(&l, &mods(), LoadMode::StatesOnly, false);
        assert!(states.install.is_empty() && states.change_version.is_empty() && states.remove.is_empty());
        assert_eq!(states.toggles, 2);
        assert_eq!(states.states.len(), 2);
    }

    #[test]
    fn old_presets_use_disabled_entries() {
        let l = parse_modlist(r#"{"name":"p","disabledEntries":["extra@1.0.0"," "]}"#).unwrap();
        let p = plan(&l, &mods(), LoadMode::StatesOnly, false);
        assert_eq!(p.disabled_entries, Some(vec!["extra@1.0.0".to_string()]));
        assert!(!p.is_empty());
        let empty = plan(&parse_modlist(r#"{"mods":[{"modId":"keep"}]}"#).unwrap(), &mods(), LoadMode::StatesOnly, false);
        assert!(empty.is_empty(), "{empty:?}");
    }

    #[test]
    fn active_and_library_split() {
        let l = parse_modlist(LIST).unwrap();
        let p = plan(&l, &mods(), LoadMode::Replace, false);
        let (local, download) = library_split(&p, |id, v| id == "upgrade" && v == "2.0.0");
        assert_eq!((local, download), (1, 2));
        assert!(!matches_installed(&l, &mods()));
        let same = parse_modlist(r#"{"mods":[{"modId":"keep","version":"1.0.0","isActive":false},{"modId":"upgrade","version":"1.0.0","isActive":true},{"modId":"extra","version":"1.0.0","isActive":true}]}"#).unwrap();
        assert!(matches_installed(&same, &mods()));
        assert!(!matches_installed(&parse_modlist("{}").unwrap(), &[]));
    }

    #[test]
    fn loose_files_are_skipped() {
        let mut m = installed("upgrade", "1.0.0", true);
        m.source_kind = ModSourceKind::Assembly;
        let p = plan(&parse_modlist(LIST).unwrap(), &[m], LoadMode::Add, false);
        assert!(p.change_version.is_empty());
        assert_eq!(p.skipped.len(), 1);
    }
}
