//! Modlist groups (`.svsmgroup`): a named set of references to other modlist files, merged
//! into one modlist when loaded. Port of 2.x `ModlistGroupService`.
//!
//! When two members disagree about a mod (different version or config), that's a conflict. The
//! newest version wins by default (then the one with a config); the user's choice is saved in
//! `conflictResolutions` (mod id -> member reference).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{Modlist, ModlistConfig, ModlistMod};

pub const GROUP_EXTENSION: &str = "svsmgroup";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupDefinition {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Card icon (Symbols Nerd Font glyph name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    /// Paths of member modlists, relative to the group file when inside its folder.
    pub members: Vec<String>,
    pub conflict_resolutions: BTreeMap<String, String>,
}

impl Default for GroupDefinition {
    fn default() -> Self {
        Self {
            schema_version: 1,
            name: None,
            description: None,
            icon: None,
            version: None,
            game_version: None,
            members: Vec::new(),
            conflict_resolutions: BTreeMap::new(),
        }
    }
}

pub fn parse_group(text: &str) -> Result<GroupDefinition, String> {
    let text = text.trim_start_matches('\u{feff}');
    let v: serde_json::Value = serde_json::from_str(text)
        .or_else(|_| json5::from_str(text))
        .map_err(|e| format!("The group file is not valid JSON: {e}"))?;
    // 2.x wrote camelCase but read case-insensitively.
    let v = match v {
        serde_json::Value::Object(m) => serde_json::Value::Object(
            m.into_iter()
                .map(|(k, v)| {
                    let mut c = k.chars();
                    (c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or(k), v)
                })
                .collect(),
        ),
        _ => return Err("The file is not a modlist group.".into()),
    };
    serde_json::from_value(v).map_err(|e| format!("The group file is damaged: {e}"))
}

pub fn group_to_json(g: &GroupDefinition) -> String {
    let mut g = g.clone();
    g.schema_version = 1;
    serde_json::to_string_pretty(&g).expect("groups always serialize")
}

/// Reference to `member` as stored in the group at `group_file`: relative (with `/`) when the
/// member is in the group's folder or below, absolute otherwise.
pub fn create_reference(group_file: &Path, member: &Path) -> String {
    let dir = group_file.parent().unwrap_or(Path::new(""));
    match member.strip_prefix(dir) {
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
        Err(_) => member.display().to_string(),
    }
}

/// Resolves a member reference (relative to the group file, either separator).
pub fn resolve_reference(group_file: &Path, reference: &str) -> PathBuf {
    let r = reference.trim();
    let p = Path::new(r);
    if p.is_absolute() || r.starts_with('/') || r.starts_with('\\') {
        return p.to_path_buf();
    }
    let mut out = group_file.parent().unwrap_or(Path::new("")).to_path_buf();
    for part in r.split(['/', '\\']).filter(|s| !s.is_empty() && *s != ".") {
        if part == ".." {
            out.pop();
        } else {
            out.push(part);
        }
    }
    // Normalise any remaining components.
    out.components().filter(|c| !matches!(c, Component::CurDir)).collect()
}

pub fn references_equal(a: &str, b: &str) -> bool {
    a.trim().replace('\\', "/").eq_ignore_ascii_case(&b.trim().replace('\\', "/"))
}

/// A member that could be loaded.
#[derive(Debug, Clone)]
pub struct GroupSource {
    pub reference: String,
    pub display_name: String,
    pub modlist: Modlist,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConflictChoice {
    pub reference: String,
    pub modlist_name: String,
    pub version: Option<String>,
    pub has_config: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupConflict {
    pub mod_id: String,
    pub choices: Vec<ConflictChoice>,
    /// Reference of the member whose entry is used.
    pub selected: String,
    /// The user picked it (otherwise it's the default choice).
    pub resolved_by_user: bool,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub modlist: Modlist,
    pub conflicts: Vec<GroupConflict>,
}

fn configs_for<'a>(list: &'a Modlist, mod_id: &str) -> Vec<&'a ModlistConfig> {
    list.configurations
        .iter()
        .flatten()
        .filter(|c| c.mod_id.as_deref().map(str::trim).is_some_and(|i| i.eq_ignore_ascii_case(mod_id)))
        .collect()
}

fn has_config(list: &Modlist, m: &ModlistMod) -> bool {
    let id = m.id().unwrap_or_default();
    let listed = configs_for(list, id);
    if listed.is_empty() {
        m.configuration_content.as_deref().is_some_and(|c| !c.is_empty())
    } else {
        listed.iter().any(|c| c.content.as_deref().is_some_and(|c| !c.is_empty()))
    }
}

/// Two entries are the same choice when version and configs match.
fn choice_key(list: &Modlist, m: &ModlistMod) -> String {
    let id = m.id().unwrap_or_default();
    let mut configs: Vec<(String, String, String)> = configs_for(list, id)
        .into_iter()
        .filter(|c| c.content.as_deref().is_some_and(|c| !c.is_empty()))
        .map(|c| {
            (
                c.file_name.as_deref().unwrap_or("").trim().to_uppercase(),
                c.relative_path.as_deref().unwrap_or("").trim().replace('\\', "/").to_uppercase(),
                c.content.clone().unwrap_or_default(),
            )
        })
        .collect();
    if configs.is_empty() {
        if let Some(c) = m.configuration_content.as_deref().filter(|c| !c.is_empty()) {
            configs.push((m.configuration_file_name.as_deref().unwrap_or("").trim().to_uppercase(), String::new(), c.to_string()));
        }
    }
    configs.sort();
    serde_json::json!({ "version": m.version(), "configs": configs }).to_string()
}

fn compare_versions(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let (na, nb) = (a.and_then(crate::version::normalize), b.and_then(crate::version::normalize));
    match (na.is_some(), nb.is_some()) {
        (true, true) => {
            if crate::version::is_candidate_newer(a, b) {
                Greater
            } else if crate::version::is_candidate_newer(b, a) {
                Less
            } else {
                Equal
            }
        }
        (true, false) => Greater,
        (false, true) => Less,
        (false, false) => a.unwrap_or("").to_lowercase().cmp(&b.unwrap_or("").to_lowercase()),
    }
}

/// Merges the members into one exclusive modlist and reports conflicts.
pub fn merge(def: &GroupDefinition, sources: &[GroupSource]) -> MergeResult {
    struct Candidate<'a> {
        source: &'a GroupSource,
        entry: &'a ModlistMod,
    }
    let mut order: Vec<String> = Vec::new();
    let mut candidates: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    let mut disabled: Vec<String> = Vec::new();

    for s in sources {
        for e in s.modlist.disabled_entries.iter().flatten() {
            let e = e.trim();
            if !e.is_empty() && !disabled.iter().any(|d| d.eq_ignore_ascii_case(e)) {
                disabled.push(e.to_string());
            }
        }
        for m in s.modlist.mods.iter().flatten() {
            let Some(id) = m.id() else { continue };
            let key = id.to_ascii_lowercase();
            let list = candidates.entry(key.clone()).or_insert_with(|| {
                order.push(id.to_string());
                Vec::new()
            });
            // The same id twice in one member is one choice.
            if !list.iter().any(|c| references_equal(&c.source.reference, &s.reference)) {
                list.push(Candidate { source: s, entry: m });
            }
        }
    }

    let resolution = |mod_id: &str| {
        def.conflict_resolutions.iter().find(|(k, _)| k.eq_ignore_ascii_case(mod_id)).map(|(_, v)| v.as_str())
    };

    let mut conflicts = Vec::new();
    let mut mods = Vec::new();
    let mut configurations = Vec::new();
    for id in &order {
        let all = &candidates[&id.to_ascii_lowercase()];
        // Distinct choices, first member wins among identical ones.
        let mut distinct: Vec<&Candidate> = Vec::new();
        for c in all {
            let k = choice_key(&c.source.modlist, c.entry);
            if !distinct.iter().any(|d| choice_key(&d.source.modlist, d.entry) == k) {
                distinct.push(c);
            }
        }
        let chosen: &Candidate = if distinct.len() == 1 {
            distinct[0]
        } else {
            let picked = resolution(id).and_then(|r| all.iter().find(|c| references_equal(&c.source.reference, r)));
            let by_user = picked.is_some();
            let chosen = match picked {
                // Map the user's pick onto the distinct entry with the same content.
                Some(p) => {
                    let k = choice_key(&p.source.modlist, p.entry);
                    *distinct.iter().find(|d| choice_key(&d.source.modlist, d.entry) == k).expect("distinct covers all")
                }
                None => {
                    let mut best = distinct[0];
                    for c in &distinct[1..] {
                        let cmp = compare_versions(c.entry.version(), best.entry.version());
                        if cmp.is_gt()
                            || (cmp.is_eq() && has_config(&c.source.modlist, c.entry) && !has_config(&best.source.modlist, best.entry))
                        {
                            best = c;
                        }
                    }
                    best
                }
            };
            conflicts.push(GroupConflict {
                mod_id: id.clone(),
                choices: distinct
                    .iter()
                    .map(|d| ConflictChoice {
                        reference: d.source.reference.clone(),
                        modlist_name: d.source.display_name.clone(),
                        version: d.entry.version().map(String::from),
                        has_config: has_config(&d.source.modlist, d.entry),
                    })
                    .collect(),
                selected: chosen.source.reference.clone(),
                resolved_by_user: by_user,
            });
            chosen
        };
        let mut entry = chosen.entry.clone();
        entry.mod_id = Some(id.clone());
        mods.push(entry);
        configurations.extend(configs_for(&chosen.source.modlist, id).into_iter().cloned());
    }

    let trimmed = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let modlist = Modlist {
        name: Some(trimmed(&def.name).unwrap_or_else(|| "Unnamed Modlist Group".into())),
        description: trimmed(&def.description),
        icon: super::format::clean_icon(def.icon.as_deref()),
        version: trimmed(&def.version),
        game_version: trimmed(&def.game_version),
        uploader: None,
        disabled_entries: Some(disabled),
        include_mod_status: Some(sources.iter().any(|s| s.modlist.has_status())),
        include_mod_versions: Some(sources.iter().any(|s| s.modlist.has_versions())),
        mods: Some(mods),
        configurations: (!configurations.is_empty()).then_some(configurations),
        exclusive: Some(true),
        online_subscription: None,
    };
    MergeResult { modlist, conflicts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modlists::format::parse_modlist;

    fn src(r: &str, json: &str) -> GroupSource {
        GroupSource { reference: r.into(), display_name: r.into(), modlist: parse_modlist(json).unwrap() }
    }

    #[test]
    fn merges_and_picks_newest_by_default() {
        let a = src("a.json", r#"{"mods":[{"modId":"x","version":"1.0.0","isActive":true},{"modId":"y","version":"2.0.0"}]}"#);
        let b = src("b.json", r#"{"mods":[{"modId":"X","version":"1.2.0","isActive":false},{"modId":"y","version":"2.0.0"},{"modId":"z"}]}"#);
        let r = merge(&GroupDefinition { name: Some("G".into()), ..Default::default() }, &[a.clone(), b.clone()]);
        let ids: Vec<_> = r.modlist.entries().iter().map(|m| (m.id().unwrap().to_string(), m.version().map(String::from))).collect();
        assert_eq!(ids, [("x".into(), Some("1.2.0".into())), ("y".into(), Some("2.0.0".into())), ("z".into(), None)]);
        assert_eq!(r.conflicts.len(), 1, "identical y is not a conflict");
        assert_eq!(r.conflicts[0].selected, "b.json");
        assert!(!r.conflicts[0].resolved_by_user);
        assert_eq!(r.modlist.exclusive, Some(true));

        // A saved resolution wins.
        let mut def = GroupDefinition::default();
        def.conflict_resolutions.insert("X".into(), "A.JSON".into());
        let r = merge(&def, &[a, b]);
        assert_eq!(r.modlist.entries()[0].version(), Some("1.0.0"));
        assert!(r.conflicts[0].resolved_by_user);
        assert_eq!(r.modlist.display_name(), Some("Unnamed Modlist Group"));
    }

    #[test]
    fn config_breaks_version_ties_and_travels_with_choice() {
        let a = src("a", r#"{"mods":[{"modId":"x","version":"1.0.0"}]}"#);
        let b = src("b", r#"{"mods":[{"modId":"x","version":"1.0.0"}],"configurations":[{"modId":"x","fileName":"x.json","content":"{}"}]}"#);
        let r = merge(&GroupDefinition::default(), &[a, b]);
        assert_eq!(r.conflicts[0].selected, "b");
        assert_eq!(r.modlist.config_files().len(), 1);
    }

    #[test]
    fn references() {
        let g = Path::new("/m/Modlists/g.svsmgroup");
        assert_eq!(create_reference(g, Path::new("/m/Modlists/Rebuilt/a.json")), "Rebuilt/a.json");
        assert_eq!(create_reference(g, Path::new("/other/b.json")), Path::new("/other/b.json").display().to_string());
        assert_eq!(resolve_reference(g, "Rebuilt\\a.json"), Path::new("/m/Modlists/Rebuilt/a.json"));
        assert_eq!(resolve_reference(g, "../Presets/p.json"), Path::new("/m/Presets/p.json"));
        assert!(references_equal("Rebuilt\\A.json", "rebuilt/a.json"));
    }

    #[test]
    fn group_file_round_trip() {
        let g = parse_group(r#"{"SchemaVersion":1,"Name":"G","Members":["a.json"],"ConflictResolutions":{"x":"a.json"}}"#).unwrap();
        assert_eq!(g.members, ["a.json"]);
        assert_eq!(parse_group(&group_to_json(&g)).unwrap(), g);
        assert!(group_to_json(&g).contains("\"conflictResolutions\""));
    }
}
