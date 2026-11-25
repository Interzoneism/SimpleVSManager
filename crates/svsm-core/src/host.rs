//! A snapshot of the host machine's well-known locations.
//!
//! Path resolution never calls `dirs` or `std::env` directly; it reads a [`HostEnv`]. The app
//! builds one with [`HostEnv::current`], tests build one by hand around a temp directory.

use std::path::PathBuf;

use crate::platform::Os;

#[derive(Debug, Clone)]
pub struct HostEnv {
    pub os: Os,
    /// User home directory.
    pub home: Option<PathBuf>,
    /// Roaming config dir: `%APPDATA%` on Windows, `~/Library/Application Support` on macOS,
    /// `$XDG_CONFIG_HOME` (default `~/.config`) on Linux. Vintage Story keeps its data here.
    pub config_dir: Option<PathBuf>,
    /// Local data dir: `%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS,
    /// `$XDG_DATA_HOME` (default `~/.local/share`) on Linux. The manager keeps its files here.
    pub data_local_dir: Option<PathBuf>,
    /// `C:\Program Files` and `C:\Program Files (x86)` on Windows; empty elsewhere.
    pub program_files: Vec<PathBuf>,
    /// Directory that contains the running manager executable.
    pub exe_dir: Option<PathBuf>,
    /// Process working directory.
    pub current_dir: Option<PathBuf>,
    /// Value of the `VINTAGE_STORY` environment variable, which the game's own tooling uses.
    pub vintage_story_env: Option<PathBuf>,
}

impl HostEnv {
    /// Reads the real machine.
    pub fn current() -> Self {
        let os = Os::current();
        let program_files = if os == Os::Windows {
            ["ProgramFiles", "ProgramFiles(x86)"]
                .iter()
                .filter_map(|k| std::env::var_os(k).map(PathBuf::from))
                .collect()
        } else {
            Vec::new()
        };

        Self {
            os,
            home: dirs::home_dir(),
            config_dir: dirs::config_dir(),
            data_local_dir: dirs::data_local_dir(),
            program_files,
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from)),
            current_dir: std::env::current_dir().ok(),
            vintage_story_env: std::env::var_os("VINTAGE_STORY")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        }
    }
}
