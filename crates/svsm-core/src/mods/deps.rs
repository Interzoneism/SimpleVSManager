//! Dependency solver: works out what to install, update or enable so a mod's dependencies load.
//! Port of 2.x `MainWindow.FixModButton_OnClick` / `ResolveDependencyTreeAsync`.
//!
//! Planning is pure. The caller runs the actions, rescans, and plans again, because a freshly
//! installed dependency's own dependencies are only known once its modinfo.json is on disk.

use std::collections::HashSet;

use serde::Serialize;

use super::model::{Dependency, InstalledMod, ModSourceKind};
use crate::moddb::{ModDbInfo, ModDbRelease};
use crate::version;

/// One step towards satisfying a mod's dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DependencyAction {
    /// Not installed: download it from the Mod DB.
    Install { mod_id: String, min_version: String },
    /// Installed but older than required: update it in place.
    Update { key: String, mod_id: String, from_version: Option<String>, min_version: String },
    /// Installed but disabled in clientsettings.json.
    Enable { key: String, mod_id: String },
}

impl DependencyAction {
    pub fn mod_id(&self) -> &str {
        match self {
            Self::Install { mod_id, .. } | Self::Update { mod_id, .. } | Self::Enable { mod_id, .. } => mod_id,
        }
    }

    /// Identifies the action across replans, so a step that didn't help isn't retried forever.
    pub fn attempt_key(&self) -> String {
        match self {
            Self::Install { mod_id, min_version } => format!("install:{}:{min_version}", mod_id.to_lowercase()),
            Self::Update { mod_id, min_version, .. } => format!("update:{}:{min_version}", mod_id.to_lowercase()),
            Self::Enable { key, .. } => format!("enable:{key}"),
        }
    }
}

/// A dependency that can't be fixed automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DependencyProblem {
    pub mod_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyPlan {
    pub actions: Vec<DependencyAction>,
    pub problems: Vec<DependencyProblem>,
}

impl DependencyPlan {
    pub fn is_satisfied(&self) -> bool {
        self.actions.is_empty() && self.problems.is_empty()
    }
}

/// The copy of `mod_id` the game loads: the first one without read errors, else the first.
/// Same rule as `apply_load_statuses`.
fn provider<'a>(mods: &'a [InstalledMod], mod_id: &str) -> Option<&'a InstalledMod> {
    let mut copies = mods.iter().filter(|m| m.mod_id.eq_ignore_ascii_case(mod_id));
    let first = copies.clone().next()?;
    Some(copies.find(|m| !m.has_errors()).unwrap_or(first))
}

fn describe(dep: &Dependency) -> String {
    let v = dep.version.trim();
    if v.is_empty() || v == "*" {
        dep.mod_id.clone()
    } else {
        format!("{} v{}", dep.mod_id, v)
    }
}

/// Walks the dependency tree of the mod with `root_key` and lists what needs doing.
/// Returns `None` if that mod isn't installed.
pub fn plan_dependency_fixes(mods: &[InstalledMod], root_key: &str) -> Option<DependencyPlan> {
    let root = mods.iter().find(|m| m.key == root_key)?;
    let mut plan = DependencyPlan::default();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(root.mod_id.to_lowercase());
    walk(mods, &root.dependencies, &mut visited, &mut plan);
    Some(plan)
}

fn walk(mods: &[InstalledMod], deps: &[Dependency], visited: &mut HashSet<String>, plan: &mut DependencyPlan) {
    for dep in deps {
        if dep.is_game_or_core() || dep.mod_id.trim().is_empty() {
            continue;
        }
        if !visited.insert(dep.mod_id.to_lowercase()) {
            continue;
        }
        let Some(p) = provider(mods, &dep.mod_id) else {
            plan.actions.push(DependencyAction::Install { mod_id: dep.mod_id.clone(), min_version: dep.version.clone() });
            continue;
        };
        if !version::satisfies_minimum_version(Some(&dep.version), p.version.as_deref()) {
            match p.source_kind {
                ModSourceKind::Zip | ModSourceKind::Folder => plan.actions.push(DependencyAction::Update {
                    key: p.key.clone(),
                    mod_id: p.mod_id.clone(),
                    from_version: p.version.clone(),
                    min_version: dep.version.clone(),
                }),
                _ => plan.problems.push(DependencyProblem {
                    mod_id: dep.mod_id.clone(),
                    message: format!(
                        "Needs {}, but the installed copy is a loose file that can't be updated automatically.",
                        describe(dep)
                    ),
                }),
            }
            // Its dependencies may change with the new version: the next plan looks again.
            continue;
        }
        if p.has_errors() {
            plan.problems.push(DependencyProblem {
                mod_id: dep.mod_id.clone(),
                message: format!("{} can't be read: {}", p.name, p.error.as_deref().unwrap_or_default()),
            });
            continue;
        }
        if !p.is_active {
            plan.actions.push(DependencyAction::Enable { key: p.key.clone(), mod_id: p.mod_id.clone() });
        }
        walk(mods, &p.dependencies, visited, plan);
    }
}

/// The newest release that supports the target game version and is at least `min_version`.
pub fn release_for_dependency<'a>(info: &'a ModDbInfo, min_version: &str) -> Option<&'a ModDbRelease> {
    info.releases
        .iter()
        .find(|r| r.compatible && version::satisfies_minimum_version(Some(min_version), Some(&r.version)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::status::tests::m;

    fn plan(mods: &[InstalledMod], root: &str) -> DependencyPlan {
        let key = mods.iter().find(|x| x.mod_id == root).unwrap().key.clone();
        plan_dependency_fixes(mods, &key).unwrap()
    }

    #[test]
    fn installs_missing_and_updates_outdated() {
        let mods = vec![
            m("a", "1.0", &[("game", "1.21.0"), ("carryonlib", "1.0.0-rc.1"), ("lib", "2.0")]),
            m("lib", "1.5", &[]),
        ];
        let p = plan(&mods, "a");
        assert_eq!(
            p.actions,
            [
                DependencyAction::Install { mod_id: "carryonlib".into(), min_version: "1.0.0-rc.1".into() },
                DependencyAction::Update {
                    key: "/mods/lib-1.5.zip".into(),
                    mod_id: "lib".into(),
                    from_version: Some("1.5".into()),
                    min_version: "2.0".into()
                },
            ]
        );
        assert!(p.problems.is_empty());
    }

    #[test]
    fn enables_disabled_and_walks_nested() {
        let mut mods = vec![m("a", "1.0", &[("mid", "*")]), m("mid", "1.0", &[("leaf", "1.0")]), m("leaf", "1.0", &[])];
        mods[1].is_active = false;
        mods[2].is_active = false;
        let p = plan(&mods, "a");
        let ids: Vec<_> = p.actions.iter().map(|a| a.mod_id()).collect();
        assert_eq!(ids, ["mid", "leaf"]);
        assert!(p.actions.iter().all(|a| matches!(a, DependencyAction::Enable { .. })));
    }

    #[test]
    fn satisfied_tree_is_empty_and_cycles_stop() {
        let mods = vec![m("a", "1.0", &[("b", "1.0")]), m("b", "1.0", &[("a", "*")])];
        assert!(plan(&mods, "a").is_satisfied());
    }

    #[test]
    fn broken_and_loose_dependencies_are_problems() {
        let mut mods = vec![m("a", "1.0", &[("bad", "*"), ("loose", "2.0")]), m("bad", "1.0", &[]), m("loose", "1.0", &[])];
        mods[1].error = Some("bad zip".into());
        mods[2].source_kind = ModSourceKind::Assembly;
        let p = plan(&mods, "a");
        assert!(p.actions.is_empty());
        let ids: Vec<_> = p.problems.iter().map(|x| x.mod_id.as_str()).collect();
        assert_eq!(ids, ["bad", "loose"]);
    }

    #[test]
    fn picks_newest_compatible_release_meeting_minimum() {
        let rel = |v: &str, compatible: bool| ModDbRelease {
            version: v.into(),
            download_url: String::new(),
            file_name: None,
            game_versions: vec![],
            compatible,
            changelog: None,
            downloads: None,
            created_ms: None,
        };
        let info = ModDbInfo {
            mod_id: "lib".into(),
            asset_id: None,
            name: None,
            author: None,
            page_url: None,
            logo_url: None,
            side: None,
            tags: vec![],
            downloads: None,
            follows: None,
            comments: None,
            trending_points: None,
            created_ms: None,
            last_released_ms: None,
            homepage_url: None,
            source_code_url: None,
            issue_tracker_url: None,
            wiki_url: None,
            releases: vec![rel("3.0", false), rel("2.1", true), rel("1.0", true)],
            latest_compatible_index: Some(1),
            fetched_ms: 0.0,
        };
        assert_eq!(release_for_dependency(&info, "2.0").map(|r| r.version.as_str()), Some("2.1"));
        assert_eq!(release_for_dependency(&info, "*").map(|r| r.version.as_str()), Some("2.1"));
        assert!(release_for_dependency(&info, "2.5").is_none());
    }
}
