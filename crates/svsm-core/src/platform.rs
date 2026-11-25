//! Operating-system specific knowledge. Keep every `cfg!(target_os)` decision in this module.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Names of the macOS app bundle. "Vintage Story.app" is a guess; harmless if it doesn't exist.
pub const MAC_BUNDLE_NAMES: &[&str] = &["Vintagestory.app", "Vintage Story.app"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Linux
        }
    }

    /// Game executables to look for inside a game directory, most specific first.
    ///
    /// The list is the same on every OS (as in the WPF app) so a Windows install on a shared
    /// drive is still recognised from Linux, but the native one comes first.
    pub fn game_executable_candidates(self) -> Vec<PathBuf> {
        let windows = PathBuf::from("Vintagestory.exe");
        let unix = PathBuf::from("Vintagestory");
        let mac_bundle: PathBuf = ["Vintagestory.app", "Contents", "MacOS", "Vintagestory"].iter().collect();
        // Bundle name with a space is a guess; harmless if it doesn't exist.
        let mac_bundle_spaced: PathBuf =
            ["Vintage Story.app", "Contents", "MacOS", "Vintagestory"].iter().collect();
        // The game folder is the bundle itself.
        let in_bundle: PathBuf = ["Contents", "MacOS", "Vintagestory"].iter().collect();
        match self {
            Os::Windows => vec![windows, unix, in_bundle, mac_bundle, mac_bundle_spaced],
            Os::Macos => vec![in_bundle, mac_bundle, mac_bundle_spaced, unix, windows],
            Os::Linux => vec![unix, windows, in_bundle, mac_bundle, mac_bundle_spaced],
        }
    }

    /// Typical install locations, used only when nothing better is known.
    ///
    /// The macOS and Linux entries are best guesses and still need checking against real
    /// installs (tarball installer, Flatpak, AUR, macOS app bundle).
    pub fn default_game_dirs(self, host: &crate::HostEnv) -> Vec<PathBuf> {
        let mut out = Vec::new();
        match self {
            Os::Windows => {
                for root in host.program_files.iter() {
                    out.push(root.join("Vintagestory"));
                }
                if let Some(c) = &host.config_dir {
                    out.push(c.join("Vintagestory")); // default installer target: %APPDATA%\Vintagestory
                }
                if let Some(l) = &host.data_local_dir {
                    out.push(l.join("Vintagestory"));
                }
                for root in [r"C:\Games", r"D:\Games"] {
                    out.push(Path::new(root).join("Vintagestory"));
                }
            }
            Os::Macos => {
                // The bundle is the game folder. Never `/Applications` itself: everything
                // inside it would be searched.
                let mut roots = vec![PathBuf::from("/Applications")];
                if let Some(h) = &host.home {
                    roots.push(h.join("Applications"));
                }
                for root in roots {
                    for bundle in MAC_BUNDLE_NAMES {
                        out.push(root.join(bundle));
                    }
                }
            }
            Os::Linux => {
                if let Some(d) = &host.data_local_dir {
                    out.push(d.join("vintagestory"));
                    out.push(d.join("Vintagestory"));
                }
                if let Some(h) = &host.home {
                    out.push(h.join("vintagestory"));
                    out.push(h.join("ApplicationData").join("vintagestory"));
                    out.push(h.join(".local").join("share").join("vintagestory"));
                }
                out.push(PathBuf::from("/opt/vintagestory"));
                out.push(PathBuf::from("/usr/share/vintagestory"));
                out.push(PathBuf::from("/usr/lib/vintagestory"));
            }
        }
        out
    }

    /// Where v3 keeps the pointer to a moved manager folder: a fixed per-user place, never next
    /// to the executable (read-only for installed apps, AppImages and signed macOS bundles).
    pub fn manager_pointer_file(self, host: &crate::HostEnv) -> Option<PathBuf> {
        const NAME: &str = "simple-vs-manager-folder.txt";
        match self {
            Os::Macos => host.home.as_ref().map(|h| h.join("Library").join("Preferences").join(NAME)),
            _ => host.config_dir.as_ref().or(host.home.as_ref()).map(|d| d.join(NAME)),
        }
    }

    /// Extra places the game's data folder can live besides `<config dir>/VintagestoryData`.
    pub fn extra_data_dir_roots(self, host: &crate::HostEnv) -> Vec<PathBuf> {
        match self {
            Os::Windows => [r"C:\Games", r"D:\Games"]
                .iter()
                .map(PathBuf::from)
                .chain(host.program_files.iter().cloned())
                .collect(),
            Os::Macos => Vec::new(),
            Os::Linux => host
                .home
                .iter()
                // Flatpak sandboxes keep XDG config under ~/.var/app/<id>/config. App id unverified.
                .map(|h| h.join(".var/app/at.vintagestory.VintageStory/config"))
                .collect(),
        }
    }
}
