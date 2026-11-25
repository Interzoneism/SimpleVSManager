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
