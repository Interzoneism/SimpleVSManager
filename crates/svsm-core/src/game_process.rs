//! Detects a running Vintage Story client.
//!
//! The game rewrites `clientsettings.json` when it exits (undoing any enable/disable made while
//! it ran) and, on Windows, keeps mod zips open. Commands that change mods, clientsettings or
//! the data folder check this first.

use std::ffi::OsStr;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// A running game process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningGame {
    pub pid: u32,
    pub name: String,
}

/// The native client launcher, without extension. `VintagestoryServer` is left out on purpose:
/// a server doesn't touch the client's clientsettings.json.
fn is_client_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    stem == "vintagestory"
}

/// Hosts that can run the game's managed assembly (`dotnet Vintagestory.dll`, older Mono builds).
fn is_runtime_host(name: &OsStr) -> bool {
    let name = name.to_string_lossy().to_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    matches!(stem, "dotnet" | "mono" | "mono-sgen")
}

fn is_client_argument(arg: &OsStr) -> bool {
    let arg = arg.to_string_lossy().to_lowercase();
    let file = arg.rsplit(['/', '\\']).next().unwrap_or(&arg);
    file == "vintagestory.dll" || file == "vintagestory.exe"
}

/// Finds a running Vintage Story client, if any.
pub fn find_running_game() -> Option<RunningGame> {
    let mut sys = System::new();
    // Names come with every refresh; command lines only for the few runtime hosts.
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let own = std::process::id();
    let mut hosts = Vec::new();
    for (pid, p) in sys.processes() {
        if pid.as_u32() == own {
            continue;
        }
        if is_client_name(p.name()) {
            return Some(RunningGame { pid: pid.as_u32(), name: p.name().to_string_lossy().to_string() });
        }
        if is_runtime_host(p.name()) {
            hosts.push(*pid);
        }
    }
    if hosts.is_empty() {
        return None;
    }
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&hosts), false, ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always));
    hosts.iter().find_map(|pid| {
        let p = sys.process(*pid)?;
        p.cmd().iter().any(|a| is_client_argument(a)).then(|| RunningGame { pid: pid.as_u32(), name: p.name().to_string_lossy().to_string() })
    })
}

/// The message shown when an action is blocked because the game is running.
pub const GAME_RUNNING_MESSAGE: &str = "Vintage Story is running. Close the game first: it rewrites clientsettings.json when it exits and keeps mod files open, so changes made now would be undone or stop halfway.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_client_only() {
        for n in ["Vintagestory.exe", "Vintagestory", "VINTAGESTORY.EXE"] {
            assert!(is_client_name(OsStr::new(n)), "{n}");
        }
        for n in ["VintagestoryServer.exe", "Simple VS Manager.exe", "vintagestory-launcher"] {
            assert!(!is_client_name(OsStr::new(n)), "{n}");
        }
        assert!(is_runtime_host(OsStr::new("dotnet.exe")));
        assert!(is_client_argument(OsStr::new("/opt/vintagestory/Vintagestory.dll")));
        assert!(is_client_argument(OsStr::new(r"C:\Games\Vintagestory\Vintagestory.exe")));
        assert!(!is_client_argument(OsStr::new("VintagestoryServer.dll")));
    }

    #[test]
    fn scanning_does_not_panic() {
        // The test runner itself is never the game.
        let _ = find_running_game();
    }
}
