//! Modlists, presets, groups and automatic backups: one JSON format (2.3.0's), one store, one
//! load flow. See `NOT_WORTH_PORTING.md` for what was merged and dropped.

pub mod apply;
pub mod configs;
pub mod format;
pub mod groups;
pub mod pdf;
pub mod store;

pub use apply::{plan, ApplyPlan, LoadMode};
pub use format::{parse_modlist, to_json, ConfigFile, ConfigList, Modlist, ModlistConfig, ModlistMod, OnlineSubscription};
pub use groups::{GroupConflict, GroupDefinition};
pub use store::{read_modlist_file, LoadedModlist, ModlistKind, ModlistStore, ModlistSummary};

use crate::mods::{is_base_game_mod, InstalledMod};

/// Metadata for a modlist made from the installed mods.
#[derive(Debug, Clone, Default)]
pub struct ModlistMeta {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub game_version: Option<String>,
    pub uploader: Option<String>,
}

/// Captures the installed mods. `with_versions` = a modlist (exact versions, exclusive);
/// without = a preset (only which mods are on or off), like 2.x.
pub fn from_installed(installed: &[InstalledMod], meta: ModlistMeta, with_versions: bool, configs: Vec<ModlistConfig>) -> Modlist {
    let mut seen = std::collections::HashSet::new();
    let mut mods: Vec<ModlistMod> = installed
        .iter()
        .filter(|m| !m.mod_id.trim().is_empty() && !is_base_game_mod(&m.mod_id))
        .filter(|m| seen.insert(m.mod_id.to_ascii_lowercase()))
        .map(|m| ModlistMod {
            mod_id: Some(m.mod_id.trim().to_string()),
            version: if with_versions { m.version.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from) } else { None },
            is_active: Some(m.is_active),
            ..Default::default()
        })
        .collect();
    mods.sort_by_key(|m| m.mod_id.as_deref().unwrap_or("").to_lowercase());
    let t = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Modlist {
        name: Some(meta.name.trim().to_string()),
        description: t(meta.description),
        version: t(meta.version),
        game_version: t(meta.game_version),
        uploader: t(meta.uploader),
        disabled_entries: None,
        mods: Some(mods),
        configurations: (!configs.is_empty()).then_some(configs),
        include_mod_status: Some(true),
        include_mod_versions: with_versions.then_some(true),
        exclusive: with_versions.then_some(true),
        online_subscription: None,
    }
}

/// `/moddb install <id>@<version>` for pasting into a server console (2.x `ServerCommandBuilder`).
pub fn server_install_command(mod_id: &str, version: &str) -> Option<String> {
    let (id, v) = (mod_id.trim(), version.trim());
    (!id.is_empty() && !v.is_empty()).then(|| format!("/moddb install {id}@{v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_installed_mods() {
        let mut a = apply::tests_support::installed("b", "B");
        a.is_active = false;
        let b = apply::tests_support::installed("a", "A");
        let base = apply::tests_support::installed("VSSurvivalMod", "Survival");
        let dup = apply::tests_support::installed("A", "A copy");
        let mods = [a, b, base, dup];
        let list = from_installed(&mods, ModlistMeta { name: " L ".into(), uploader: Some(" ".into()), ..Default::default() }, true, vec![]);
        assert_eq!(list.name.as_deref(), Some("L"));
        assert_eq!(list.uploader, None);
        let ids: Vec<_> = list.entries().iter().map(|m| (m.id().unwrap(), m.is_active)).collect();
        assert_eq!(ids, [("a", Some(true)), ("b", Some(false))]);
        assert_eq!(list.exclusive, Some(true));
        let preset = from_installed(&mods, ModlistMeta::default(), false, vec![]);
        assert!(preset.entries().iter().all(|m| m.version.is_none()));
        assert_eq!((preset.exclusive, preset.include_mod_versions), (None, None));
        assert_eq!(server_install_command(" carryon ", "1.2.0").as_deref(), Some("/moddb install carryon@1.2.0"));
        assert_eq!(server_install_command("x", " "), None);
    }
}
