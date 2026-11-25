//! Disk cache for Mod DB responses under `<manager dir>/Temp Cache/svsm3 Mod Database/`.
//!
//! Each mod's raw `mod` object is stored with the time it was fetched, so the app can start
//! offline and re-evaluate compatibility for another game version without a download.
//! Unknown mods are cached too (as `null`) so they aren't looked up on every start.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, CoreResult};

pub const CACHE_DIR_NAME: &str = "svsm3 Mod Database";

#[derive(Debug, Clone)]
pub struct ModDbCache {
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stamped<T> {
    fetched_ms: f64,
    data: T,
}

/// A cached `mod` object (`None` = the Mod DB doesn't know this id).
#[derive(Debug, Clone)]
pub struct CachedMod {
    pub raw: Option<Value>,
    pub fetched_ms: f64,
}

/// Replaces characters that are invalid in file names on any OS.
pub fn sanitize_file_name(input: &str, fallback: &str) -> String {
    let s: String = input
        .trim()
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() || s == ".." {
        fallback.to_string()
    } else {
        s
    }
}

impl ModDbCache {
    pub fn new(manager_dir: &Path) -> Self {
        Self { dir: manager_dir.join("Temp Cache").join(CACHE_DIR_NAME) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn mod_path(&self, mod_id: &str) -> PathBuf {
        self.dir.join("mods").join(format!("{}.json", sanitize_file_name(&mod_id.to_ascii_lowercase(), "mod")))
    }

    pub fn load_mod(&self, mod_id: &str) -> Option<CachedMod> {
        let s: Stamped<Option<Value>> = read_json(&self.mod_path(mod_id))?;
        Some(CachedMod { raw: s.data, fetched_ms: s.fetched_ms })
    }

    pub fn store_mod(&self, mod_id: &str, raw: Option<&Value>, fetched_ms: f64) -> CoreResult<()> {
        write_json(&self.mod_path(mod_id), &Stamped { fetched_ms, data: raw })
    }

    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Option<(T, f64)> {
        let s: Stamped<T> = read_json(&self.dir.join(format!("{}.json", sanitize_file_name(name, "data"))))?;
        Some((s.data, s.fetched_ms))
    }

    pub fn store<T: Serialize>(&self, name: &str, data: &T, fetched_ms: f64) -> CoreResult<()> {
        write_json(&self.dir.join(format!("{}.json", sanitize_file_name(name, "data"))), &Stamped { fetched_ms, data })
    }

    /// Removes everything cached. Safe to call while offline: data is simply fetched again later.
    pub fn clear(&self) -> CoreResult<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::io(&self.dir, e)),
        }
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let dir = path.parent().expect("cache paths have a parent");
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec(value).expect("cache values always serialize");
    std::fs::write(&tmp, bytes).map_err(|e| CoreError::io(&tmp, e))?;
    crate::fsutil::rename_with_retry(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_mods_and_misses() {
        let t = tempfile::tempdir().unwrap();
        let c = ModDbCache::new(t.path());
        assert!(c.load_mod("CarryOn").is_none());
        c.store_mod("CarryOn", Some(&serde_json::json!({"name": "Carry On"})), 5.0).unwrap();
        c.store_mod("ghost", None, 6.0).unwrap();
        let hit = c.load_mod("carryon").unwrap();
        assert_eq!(hit.fetched_ms, 5.0);
        assert_eq!(hit.raw.unwrap()["name"], "Carry On");
        assert!(c.load_mod("ghost").unwrap().raw.is_none());
        c.clear().unwrap();
        assert!(c.load_mod("carryon").is_none());
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize_file_name("a/b:c?", "x"), "a_b_c_");
        assert_eq!(sanitize_file_name("  ", "x"), "x");
        assert_eq!(sanitize_file_name("..", "x"), "x");
        assert_eq!(sanitize_file_name("v1.0.", "x"), "v1.0");
    }
}
