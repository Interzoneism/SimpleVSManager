//! Starting the game. Always passes `--dataPath` so the game uses the folder the manager is
//! showing (as the WPF app does).

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{CoreError, CoreResult};
use crate::platform::Os;

/// Environment variables an AppImage sets for its own bundled libraries. The game must not
/// inherit them or it may load our libraries instead of the system's.
const APPIMAGE_ENV: &[&str] = &["APPIMAGE", "APPDIR", "OWD", "ARGV0", "LD_LIBRARY_PATH", "LD_PRELOAD", "GDK_PIXBUF_MODULE_FILE", "GIO_MODULE_DIR", "GTK_PATH", "GTK_EXE_PREFIX", "GTK_DATA_PREFIX", "GSETTINGS_SCHEMA_DIR", "PYTHONHOME", "PERLLIB", "QT_PLUGIN_PATH", "XDG_DATA_DIRS"];

/// Builds the launch command for a game executable.
pub fn game_command(os: Os, executable: &Path, data_dir: &Path) -> Command {
    // macOS: start the bundle through LaunchServices (`open`), so Gatekeeper, the Dock and
    // App Translocation treat it like a normal app start.
    if os == Os::Macos {
        if let Some(bundle) = app_bundle_of(executable) {
            let mut cmd = Command::new("/usr/bin/open");
            cmd.arg("-a").arg(bundle).arg("--args").arg("--dataPath").arg(data_dir);
            cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            return cmd;
        }
    }
    let mut cmd = Command::new(executable);
    if let Some(dir) = executable.parent() {
        cmd.current_dir(dir);
    }
    cmd.arg("--dataPath").arg(data_dir);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

    if os == Os::Linux && std::env::var_os("APPIMAGE").is_some() {
        for key in APPIMAGE_ENV {
            cmd.env_remove(key);
        }
    }
    detach(&mut cmd);
    cmd
}

/// `X.app` for `X.app/Contents/MacOS/<exe>`.
fn app_bundle_of(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && bundle.extension()? .eq_ignore_ascii_case("app")).then_some(bundle)
}

/// Launches the game and returns without waiting. The child keeps running if we exit.
pub fn launch_game(os: Os, executable: &Path, data_dir: &Path) -> CoreResult<()> {
    if !executable.is_file() {
        return Err(CoreError::io(executable, std::io::Error::new(std::io::ErrorKind::NotFound, "game executable not found")));
    }
    std::fs::create_dir_all(data_dir).map_err(|e| CoreError::io(data_dir, e))?;
    game_command(os, executable, data_dir).spawn().map(|_| ()).map_err(|e| CoreError::io(executable, e))
}

/// Flathub id of the Vintage Story Flatpak.
pub const FLATPAK_ID: &str = "at.vintagestory.VintageStory";

/// Linux: whether the game is installed as a Flatpak (per user or system wide).
pub fn flatpak_installed(host: &crate::HostEnv) -> bool {
    if host.os != Os::Linux {
        return false;
    }
    let user = host.data_local_dir.as_ref().map(|d| d.join("flatpak/app").join(FLATPAK_ID));
    let system = std::path::PathBuf::from("/var/lib/flatpak/app").join(FLATPAK_ID);
    user.is_some_and(|p| p.is_dir()) || system.is_dir()
}

/// The command that starts the Flatpak with our data folder.
pub fn flatpak_command(data_dir: &Path) -> Command {
    let mut cmd = Command::new("flatpak");
    cmd.arg("run").arg(FLATPAK_ID).arg("--dataPath").arg(data_dir);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if std::env::var_os("APPIMAGE").is_some() {
        for key in APPIMAGE_ENV {
            cmd.env_remove(key);
        }
    }
    detach(&mut cmd);
    cmd
}

/// Starts the Flatpak version of the game.
pub fn launch_flatpak(data_dir: &Path) -> CoreResult<()> {
    std::fs::create_dir_all(data_dir).map_err(|e| CoreError::io(data_dir, e))?;
    flatpak_command(data_dir).spawn().map(|_| ()).map_err(|e| CoreError::io(Path::new("flatpak"), e))
}

/// Linux: starts a `.desktop` shortcut with `gio launch` (opening it would usually show it
/// in a text editor instead).
pub fn launch_desktop_entry(entry: &Path) -> CoreResult<()> {
    let mut cmd = Command::new("gio");
    cmd.arg("launch").arg(entry);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if std::env::var_os("APPIMAGE").is_some() {
        for key in APPIMAGE_ENV {
            cmd.env_remove(key);
        }
    }
    detach(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| CoreError::io(entry, std::io::Error::new(e.kind(), format!("gio launch failed: {e}"))))
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // New process group: closing the manager (or its terminal) doesn't signal the game.
    cmd.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn detach(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_has_data_path() {
        let cmd = game_command(Os::Linux, Path::new("/games/vs/Vintagestory"), Path::new("/data"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(args, ["--dataPath", "/data"]);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/games/vs")));
    }

    #[test]
    fn macos_bundle_starts_through_open() {
        let cmd = game_command(Os::Macos, Path::new("/Applications/Vintagestory.app/Contents/MacOS/Vintagestory"), Path::new("/data"));
        assert_eq!(cmd.get_program(), "/usr/bin/open");
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(args, ["-a", "/Applications/Vintagestory.app", "--args", "--dataPath", "/data"]);
        // A plain executable still starts directly.
        let cmd = game_command(Os::Macos, Path::new("/games/vs/Vintagestory"), Path::new("/data"));
        assert_eq!(cmd.get_program(), "/games/vs/Vintagestory");
    }

    #[test]
    fn flatpak_command_passes_the_data_path() {
        let cmd = flatpak_command(Path::new("/data"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(cmd.get_program(), "flatpak");
        assert_eq!(args, ["run", FLATPAK_ID, "--dataPath", "/data"]);
    }

    #[test]
    fn missing_executable_is_an_error() {
        assert!(launch_game(Os::current(), Path::new("/no/such/Vintagestory"), Path::new("/tmp")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn launches_a_detached_process() {
        let t = tempfile::tempdir().unwrap();
        let exe = t.path().join("Vintagestory");
        let marker = t.path().join("ran");
        std::fs::write(&exe, format!("#!/bin/sh\necho \"$@\" > '{}'\n", marker.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        launch_game(Os::Linux, &exe, &t.path().join("data")).unwrap();
        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let args = std::fs::read_to_string(&marker).unwrap();
        assert!(args.starts_with("--dataPath "));
    }
}
