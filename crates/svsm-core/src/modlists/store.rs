//! Modlist files in the manager folder, shared with Simple VS Manager 2.x:
//!
//! * `Modlists/` (and its `Rebuilt/` subfolder): modlists and `.svsmgroup` groups,
//! * `Presets/`: presets (modlists without metadata or versions),
//! * `Backups/`: modlists the app saves by itself before it changes your mods.
//!
//! Every entry is addressed by an id: its path relative to the manager folder with `/`
//! (`Modlists/Rebuilt/My list.json`). Ids from the UI are validated before use.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{parse_modlist, to_json, Modlist};
use super::groups::{self, GroupConflict, GroupDefinition, GroupSource, GROUP_EXTENSION};
use super::pdf;
use crate::error::{CoreError, CoreResult};
use crate::moddb::cache::sanitize_file_name;

pub const MODLISTS_DIR: &str = "Modlists";
pub const PRESETS_DIR: &str = "Presets";
pub const BACKUPS_DIR: &str = "Backups";
/// Automatic backups kept (2.x kept 10).
pub const MAX_BACKUPS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ModlistKind {
    Modlist,
    Preset,
    Backup,
    Group,
}

impl ModlistKind {
    fn dir(self) -> &'static str {
        match self {
            ModlistKind::Modlist | ModlistKind::Group => MODLISTS_DIR,
            ModlistKind::Preset => PRESETS_DIR,
            ModlistKind::Backup => BACKUPS_DIR,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModlistModView {
    pub mod_id: String,
    pub version: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberView {
    pub reference: String,
    /// Id of the member when it's one of our modlists (for selecting it in the UI).
    pub id: Option<String>,
    pub name: String,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub members: Vec<GroupMemberView>,
    pub conflicts: Vec<GroupConflict>,
}

/// One entry for the Modlists screen.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModlistSummary {
    pub id: String,
    pub kind: ModlistKind,
    pub file_name: String,
    pub name: String,
    pub description: Option<String>,
    /// Card icon (Symbols Nerd Font glyph name); `null` = the default icon for the kind.
    pub icon: Option<String>,
    pub version: Option<String>,
    pub game_version: Option<String>,
    pub uploader: Option<String>,
    pub modified_ms: Option<f64>,
    pub mods: Vec<ModlistModView>,
    pub config_count: u32,
    pub has_versions: bool,
    /// The file couldn't be read.
    pub error: Option<String>,
    pub group: Option<GroupView>,
    /// The installed mods match this list exactly (filled in by the caller).
    pub active: bool,
    /// Mods that would have to be downloaded to load it (not installed, not in the library).
    pub downloads_needed: u32,
    /// Local copy of an online modlist.
    pub subscription: Option<SubscriptionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionView {
    pub registry_id: String,
    pub synced_revision: f64,
    pub remote_revision: f64,
    pub update_available: bool,
    /// The online modlist was deleted by its author.
    pub source_gone: bool,
}

/// A loaded entry: a modlist (groups are merged into one).
#[derive(Debug, Clone)]
pub struct LoadedModlist {
    pub id: String,
    pub kind: ModlistKind,
    pub modlist: Modlist,
    pub conflicts: Vec<GroupConflict>,
}

#[derive(Debug, Clone)]
pub struct ModlistStore {
    root: PathBuf,
}

fn other(path: &Path, msg: impl Into<String>) -> CoreError {
    CoreError::io(path, std::io::Error::other(msg.into()))
}

fn modified_ms(path: &Path) -> Option<f64> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as f64)
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
}

/// Reads a modlist from a `.json` or `.pdf` file anywhere (imports, drag and drop).
pub fn read_modlist_file(path: &Path) -> CoreResult<Modlist> {
    if has_ext(path, "pdf") {
        return pdf::read_pdf(path);
    }
    let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
    let mut list = parse_modlist(&text).map_err(|message| CoreError::Parse { path: path.to_path_buf(), message })?;
    if list.display_name().is_none() {
        list.name = Some(stem(path));
    }
    Ok(list)
}

impl ModlistStore {
    pub fn new(manager_dir: &Path) -> Self {
        Self { root: manager_dir.to_path_buf() }
    }

    pub fn dir(&self, kind: ModlistKind) -> PathBuf {
        self.root.join(kind.dir())
    }

    pub fn id_for(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let id = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        self.resolve(&id).ok().map(|_| id)
    }

    /// Validates an id from the UI and returns its path and kind. The file may not exist yet.
    pub fn resolve(&self, id: &str) -> CoreResult<(PathBuf, ModlistKind)> {
        let bad = || other(Path::new(id), "Not a modlist in the manager folder.");
        let parts: Vec<&str> = id.split('/').collect();
        if parts.len() < 2 || parts.len() > 3 || parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['\\', ':'])) {
            return Err(bad());
        }
        let path = parts.iter().fold(self.root.clone(), |p, part| p.join(part));
        let kind = match parts[0] {
            MODLISTS_DIR if has_ext(&path, GROUP_EXTENSION) && parts.len() == 2 => ModlistKind::Group,
            MODLISTS_DIR if has_ext(&path, "json") => ModlistKind::Modlist,
            PRESETS_DIR if has_ext(&path, "json") && parts.len() == 2 => ModlistKind::Preset,
            BACKUPS_DIR if has_ext(&path, "json") && parts.len() == 2 => ModlistKind::Backup,
            _ => return Err(bad()),
        };
        Ok((path, kind))
    }

    fn files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut scan = |dir: PathBuf, sub: bool| {
            let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
            let mut subdirs = Vec::new();
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if sub {
                        subdirs.push(p);
                    }
                } else if has_ext(&p, "json") || has_ext(&p, GROUP_EXTENSION) {
                    out.push(p);
                }
            }
            subdirs
        };
        for sub in scan(self.dir(ModlistKind::Modlist), true) {
            scan(sub, false);
        }
        scan(self.dir(ModlistKind::Preset), false);
        scan(self.dir(ModlistKind::Backup), false);
        out.into_iter().filter(|p| self.id_for(p).is_some()).collect()
    }

    /// Every modlist, preset, backup and group, newest first within each kind.
    pub fn list(&self) -> Vec<ModlistSummary> {
        let mut out: Vec<ModlistSummary> = self.files().iter().map(|p| self.summarize(p)).collect();
        out.sort_by(|a, b| {
            let order = |k: ModlistKind| match k {
                ModlistKind::Group => 0,
                ModlistKind::Modlist => 1,
                ModlistKind::Preset => 2,
                ModlistKind::Backup => 3,
            };
            order(a.kind).cmp(&order(b.kind)).then_with(|| match a.kind {
                ModlistKind::Backup => b.modified_ms.partial_cmp(&a.modified_ms).unwrap_or(std::cmp::Ordering::Equal),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            })
        });
        out
    }

    pub fn summary(&self, id: &str) -> CoreResult<ModlistSummary> {
        let (path, _) = self.resolve(id)?;
        if !path.is_file() {
            return Err(other(&path, "That modlist no longer exists."));
        }
        Ok(self.summarize(&path))
    }

    fn summarize(&self, path: &Path) -> ModlistSummary {
        let id = self.id_for(path).unwrap_or_default();
        let (_, kind) = self.resolve(&id).unwrap_or((path.to_path_buf(), ModlistKind::Modlist));
        let mut s = ModlistSummary {
            id: id.clone(),
            kind,
            file_name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            name: stem(path),
            description: None,
            icon: None,
            version: None,
            game_version: None,
            uploader: None,
            modified_ms: modified_ms(path),
            mods: Vec::new(),
            config_count: 0,
            has_versions: false,
            error: None,
            group: None,
            active: false,
            downloads_needed: 0,
            subscription: None,
        };
        match self.load(&id) {
            Ok(loaded) => {
                let l = &loaded.modlist;
                if let Some(n) = l.display_name() {
                    s.name = n.to_string();
                }
                let t = |v: &Option<String>| v.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
                s.description = t(&l.description);
                s.icon = l.icon();
                s.version = t(&l.version);
                s.game_version = t(&l.game_version);
                s.uploader = t(&l.uploader);
                s.mods = l
                    .entries()
                    .iter()
                    .map(|m| ModlistModView { mod_id: m.id().unwrap_or_default().to_string(), version: m.version().map(String::from), is_active: m.is_active })
                    .collect();
                s.config_count = l.config_files().len() as u32;
                s.has_versions = l.has_versions();
                s.subscription = l.online_subscription.as_ref().and_then(|o| {
                    Some(SubscriptionView {
                        registry_id: o.registry_id.clone().filter(|r| !r.trim().is_empty())?,
                        synced_revision: o.synced_revision,
                        remote_revision: o.last_known_remote_revision,
                        update_available: o.update_available(),
                        source_gone: false,
                    })
                });
                if kind == ModlistKind::Group {
                    let def = self.read_group(path).unwrap_or_default();
                    let (_, members) = self.group_sources(path, &def);
                    s.group = Some(GroupView { members, conflicts: loaded.conflicts });
                }
            }
            Err(e) => s.error = Some(e.to_string()),
        }
        s
    }

    pub fn read_group(&self, path: &Path) -> CoreResult<GroupDefinition> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        groups::parse_group(&text).map_err(|message| CoreError::Parse { path: path.to_path_buf(), message })
    }

    /// Loads a group's members. Returns the loadable ones and a view of every member.
    pub fn group_sources(&self, group_path: &Path, def: &GroupDefinition) -> (Vec<GroupSource>, Vec<GroupMemberView>) {
        let mut sources = Vec::new();
        let mut views = Vec::new();
        for reference in &def.members {
            let path = groups::resolve_reference(group_path, reference);
            // A group can't contain groups.
            let loaded = if has_ext(&path, GROUP_EXTENSION) { None } else { read_modlist_file(&path).ok() };
            let name = loaded.as_ref().and_then(|l| l.display_name().map(String::from)).unwrap_or_else(|| stem(&path));
            views.push(GroupMemberView { reference: reference.clone(), id: self.id_for(&path), name: name.clone(), missing: loaded.is_none() });
            if let Some(modlist) = loaded {
                sources.push(GroupSource { reference: reference.clone(), display_name: name, modlist });
            }
        }
        (sources, views)
    }

    pub fn load(&self, id: &str) -> CoreResult<LoadedModlist> {
        let (path, kind) = self.resolve(id)?;
        if kind == ModlistKind::Group {
            let def = self.read_group(&path)?;
            let (sources, _) = self.group_sources(&path, &def);
            let mut merged = groups::merge(&def, &sources);
            if def.name.as_deref().is_none_or(|n| n.trim().is_empty()) {
                merged.modlist.name = Some(stem(&path));
            }
            return Ok(LoadedModlist { id: id.to_string(), kind, modlist: merged.modlist, conflicts: merged.conflicts });
        }
        Ok(LoadedModlist { id: id.to_string(), kind, modlist: read_modlist_file(&path)?, conflicts: Vec::new() })
    }

    fn write(&self, path: &Path, text: &str) -> CoreResult<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        crate::fsutil::write_synced(&tmp, text.as_bytes())?;
        crate::fsutil::rename_with_retry(&tmp, path)
    }

    /// Where a new entry named `name` would be saved.
    pub fn id_for_name(&self, kind: ModlistKind, name: &str) -> String {
        let ext = if kind == ModlistKind::Group { GROUP_EXTENSION } else { "json" };
        let fallback = match kind {
            ModlistKind::Preset => "Preset",
            ModlistKind::Group => "Modlist group",
            _ => "Modlist",
        };
        format!("{}/{}.{ext}", kind.dir(), sanitize_file_name(name, fallback))
    }

    /// Saves a modlist or preset. Refuses to replace an existing file unless `overwrite`.
    pub fn save(&self, kind: ModlistKind, name: &str, modlist: &Modlist, overwrite: bool) -> CoreResult<String> {
        if kind == ModlistKind::Group {
            return Err(other(Path::new(name), "Use save_group for groups."));
        }
        let id = self.id_for_name(kind, name);
        let (path, _) = self.resolve(&id)?;
        if path.exists() && !overwrite {
            return Err(other(&path, format!("\"{}\" already exists.", stem(&path))));
        }
        self.write(&path, &to_json(modlist))?;
        Ok(id)
    }

    /// Creates or updates a group. `id` is the existing group (renaming moves the file).
    pub fn save_group(&self, id: Option<&str>, def: &GroupDefinition) -> CoreResult<String> {
        let name = def.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Modlist group");
        let new_id = self.id_for_name(ModlistKind::Group, name);
        let (new_path, _) = self.resolve(&new_id)?;
        let old = id.map(|i| self.resolve(i)).transpose()?.filter(|(_, k)| *k == ModlistKind::Group).map(|(p, _)| p);
        let renaming = old.as_ref().is_some_and(|o| o != &new_path);
        if (old.is_none() || renaming) && new_path.exists() {
            return Err(other(&new_path, format!("\"{name}\" already exists.")));
        }
        // Member references are relative to the group file; it stays in the same folder.
        self.write(&new_path, &groups::group_to_json(def))?;
        if let Some(o) = old.filter(|_| renaming) {
            let _ = std::fs::remove_file(o);
        }
        Ok(new_id)
    }

    /// Sets or clears (`None`) an entry's card icon. Other fields in the file are kept as they are.
    pub fn set_icon(&self, id: &str, icon: Option<&str>) -> CoreResult<()> {
        let (path, kind) = self.resolve(id)?;
        if !path.is_file() {
            return Err(other(&path, "That modlist no longer exists."));
        }
        let icon = super::format::clean_icon(icon);
        if kind == ModlistKind::Group {
            let mut def = self.read_group(&path)?;
            def.icon = icon;
            return self.write(&path, &groups::group_to_json(&def));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))?;
        let text = match serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) {
            // Plain JSON: change only the icon, so fields we don't know survive.
            Ok(serde_json::Value::Object(mut map)) => {
                map.retain(|k, _| !k.eq_ignore_ascii_case("icon"));
                if let Some(icon) = icon {
                    map.insert("icon".into(), serde_json::Value::String(icon));
                }
                serde_json::to_string_pretty(&serde_json::Value::Object(map)).expect("JSON always serializes")
            }
            // Lenient JSON (comments, trailing commas): rewrite it in our format.
            _ => {
                let mut list = parse_modlist(&text).map_err(|message| CoreError::Parse { path: path.clone(), message })?;
                list.icon = icon;
                to_json(&list)
            }
        };
        self.write(&path, &text)
    }

    /// Moves entries to the system trash.
    pub fn delete(&self, ids: &[String]) -> CoreResult<()> {
        for id in ids {
            let (path, _) = self.resolve(id)?;
            if path.is_file() {
                trash::delete(&path).map_err(|e| other(&path, e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Saves an automatic backup like 2.x (`05 Nov 2025 • 19.09 • 05s -- ModlistLoaded (5 mods)`)
    /// and keeps the newest [`MAX_BACKUPS`] automatic ones.
    pub fn snapshot(&self, trigger: &str, mut modlist: Modlist) -> CoreResult<String> {
        let n = modlist.entries().len();
        let stamp = chrono::Local::now().format("%d %b %Y • %H.%M • %Ss");
        let label = if n == 1 { "1 mod".to_string() } else { format!("{n} mods") };
        let mut name = format!("{stamp} -- {trigger} ({label})");
        modlist.name = Some(name.clone());
        // Two snapshots in the same second: add a counter.
        let mut i = 2;
        while self.resolve(&self.id_for_name(ModlistKind::Backup, &name))?.0.exists() {
            name = format!("{stamp} -- {trigger} ({label}) {i}");
            i += 1;
        }
        let id = self.save(ModlistKind::Backup, &name, &modlist, false)?;
        self.prune_backups();
        Ok(id)
    }

    fn prune_backups(&self) {
        let Ok(rd) = std::fs::read_dir(self.dir(ModlistKind::Backup)) else { return };
        let mut auto: Vec<(PathBuf, Option<f64>)> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| has_ext(p, "json"))
            // App-start backups from 2.x are left alone (2.x pruned them separately).
            .filter(|p| {
                let s = stem(p);
                !(s.ends_with("_AppStarted") || s.contains("-- AppStarted"))
            })
            .map(|p| {
                let m = modified_ms(&p);
                (p, m)
            })
            .collect();
        auto.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (p, _) in auto.into_iter().skip(MAX_BACKUPS) {
            let _ = std::fs::remove_file(p);
        }
    }

    /// Copies an outside modlist (json or pdf) into `Modlists/` as JSON.
    pub fn import(&self, source: &Path, overwrite: bool) -> CoreResult<String> {
        let list = read_modlist_file(source)?;
        let name = list.display_name().map(String::from).unwrap_or_else(|| stem(source));
        self.save(ModlistKind::Modlist, &name, &list, overwrite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modlists::format::ModlistMod;

    fn list(name: &str, ids: &[&str]) -> Modlist {
        Modlist {
            name: Some(name.into()),
            mods: Some(ids.iter().map(|i| ModlistMod { mod_id: Some((*i).into()), version: Some("1.0.0".into()), is_active: Some(true), ..Default::default() }).collect()),
            include_mod_status: Some(true),
            include_mod_versions: Some(true),
            exclusive: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn ids_are_validated() {
        let s = ModlistStore::new(Path::new("/m"));
        assert_eq!(s.resolve("Modlists/a.json").unwrap().1, ModlistKind::Modlist);
        assert_eq!(s.resolve("Modlists/Rebuilt/a.json").unwrap().1, ModlistKind::Modlist);
        assert_eq!(s.resolve("Modlists/g.svsmgroup").unwrap().1, ModlistKind::Group);
        assert_eq!(s.resolve("Presets/p.json").unwrap().1, ModlistKind::Preset);
        assert_eq!(s.resolve("Backups/b.json").unwrap().1, ModlistKind::Backup);
        for bad in ["../x.json", "Modlists/../../x.json", "Other/a.json", "Modlists/a.txt", "Presets/sub/a.json", "Modlists/a\\..\\b.json", "Modlists//a.json", "Modlists/c:x.json"] {
            assert!(s.resolve(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn save_list_load_and_delete() {
        let t = tempfile::tempdir().unwrap();
        let s = ModlistStore::new(t.path());
        let id = s.save(ModlistKind::Modlist, "My: list", &list("My: list", &["a", "b"]), false).unwrap();
        assert_eq!(id, "Modlists/My_ list.json");
        assert!(s.save(ModlistKind::Modlist, "My: list", &list("x", &[]), false).is_err());
        s.save(ModlistKind::Preset, "Quick", &list("Quick", &["a"]), false).unwrap();
        std::fs::create_dir_all(t.path().join("Modlists/Rebuilt")).unwrap();
        std::fs::write(t.path().join("Modlists/Rebuilt/old.json"), r#"{"mods":[{"modId":"z"}]}"#).unwrap();
        std::fs::write(t.path().join("Modlists/broken.json"), "{").unwrap();

        let all = s.list();
        let names: Vec<_> = all.iter().map(|e| (e.kind, e.name.as_str())).collect();
        assert_eq!(names, [(ModlistKind::Modlist, "broken"), (ModlistKind::Modlist, "My: list"), (ModlistKind::Modlist, "old"), (ModlistKind::Preset, "Quick")]);
        assert!(all[0].error.is_some());
        assert_eq!(all[1].mods.len(), 2);
        assert_eq!(s.load(&id).unwrap().modlist.entries().len(), 2);

        s.delete(std::slice::from_ref(&id)).unwrap();
        assert!(s.load(&id).is_err());
    }

    #[test]
    fn groups_merge_members() {
        let t = tempfile::tempdir().unwrap();
        let s = ModlistStore::new(t.path());
        let a = s.save(ModlistKind::Modlist, "A", &list("A", &["x", "y"]), false).unwrap();
        let mut b_list = list("B", &["y", "z"]);
        b_list.mods.as_mut().unwrap()[0].version = Some("2.0.0".into());
        let b = s.save(ModlistKind::Modlist, "B", &b_list, false).unwrap();
        let g_path = t.path().join("Modlists/G.svsmgroup");
        let def = GroupDefinition {
            name: Some("G".into()),
            members: vec![
                groups::create_reference(&g_path, &s.resolve(&a).unwrap().0),
                groups::create_reference(&g_path, &s.resolve(&b).unwrap().0),
                "gone.json".into(),
            ],
            ..Default::default()
        };
        let g = s.save_group(None, &def).unwrap();
        assert_eq!(g, "Modlists/G.svsmgroup");
        let loaded = s.load(&g).unwrap();
        assert_eq!(loaded.modlist.entries().len(), 3);
        assert_eq!(loaded.conflicts.len(), 1);
        let sum = s.summary(&g).unwrap();
        let view = sum.group.unwrap();
        assert_eq!(view.members.iter().map(|m| m.missing).collect::<Vec<_>>(), [false, false, true]);
        assert_eq!(view.members[0].id.as_deref(), Some("Modlists/A.json"));

        // Rename moves the file.
        let renamed = s.save_group(Some(&g), &GroupDefinition { name: Some("H".into()), ..def }).unwrap();
        assert_eq!(renamed, "Modlists/H.svsmgroup");
        assert!(!g_path.exists());
    }

    #[test]
    fn snapshots_are_pruned() {
        let t = tempfile::tempdir().unwrap();
        let s = ModlistStore::new(t.path());
        std::fs::create_dir_all(s.dir(ModlistKind::Backup)).unwrap();
        std::fs::write(s.dir(ModlistKind::Backup).join("x -- AppStarted (1 mod).json"), "{}").unwrap();
        for _ in 0..12 {
            s.snapshot("ModlistLoaded", list("ignored", &["a"])).unwrap();
        }
        let backups: Vec<_> = s.list().into_iter().filter(|e| e.kind == ModlistKind::Backup).collect();
        assert_eq!(backups.len(), MAX_BACKUPS + 1);
        assert!(backups.iter().any(|b| b.name.contains("-- ModlistLoaded (1 mod)")));
    }

    #[test]
    fn icons_are_saved_without_touching_other_fields() {
        let t = tempfile::tempdir().unwrap();
        let s = ModlistStore::new(t.path());
        std::fs::create_dir_all(t.path().join("Modlists")).unwrap();
        std::fs::write(t.path().join("Modlists/a.json"), r#"{"name":"A","Icon":"old","futureField":7,"mods":[{"modId":"x"}]}"#).unwrap();
        let id = "Modlists/a.json";
        assert_eq!(s.summary(id).unwrap().icon.as_deref(), Some("old"));
        s.set_icon(id, Some("md-sword")).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(t.path().join(id)).unwrap()).unwrap();
        assert_eq!((raw["icon"].as_str(), raw["futureField"].as_i64(), raw.get("Icon")), (Some("md-sword"), Some(7), None));
        assert_eq!(s.summary(id).unwrap().icon.as_deref(), Some("md-sword"));
        s.set_icon(id, Some("bad icon\"<")).unwrap();
        assert_eq!(s.summary(id).unwrap().icon, None, "invalid names are dropped");

        // Lenient files are rewritten; groups keep the icon in the definition.
        std::fs::write(t.path().join("Modlists/b.json"), "{name: 'B', mods: [{modId: 'y'},],}").unwrap();
        s.set_icon("Modlists/b.json", Some("fa-dragon")).unwrap();
        assert_eq!(s.summary("Modlists/b.json").unwrap().icon.as_deref(), Some("fa-dragon"));
        let g = s.save_group(None, &GroupDefinition { name: Some("G".into()), members: vec!["a.json".into()], ..Default::default() }).unwrap();
        s.set_icon(&g, Some("md-castle")).unwrap();
        assert_eq!(s.summary(&g).unwrap().icon.as_deref(), Some("md-castle"));
        assert_eq!(s.read_group(&s.resolve(&g).unwrap().0).unwrap().members, ["a.json"]);
    }

    #[test]
    fn imports_json_and_pdf() {
        let t = tempfile::tempdir().unwrap();
        let s = ModlistStore::new(t.path());
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modlists/svsm-2.3.0-with-configs.pdf");
        let id = s.import(&pdf, false).unwrap();
        assert_eq!(id, "Modlists/Golden list ä.json");
        assert_eq!(s.load(&id).unwrap().modlist.config_files().len(), 2);
    }
}
