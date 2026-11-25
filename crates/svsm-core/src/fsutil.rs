//! Small file-system helpers shared by modules that write user or game files.

use std::io::Write;
use std::path::Path;

use crate::error::{CoreError, CoreResult};

/// Writes `bytes` to `path` and flushes them to disk.
pub fn write_synced(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let mut f = std::fs::File::create(path).map_err(|e| CoreError::io(path, e))?;
    f.write_all(bytes).map_err(|e| CoreError::io(path, e))?;
    f.sync_all().map_err(|e| CoreError::io(path, e))
}

/// Renames `from` over `to`. On Windows, antivirus or the game can briefly hold the target
/// open, so retry a few times before giving up.
pub fn rename_with_retry(from: &Path, to: &Path) -> CoreResult<()> {
    let mut last = None;
    for attempt in 0..5 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1)));
            }
        }
    }
    Err(CoreError::io(to, last.expect("loop ran at least once")))
}
