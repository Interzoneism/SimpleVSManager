//! Load-status checks: duplicates and dependencies. Port of
//! `ModDiscoveryService.ApplyLoadStatuses` (full recompute; it's fast enough for 1000s of mods).

use std::collections::HashMap;

use super::model::{Dependency, InstalledMod};
use crate::version;

pub const GENERAL_LOAD_ERROR: &str = "Unable to load mod. Check log files.";
pub const DEPENDENCY_ERROR: &str =
    "Unable to load mod. A dependency has an error. Make sure they all load correctly.";

/// Fills `load_error`, `missing_dependencies`, `dependency_has_errors` and `duplicates`.
///
/// `is_disabled(mod_id, version)` reports whether a mod is disabled in clientsettings.json.
pub fn apply_load_statuses(mods: &mut [InstalledMod], is_disabled: impl Fn(&str, Option<&str>) -> bool) {
    for m in mods.iter_mut() {
        m.load_error = None;
        m.dependency_has_errors = false;
        m.missing_dependencies.clear();
        m.duplicates.clear();
    }

    // Group by lowercase mod id, keeping discovery order.
    let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, m) in mods.iter().enumerate() {
        if !m.mod_id.trim().is_empty() {
            by_id.entry(m.mod_id.to_lowercase()).or_default().push(i);
        }
    }

    // The "primary" copy is the first one without errors (else the first). The others won't load.
    let mut primary: HashMap<String, usize> = HashMap::new();
    for (id, idxs) in &by_id {
        let p = idxs.iter().copied().find(|&i| !mods[i].has_errors()).unwrap_or(idxs[0]);
        primary.insert(id.clone(), p);
        if idxs.len() > 1 {
            let paths: Vec<String> = idxs.iter().map(|&i| mods[i].source_path.clone()).collect();
            for &i in idxs {
                mods[i].duplicates = paths.iter().filter(|p| **p != mods[i].source_path).cloned().collect();
                if i != p {
                    mods[i].load_error = Some(GENERAL_LOAD_ERROR.to_string());
                }
            }
        }
    }

    // Snapshot provider facts before mutating dependents.
    struct Provider {
        broken: bool,
        version: Option<String>,
    }
    let providers: HashMap<String, Provider> = primary
        .iter()
        .map(|(id, &i)| {
            let m = &mods[i];
            let broken = m.has_errors() || m.has_load_error() || is_disabled(&m.mod_id, m.version.as_deref());
            (id.clone(), Provider { broken, version: m.version.clone() })
        })
        .collect();

    for m in mods.iter_mut() {
        if m.has_errors() || m.has_load_error() || m.dependencies.is_empty() {
            continue;
        }
        let mut missing: Vec<Dependency> = Vec::new();
        let mut dependency_broken = false;
        for dep in &m.dependencies {
            if dep.is_game_or_core() {
                continue;
            }
            match providers.get(&dep.mod_id.to_lowercase()) {
                None => missing.push(dep.clone()),
                Some(p) if p.broken => {
                    dependency_broken = true;
                    break;
                }
                Some(p) => {
                    if !version::satisfies_minimum_version(Some(&dep.version), p.version.as_deref()) {
                        missing.push(dep.clone());
                    }
                }
            }
        }
        if dependency_broken {
            m.dependency_has_errors = true;
            m.load_error = Some(DEPENDENCY_ERROR.to_string());
        } else if !missing.is_empty() {
            m.load_error = Some(missing_message(&missing));
            m.missing_dependencies = missing;
        }
    }
}

fn missing_message(deps: &[Dependency]) -> String {
    let fmt = |d: &Dependency| {
        let v = d.version.trim();
        if v.is_empty() || v == "*" {
            d.mod_id.clone()
        } else {
            format!("{} v{}", d.mod_id, v)
        }
    };
    if deps.len() == 1 {
        format!("Unable to load mod. Requires dependency {}", fmt(&deps[0]))
    } else {
        format!(
            "Unable to load mod. Requires dependencies {}",
            deps.iter().map(fmt).collect::<Vec<_>>().join(", ")
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mods::ModSourceKind;

    pub fn m(id: &str, ver: &str, deps: &[(&str, &str)]) -> InstalledMod {
        InstalledMod {
            key: format!("/mods/{id}-{ver}.zip"),
            mod_id: id.into(),
            name: id.into(),
            manifest_name: None,
            version: Some(ver.into()),
            network_version: None,
            description: None,
            website: None,
            authors: vec![],
            contributors: vec![],
            dependencies: deps.iter().map(|(i, v)| Dependency { mod_id: (*i).into(), version: (*v).into() }).collect(),
            side: None,
            required_on_client: None,
            required_on_server: None,
            source_path: format!("/mods/{id}-{ver}.zip"),
            source_kind: ModSourceKind::Zip,
            file_name: format!("{id}-{ver}.zip"),
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

    #[test]
    fn missing_and_too_old_dependencies() {
        let mut mods = vec![
            m("a", "1.0", &[("game", "1.21.0"), ("lib", "2.0"), ("gone", "*")]),
            m("lib", "1.5", &[]),
        ];
        apply_load_statuses(&mut mods, |_, _| false);
        let names: Vec<_> = mods[0].missing_dependencies.iter().map(|d| d.mod_id.as_str()).collect();
        assert_eq!(names, ["lib", "gone"]);
        assert_eq!(
            mods[0].load_error.as_deref(),
            Some("Unable to load mod. Requires dependencies lib v2.0, gone")
        );
        assert!(mods[1].load_error.is_none());
    }

    #[test]
    fn disabled_dependency_is_an_error() {
        let mut mods = vec![m("a", "1.0", &[("lib", "*")]), m("lib", "1.0", &[])];
        apply_load_statuses(&mut mods, |id, _| id == "lib");
        assert!(mods[0].dependency_has_errors);
        assert_eq!(mods[0].load_error.as_deref(), Some(DEPENDENCY_ERROR));
    }

    #[test]
    fn duplicates_flag_all_but_primary() {
        let mut mods = vec![m("x", "1.0", &[]), m("X", "2.0", &[])];
        apply_load_statuses(&mut mods, |_, _| false);
        assert!(mods[0].load_error.is_none());
        assert_eq!(mods[1].load_error.as_deref(), Some(GENERAL_LOAD_ERROR));
        assert_eq!(mods[0].duplicates, ["/mods/X-2.0.zip"]);
        assert_eq!(mods[1].duplicates, ["/mods/x-1.0.zip"]);
    }

    #[test]
    fn broken_primary_is_skipped() {
        let mut mods = vec![m("x", "1.0", &[]), m("x", "2.0", &[])];
        mods[0].error = Some("bad zip".into());
        apply_load_statuses(&mut mods, |_, _| false);
        assert!(mods[1].load_error.is_none());
        assert_eq!(mods[0].load_error.as_deref(), Some(GENERAL_LOAD_ERROR));
    }
}
