//! Detects the installed Vintage Story version.
//!
//! The WPF app used `FileVersionInfo`, which only works on Windows. The game is a .NET app on
//! every OS, so its assemblies (`VintagestoryAPI.dll`, `Vintagestory.dll`) are PE files with a
//! Win32 version resource everywhere. We read that resource directly with `pelite`, which works
//! on any OS. Native launchers (ELF on Linux, Mach-O on macOS) aren't PE, so they're tried and
//! skipped.

use std::path::{Path, PathBuf};

use pelite::resources::version_info::VersionInfo;

use crate::error::{CoreError, CoreResult};
use crate::version;

/// Files inside a game directory that may carry the version, in the order they're tried.
const CANDIDATES: &[&str] = &[
    "Vintagestory.exe",
    "VintagestoryAPI.dll",
    "Vintagestory.dll",
    "VintagestoryServer.exe",
    "VintagestoryServer.dll",
];

/// Reads the normalized file version (falling back to product version) from a PE file.
pub fn read_pe_version(path: &Path) -> CoreResult<String> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
    let no_info = || CoreError::NoVersionInfo(path.to_path_buf());

    let pe = pelite::PeFile::from_bytes(&bytes).map_err(|_| no_info())?;
    let resources = pe.resources().map_err(|_| no_info())?;
    let info: VersionInfo = resources.version_info().map_err(|_| no_info())?;

    // Prefer the string table (FileVersion, then ProductVersion), like FileVersionInfo in the
    // WPF app: it says "1.21.0" where the fixed block says "1.21.0.0".
    for key in ["FileVersion", "ProductVersion"] {
        for lang in info.translation() {
            if let Some(value) = info.value(*lang, key) {
                if let Some(n) = version::normalize(&value) {
                    return Ok(n);
                }
            }
        }
    }

    // Fall back to the numeric fixed-info block.
    if let Some(fixed) = info.fixed() {
        let v = fixed.dwFileVersion;
        let raw = format!("{}.{}.{}.{}", v.Major, v.Minor, v.Patch, v.Build);
        if raw != "0.0.0.0" {
            if let Some(n) = version::normalize(&raw) {
                return Ok(n);
            }
        }
    }

    Err(no_info())
}

/// Finds the game version for a game directory (or a direct path to an executable/DLL).
pub fn detect_game_version(game_path: &Path) -> Option<String> {
    candidate_files(game_path)
        .into_iter()
        .find_map(|file| read_pe_version(&file).ok())
}

fn candidate_files(game_path: &Path) -> Vec<PathBuf> {
    if game_path.is_file() {
        let mut files = vec![game_path.to_path_buf()];
        // A native launcher sits next to the managed assemblies; look beside it too.
        if let Some(dir) = game_path.parent() {
            files.extend(CANDIDATES.iter().map(|c| dir.join(c)));
        }
        return files;
    }

    let mut files: Vec<PathBuf> = CANDIDATES.iter().map(|c| game_path.join(c)).collect();

    // macOS app bundles: the assemblies' location inside the bundle isn't pinned down yet, so
    // look a few levels into any *.app for VintagestoryAPI.dll.
    if let Ok(entries) = std::fs::read_dir(game_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) && p.is_dir() {
                find_named(&p, "VintagestoryAPI.dll", 4, &mut files);
            }
        }
    }
    if game_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) {
        find_named(game_path, "VintagestoryAPI.dll", 4, &mut files);
    }
    files
}

fn find_named(dir: &Path, name: &str, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if depth > 0 {
                find_named(&p, name, depth - 1, out);
            }
        } else if p.file_name().is_some_and(|f| f == name) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_pe_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("Vintagestory");
        std::fs::write(&f, b"\x7fELF not really").unwrap();
        assert!(read_pe_version(&f).is_err());
        assert_eq!(detect_game_version(dir.path()), None);
    }

    #[test]
    fn missing_dir_is_none() {
        assert_eq!(detect_game_version(Path::new("/definitely/not/here")), None);
    }
}
