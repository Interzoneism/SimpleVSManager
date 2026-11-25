//! Installed mods: discovery, manifest parsing and load-status checks.

mod deps;
mod discovery;
mod model;
mod modinfo;
mod status;

pub use deps::{plan_dependency_fixes, release_for_dependency, DependencyAction, DependencyPlan, DependencyProblem};
pub use discovery::{discover_mods, search_paths, DiscoveryResult};
pub use model::{Dependency, InstalledMod, ModSourceKind};
pub use modinfo::{parse_modinfo, to_mod_id, ModInfo};
pub use status::apply_load_statuses;

/// Base-game "mods" that live in the game folder and must never be listed or touched.
pub fn is_base_game_mod(mod_id: &str) -> bool {
    ["VSCreativeMod", "VSEssentials", "VSSurvivalMod"]
        .iter()
        .any(|b| b.eq_ignore_ascii_case(mod_id))
}

use std::path::Path;

use crate::error::{CoreError, CoreResult};

/// Moves a mod to the system trash (Recycle Bin, macOS Trash, freedesktop trash).
///
/// Refuses anything that isn't a direct child of one of `search_paths`, so a bad key from the
/// UI can never delete something else.
pub fn delete_mod(source_path: &Path, search_paths: &[std::path::PathBuf]) -> CoreResult<()> {
    let parent = source_path.parent().unwrap_or(Path::new(""));
    let allowed = search_paths.iter().any(|sp| sp == parent);
    if !allowed || !source_path.exists() {
        return Err(CoreError::io(
            source_path,
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not an installed mod"),
        ));
    }
    trash::delete(source_path).map_err(|e| CoreError::io(source_path, std::io::Error::other(e.to_string())))
}

#[cfg(test)]
mod delete_tests {
    use super::*;

    #[test]
    fn refuses_paths_outside_mod_folders() {
        let t = tempfile::tempdir().unwrap();
        let outside = t.path().join("important.txt");
        std::fs::write(&outside, "x").unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        assert!(delete_mod(&outside, &[mods]).is_err());
        assert!(outside.exists());
    }
}

/// Splits `name.immbackup` / `name.immbackup2` into `name`.
fn parked_original(name: &str) -> Option<&str> {
    let i = name.rfind(".immbackup")?;
    let rest = &name[i + ".immbackup".len()..];
    (i > 0 && rest.chars().all(|c| c.is_ascii_digit())).then(|| &name[..i])
}

/// `.name.svsm-extract` / `.name.svsm-extract2`: a staging folder of a folder-mod update.
fn is_extract_staging(name: &str) -> bool {
    name.starts_with('.') && name.rfind(".svsm-extract").is_some_and(|i| name[i + ".svsm-extract".len()..].chars().all(|c| c.is_ascii_digit()))
}

/// Cleans up after an update that was interrupted (crash, power cut) in these mod folders:
///
/// * `X.immbackup` is the old version parked during an update. If `X` is missing, the new copy
///   never landed, so the old one is put back. Otherwise the update finished and the parked
///   copy goes to the trash.
/// * `.X.svsm-extract` is a half-extracted folder mod, which the game would load next to the
///   real one. It is removed.
///
/// Returns one line per fix, for the log. Never call it while an update is running.
pub fn recover_interrupted_installs(dirs: &[std::path::PathBuf]) -> Vec<String> {
    let mut fixed = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(original) = parked_original(&name) {
                let target = dir.join(original);
                if !target.exists() {
                    match std::fs::rename(&path, &target) {
                        Ok(()) => fixed.push(format!("Put back {} after an interrupted update", target.display())),
                        Err(e) => fixed.push(format!("Could not put back {}: {e}", path.display())),
                    }
                } else if trash::delete(&path).is_ok() {
                    fixed.push(format!("Moved the leftover {} to the trash", path.display()));
                }
            } else if is_extract_staging(&name) && path.is_dir() {
                match std::fs::remove_dir_all(&path) {
                    Ok(()) => fixed.push(format!("Removed the half-extracted {}", path.display())),
                    Err(e) => fixed.push(format!("Could not remove {}: {e}", path.display())),
                }
            }
        }
    }
    fixed
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(parked_original("CarryOn.zip.immbackup"), Some("CarryOn.zip"));
        assert_eq!(parked_original("CarryOn.zip.immbackup3"), Some("CarryOn.zip"));
        assert_eq!(parked_original("CarryOn.zip.immbackupx"), None);
        assert_eq!(parked_original(".immbackup"), None);
        assert!(is_extract_staging(".mymod.svsm-extract"));
        assert!(is_extract_staging(".mymod.svsm-extract2"));
        assert!(!is_extract_staging("mymod.svsm-extract"));
    }

    #[test]
    fn puts_back_parked_mods_and_removes_staging() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(mods.join(".folder.svsm-extract/inner")).unwrap();
        std::fs::write(mods.join("Lost.zip.immbackup"), "old").unwrap();
        std::fs::write(mods.join("Keep.zip"), "new").unwrap();
        std::fs::write(mods.join("Other.zip"), "x").unwrap();
        let fixed = recover_interrupted_installs(std::slice::from_ref(&mods));
        assert_eq!(std::fs::read_to_string(mods.join("Lost.zip")).unwrap(), "old");
        assert!(!mods.join("Lost.zip.immbackup").exists());
        assert!(!mods.join(".folder.svsm-extract").exists());
        assert_eq!(std::fs::read_to_string(mods.join("Keep.zip")).unwrap(), "new");
        assert!(mods.join("Other.zip").exists());
        assert_eq!(fixed.len(), 2, "{fixed:?}");
    }
}
