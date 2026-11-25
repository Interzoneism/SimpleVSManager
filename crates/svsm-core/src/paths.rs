//! Resolution of the three folders the manager cares about:
//!
//! * the **manager folder** (settings, modlists, caches),
//! * the game's **data folder** (`VintagestoryData`: mods, clientsettings.json, logs, saves),
//! * the **game folder** (where the executable lives).
//!
//! Ports `DevConfig.GetManagerDirectory`, `CustomConfigFolderManager`, `DataDirectoryLocator`
//! and `GameDirectoryLocator`, generalised for macOS and Linux.

use std::path::{Path, PathBuf};

use crate::host::HostEnv;
use crate::platform::Os;

pub const MANAGER_FOLDER_NAME: &str = "Simple VS Manager";
pub const MANAGER_FOLDER_NAME_LINUX: &str = "simple-vs-manager";
pub const CUSTOM_FOLDER_POINTER_FILE: &str = "CustomConfigFolder.txt";
pub const DATA_FOLDER_NAME: &str = "VintagestoryData";

/// Where the manager folder came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerDirKind {
    /// `CustomConfigFolder.txt` next to the executable pointed at an existing folder.
    Custom,
    /// The OS default location.
    Default,
}

/// Resolves the manager folder.
///
/// Windows keeps the WPF location (`%LOCALAPPDATA%\Simple VS Manager`) so both apps share
/// modlists and backups. macOS uses `~/Library/Application Support/Simple VS Manager`,
/// Linux uses `$XDG_DATA_HOME/simple-vs-manager`.
pub fn resolve_manager_dir(host: &HostEnv) -> (PathBuf, ManagerDirKind) {
    if let Some(custom) = read_custom_folder_pointer(host) {
        return (custom, ManagerDirKind::Custom);
    }
    (default_manager_dir(host), ManagerDirKind::Default)
}

pub fn default_manager_dir(host: &HostEnv) -> PathBuf {
    let name = match host.os {
        Os::Linux => MANAGER_FOLDER_NAME_LINUX,
        _ => MANAGER_FOLDER_NAME,
    };
    host.data_local_dir
        .as_ref()
        .or(host.config_dir.as_ref())
        .or(host.home.as_ref())
        .or(host.exe_dir.as_ref())
        .map(|base| base.join(name))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Reads `CustomConfigFolder.txt` next to the executable, as the WPF app does.
fn read_custom_folder_pointer(host: &HostEnv) -> Option<PathBuf> {
    let file = host.exe_dir.as_ref()?.join(CUSTOM_FOLDER_POINTER_FILE);
    let text = std::fs::read_to_string(file).ok()?;
    let path = PathBuf::from(text.trim().trim_start_matches('\u{feff}'));
    (!path.as_os_str().is_empty() && path.is_dir()).then_some(path)
}

/// Where `CustomConfigFolder.txt` lives (next to the executable).
pub fn custom_folder_pointer_file(host: &HostEnv) -> Option<PathBuf> {
    host.exe_dir.as_ref().map(|d| d.join(CUSTOM_FOLDER_POINTER_FILE))
}

/// The manager folder a "Change folder…" to `parent` would use: `<parent>/Simple VS Manager`.
pub fn manager_dir_in(host: &HostEnv, parent: &Path) -> PathBuf {
    parent.join(match host.os {
        Os::Linux => MANAGER_FOLDER_NAME_LINUX,
        _ => MANAGER_FOLDER_NAME,
    })
}

/// Result of [`move_manager_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerMove {
    pub to: PathBuf,
    /// The old folder couldn't be removed completely (a file was in use); the copy is complete.
    pub old_folder_left: bool,
}

/// Moves the manager folder to `to` and points `CustomConfigFolder.txt` at it (or removes the
/// pointer when `to` is the default location), as 2.x's "Change manager config and cache
/// folder" does. Everything is copied first; the old folder is only deleted once the copy and
/// the pointer are in place, so a failure never leaves the settings half-moved.
pub fn move_manager_dir(host: &HostEnv, from: &Path, to: &Path) -> crate::error::CoreResult<ManagerMove> {
    use crate::error::CoreError;
    let invalid = |msg: &str| CoreError::io(to, std::io::Error::new(std::io::ErrorKind::InvalidInput, msg.to_string()));
    if same_dir(from, to) {
        return Err(invalid("That is already the manager folder."));
    }
    if to.starts_with(from) || (from.exists() && to.exists() && std::fs::canonicalize(to).ok().zip(std::fs::canonicalize(from).ok()).is_some_and(|(t, f)| t.starts_with(&f))) {
        return Err(invalid("The new folder can't be inside the current manager folder."));
    }
    if from.starts_with(to) {
        return Err(invalid("The new folder can't contain the current manager folder."));
    }
    let pointer = custom_folder_pointer_file(host).ok_or_else(|| invalid("The manager's own folder is unknown."))?;
    if from.is_dir() {
        copy_tree(from, to)?;
    } else {
        std::fs::create_dir_all(to).map_err(|e| CoreError::io(to, e))?;
    }
    if same_dir(to, &default_manager_dir(host)) {
        match std::fs::remove_file(&pointer) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io(&pointer, e)),
        }
    } else {
        std::fs::write(&pointer, to.display().to_string()).map_err(|e| CoreError::io(&pointer, e))?;
    }
    let old_folder_left = from.exists() && std::fs::remove_dir_all(from).is_err();
    Ok(ManagerMove { to: to.to_path_buf(), old_folder_left })
}

fn same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Copies a folder tree, replacing files that already exist at the destination.
fn copy_tree(from: &Path, to: &Path) -> crate::error::CoreResult<()> {
    use crate::error::CoreError;
    std::fs::create_dir_all(to).map_err(|e| CoreError::io(to, e))?;
    for entry in std::fs::read_dir(from).map_err(|e| CoreError::io(from, e))? {
        let entry = entry.map_err(|e| CoreError::io(from, e))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| CoreError::io(&src, e))?;
        if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else if kind.is_file() {
            std::fs::copy(&src, &dst).map_err(|e| CoreError::io(&src, e))?;
        }
    }
    Ok(())
}

/// Candidate data folders, most likely first. Mirrors the game's own lookup plus the WPF extras.
pub fn data_dir_candidates(host: &HostEnv) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(c) = &host.config_dir {
        out.push(c.join(DATA_FOLDER_NAME));
    }
    if host.os == Os::Windows {
        if let Some(l) = &host.data_local_dir {
            out.push(l.join(DATA_FOLDER_NAME));
        }
    }
    if let Some(exe) = &host.exe_dir {
        // Portable layout: data folder next to the manager.
        out.push(exe.join(DATA_FOLDER_NAME));
        out.push(exe.join("data"));
    }
    for root in host.os.extra_data_dir_roots(host) {
        out.push(root.join(DATA_FOLDER_NAME));
    }
    dedup(out)
}

/// Picks a data folder: the first candidate that exists, otherwise the OS default (which is
/// where the game will create it on first run).
pub fn resolve_data_dir(host: &HostEnv) -> Option<(PathBuf, bool)> {
    let candidates = data_dir_candidates(host);
    if let Some(found) = candidates.iter().find(|p| looks_like_data_dir(p)) {
        return Some((found.clone(), true));
    }
    candidates.into_iter().next().map(|p| (p, false))
}

/// A data folder is recognised by its `clientsettings.json` or `Mods` folder.
pub fn looks_like_data_dir(path: &Path) -> bool {
    path.join("clientsettings.json").is_file() || path.join("Mods").is_dir()
}

/// Candidate game folders, most likely first.
pub fn game_dir_candidates(host: &HostEnv) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(env) = &host.vintage_story_env {
        out.push(env.clone());
    }
    if let Some(exe) = &host.exe_dir {
        out.push(exe.clone());
        out.push(exe.join("VSFOLDER"));
    }
    if let Some(cwd) = &host.current_dir {
        out.push(cwd.clone());
    }
    out.extend(host.os.default_game_dirs(host));
    dedup(out)
}

/// Finds the first candidate that contains a game executable.
pub fn resolve_game_dir(host: &HostEnv) -> Option<(PathBuf, PathBuf)> {
    game_dir_candidates(host)
        .into_iter()
        .find_map(|dir| find_game_executable(host.os, &dir).map(|exe| (dir, exe)))
}

/// Looks for the game executable in `dir`.
pub fn find_game_executable(os: Os, dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    os.game_executable_candidates()
        .into_iter()
        .map(|rel| dir.join(rel))
        .find(|p| p.is_file())
}

fn dedup(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    // Compare paths exactly. Never case-fold: Linux and some macOS volumes are case-sensitive.
    paths.into_iter().filter(|p| seen.insert(p.clone())).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fake Linux-style host rooted in a temp dir.
    pub fn fake_host(root: &Path, os: Os) -> HostEnv {
        HostEnv {
            os,
            home: Some(root.join("home")),
            config_dir: Some(root.join("home/.config")),
            data_local_dir: Some(root.join("home/.local/share")),
            program_files: vec![],
            exe_dir: Some(root.join("app")),
            current_dir: Some(root.join("cwd")),
            vintage_story_env: None,
        }
    }

    #[test]
    fn manager_dir_defaults_per_os() {
        let t = tempfile::tempdir().unwrap();
        let linux = fake_host(t.path(), Os::Linux);
        assert_eq!(
            resolve_manager_dir(&linux),
            (t.path().join("home/.local/share/simple-vs-manager"), ManagerDirKind::Default)
        );
        let win = fake_host(t.path(), Os::Windows);
        assert_eq!(resolve_manager_dir(&win).0, t.path().join("home/.local/share/Simple VS Manager"));
    }

    #[test]
    fn manager_dir_custom_pointer() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let custom = t.path().join("my config");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::create_dir_all(host.exe_dir.as_ref().unwrap()).unwrap();
        std::fs::write(
            host.exe_dir.as_ref().unwrap().join(CUSTOM_FOLDER_POINTER_FILE),
            format!("\u{feff}{}\r\n", custom.display()),
        )
        .unwrap();
        assert_eq!(resolve_manager_dir(&host), (custom, ManagerDirKind::Custom));
    }

    #[test]
    fn manager_dir_pointer_to_missing_folder_is_ignored() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        std::fs::create_dir_all(host.exe_dir.as_ref().unwrap()).unwrap();
        std::fs::write(host.exe_dir.as_ref().unwrap().join(CUSTOM_FOLDER_POINTER_FILE), "/nope").unwrap();
        assert_eq!(resolve_manager_dir(&host).1, ManagerDirKind::Default);
    }

    #[test]
    fn data_dir_prefers_existing() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        // Nothing exists: default to ~/.config/VintagestoryData, marked as not found.
        assert_eq!(
            resolve_data_dir(&host),
            Some((t.path().join("home/.config/VintagestoryData"), false))
        );
        // A portable data folder next to the exe wins once it exists.
        let portable = t.path().join("app/VintagestoryData");
        std::fs::create_dir_all(portable.join("Mods")).unwrap();
        assert_eq!(resolve_data_dir(&host), Some((portable, true)));
    }

    #[test]
    fn game_dir_found_via_env_var() {
        let t = tempfile::tempdir().unwrap();
        let mut host = fake_host(t.path(), Os::Linux);
        let game = t.path().join("games/vs");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Vintagestory"), b"").unwrap();
        host.vintage_story_env = Some(game.clone());
        assert_eq!(resolve_game_dir(&host), Some((game.clone(), game.join("Vintagestory"))));
    }

    #[test]
    fn game_dir_default_linux_location() {
        let t = tempfile::tempdir().unwrap();
        let host = fake_host(t.path(), Os::Linux);
        let game = t.path().join("home/.local/share/vintagestory");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Vintagestory"), b"").unwrap();
        assert_eq!(resolve_game_dir(&host).map(|g| g.0), Some(game));
    }

    #[test]
    fn windows_exe_preferred_on_windows() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path();
        std::fs::write(dir.join("Vintagestory"), b"").unwrap();
        std::fs::write(dir.join("Vintagestory.exe"), b"").unwrap();
        assert_eq!(find_game_executable(Os::Windows, dir), Some(dir.join("Vintagestory.exe")));
        assert_eq!(find_game_executable(Os::Linux, dir), Some(dir.join("Vintagestory")));
    }

    #[test]
    fn moves_the_manager_folder_and_back() {
        let t = tempfile::tempdir().unwrap();
        let mut host = fake_host(t.path(), Os::Windows);
        host.exe_dir = Some(t.path().join("app"));
        std::fs::create_dir_all(t.path().join("app")).unwrap();
        let (from, kind) = resolve_manager_dir(&host);
        assert_eq!(kind, ManagerDirKind::Default);
        std::fs::create_dir_all(from.join("Modlists")).unwrap();
        std::fs::write(from.join("svsm3-settings.json"), "{}").unwrap();
        std::fs::write(from.join("Modlists/a.json"), "x").unwrap();

        let to = manager_dir_in(&host, &t.path().join("D"));
        assert!(move_manager_dir(&host, &from, &from.join("sub")).is_err(), "no moving into itself");
        let moved = move_manager_dir(&host, &from, &to).unwrap();
        assert!(!moved.old_folder_left);
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(to.join("Modlists/a.json")).unwrap(), "x");
        assert_eq!(resolve_manager_dir(&host), (to.clone(), ManagerDirKind::Custom));

        // Back to the default: the pointer goes away.
        move_manager_dir(&host, &to, &from).unwrap();
        assert!(!custom_folder_pointer_file(&host).unwrap().exists());
        assert_eq!(resolve_manager_dir(&host), (from.clone(), ManagerDirKind::Default));
        assert!(from.join("svsm3-settings.json").is_file());
    }
}
