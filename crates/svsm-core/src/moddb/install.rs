//! Installing downloaded releases and the version cache under `Temp Cache/Cached Mods/`.
//!
//! The cache layout is shared with Simple VS Manager 2.x (`<mod id>/<version>/<file name>`), so
//! versions cached by either app can be used for rollback by both.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::cache::sanitize_file_name;
use crate::error::{CoreError, CoreResult};

fn other(path: &Path, msg: impl Into<String>) -> CoreError {
    CoreError::io(path, std::io::Error::other(msg.into()))
}

/// `<manager dir>/Temp Cache/Cached Mods`.
#[derive(Debug, Clone)]
pub struct CachedMods {
    dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CachedVersion {
    pub version: String,
    pub path: String,
    pub size_bytes: f64,
}

impl CachedMods {
    pub fn new(manager_dir: &Path) -> Self {
        Self { dir: manager_dir.join("Temp Cache").join("Cached Mods") }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn mod_dir(&self, mod_id: &str) -> PathBuf {
        self.dir.join(sanitize_file_name(mod_id, "mod"))
    }

    /// Where a version's archive is stored: `<mod id>/<version>/<file name>`.
    pub fn path_for(&self, mod_id: &str, version: &str, file_name: Option<&str>) -> PathBuf {
        let ver = sanitize_file_name(version, "version");
        let name = match file_name.map(|f| sanitize_file_name(f, "")).filter(|f| !f.is_empty()) {
            Some(f) if Path::new(&f).extension().is_some() => f,
            Some(f) => format!("{f}.zip"),
            None => format!("{}-{ver}.zip", sanitize_file_name(mod_id, "mod")),
        };
        self.mod_dir(mod_id).join(&ver).join(name)
    }

    /// Finds a cached archive for a version: the preferred name, any file in the version
    /// folder, or the 2.x legacy layout `<mod id>/<version>.zip`.
    pub fn locate(&self, mod_id: &str, version: &str, file_name: Option<&str>) -> Option<PathBuf> {
        let preferred = self.path_for(mod_id, version, file_name);
        if preferred.is_file() {
            return Some(preferred);
        }
        let ver_dir = preferred.parent()?.to_path_buf();
        if let Ok(rd) = std::fs::read_dir(&ver_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && !p.to_string_lossy().ends_with(".tmp") {
                    return Some(p);
                }
            }
        }
        let legacy = self.mod_dir(mod_id).join(format!("{}.zip", sanitize_file_name(version, "version")));
        legacy.is_file().then_some(legacy)
    }

    /// Every cached version of a mod, newest version first.
    pub fn versions(&self, mod_id: &str) -> Vec<CachedVersion> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.mod_dir(mod_id)) else {
            return out;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if let Some(file) = self.locate(mod_id, &name, None) {
                    let size = std::fs::metadata(&file).map(|m| m.len() as f64).unwrap_or(0.0);
                    out.push(CachedVersion { version: name, path: file.to_string_lossy().into(), size_bytes: size });
                }
            } else if let Some(ver) = name.strip_suffix(".zip") {
                if !out.iter().any(|v| v.version == ver) {
                    let size = e.metadata().map(|m| m.len() as f64).unwrap_or(0.0);
                    out.push(CachedVersion { version: ver.to_string(), path: p.to_string_lossy().into(), size_bytes: size });
                }
            }
        }
        out.sort_by(|a, b| {
            if crate::version::is_candidate_newer(Some(&a.version), Some(&b.version)) {
                std::cmp::Ordering::Less
            } else if crate::version::is_candidate_newer(Some(&b.version), Some(&a.version)) {
                std::cmp::Ordering::Greater
            } else {
                a.version.cmp(&b.version)
            }
        });
        out
    }

    /// Total size of the cache in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.entries().iter().map(|e| e.size).sum()
    }

    fn entries(&self) -> Vec<CacheEntry> {
        let mut out = Vec::new();
        let Ok(mods) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        for m in mods.flatten() {
            let mod_id = m.file_name().to_string_lossy().to_string();
            let Ok(items) = std::fs::read_dir(m.path()) else { continue };
            for item in items.flatten() {
                let p = item.path();
                let files: Vec<PathBuf> = if p.is_dir() {
                    std::fs::read_dir(&p).into_iter().flatten().flatten().map(|f| f.path()).filter(|f| f.is_file()).collect()
                } else {
                    vec![p.clone()]
                };
                let version = if p.is_dir() {
                    item.file_name().to_string_lossy().to_string()
                } else {
                    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
                };
                for f in files {
                    let Ok(meta) = f.metadata() else { continue };
                    let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    out.push(CacheEntry { mod_id: mod_id.clone(), version: version.clone(), path: f, size: meta.len(), modified });
                }
            }
        }
        out
    }

    /// Deletes the oldest cached files until the cache fits in `limit_bytes`. Versions in
    /// `keep` (mod id, version) are never deleted. Returns the bytes freed.
    pub fn prune(&self, limit_bytes: u64, keep: &[(String, String)]) -> u64 {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|e| e.size).sum();
        if total <= limit_bytes {
            return 0;
        }
        entries.sort_by_key(|e| e.modified);
        let mut freed = 0;
        for e in entries {
            if total <= limit_bytes {
                break;
            }
            let kept = keep.iter().any(|(id, v)| {
                sanitize_file_name(id, "mod").eq_ignore_ascii_case(&e.mod_id)
                    && sanitize_file_name(v, "version").eq_ignore_ascii_case(&e.version)
            });
            if kept || std::fs::remove_file(&e.path).is_err() {
                continue;
            }
            total -= e.size;
            freed += e.size;
            // Tidy empty version and mod folders.
            if let Some(dir) = e.path.parent().filter(|d| *d != self.dir) {
                let _ = std::fs::remove_dir(dir);
                if let Some(parent) = dir.parent().filter(|d| *d != self.dir) {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
        freed
    }

    /// Removes every cached version.
    pub fn clear(&self) -> CoreResult<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::io(&self.dir, e)),
        }
    }

    /// Moves an installed archive into the cache (the mod library) for `version`. If that
    /// version is already cached, the installed file is just removed. Returns the cached path.
    pub fn store_move(&self, mod_id: &str, version: &str, file_name: Option<&str>, archive: &Path) -> CoreResult<PathBuf> {
        validate_archive(archive)?;
        if let Some(existing) = self.locate(mod_id, version, file_name) {
            std::fs::remove_file(archive).map_err(|e| CoreError::io(archive, e))?;
            return Ok(existing);
        }
        let dest = self.path_for(mod_id, version, file_name);
        move_file(archive, &dest)?;
        Ok(dest)
    }

    /// Copies `archive` into the cache for `version` unless that version is already cached.
    pub fn store_copy(&self, mod_id: &str, version: &str, file_name: Option<&str>, archive: &Path) -> CoreResult<PathBuf> {
        if let Some(existing) = self.locate(mod_id, version, file_name) {
            return Ok(existing);
        }
        let dest = self.path_for(mod_id, version, file_name);
        copy_atomic(archive, &dest)?;
        Ok(dest)
    }
}

struct CacheEntry {
    mod_id: String,
    version: String,
    path: PathBuf,
    size: u64,
    modified: std::time::SystemTime,
}

/// Checks that a file is a zip containing a `modinfo.json` somewhere.
pub fn validate_archive(path: &Path) -> CoreResult<()> {
    let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let zip = zip::ZipArchive::new(file)
        .map_err(|_| other(path, "The downloaded file is not a valid Vintage Story mod archive."))?;
    let has_manifest = zip
        .file_names()
        .filter_map(|n| n.ok())
        .any(|n| n.rsplit(['/', '\\']).next().is_some_and(|f| f.eq_ignore_ascii_case("modinfo.json")));
    if has_manifest {
        Ok(())
    } else {
        Err(other(path, "The downloaded file does not contain a modinfo.json manifest."))
    }
}

/// Where an archive goes.
#[derive(Debug, Clone)]
pub enum InstallTarget {
    /// Replace an installed zip (the new file may get the release's file name).
    ReplaceFile { existing: PathBuf },
    /// Replace an installed (unpacked) folder mod by extracting the archive.
    ReplaceFolder { existing: PathBuf },
    /// A fresh install into a mods folder.
    NewFile { mods_dir: PathBuf },
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub mod_id: String,
    pub installed_version: Option<String>,
    pub new_version: String,
    /// The release's file name on the Mod DB.
    pub release_file_name: Option<String>,
    pub target: InstallTarget,
    /// Keep the replaced version in the cache for rollback.
    pub cache_replaced: bool,
}

/// Installs a validated archive. Returns the installed path. On failure the previous version
/// is put back.
pub fn install_archive(plan: &InstallPlan, archive: &Path, cache: &CachedMods) -> CoreResult<PathBuf> {
    validate_archive(archive)?;
    match &plan.target {
        InstallTarget::NewFile { mods_dir } => {
            let name = target_file_name(plan, None);
            let dest = mods_dir.join(name);
            if dest.exists() {
                return Err(other(&dest, "A file with this name already exists in the Mods folder."));
            }
            std::fs::create_dir_all(mods_dir).map_err(|e| CoreError::io(mods_dir, e))?;
            copy_atomic(archive, &dest)?;
            Ok(dest)
        }
        InstallTarget::ReplaceFile { existing } => replace_file(plan, existing, archive, cache),
        InstallTarget::ReplaceFolder { existing } => replace_folder(existing, archive),
    }
}

fn target_file_name(plan: &InstallPlan, existing: Option<&Path>) -> String {
    let from_release = plan
        .release_file_name
        .as_deref()
        .map(|f| sanitize_file_name(f, ""))
        .filter(|f| !f.is_empty())
        .map(|f| if Path::new(&f).extension().is_some() { f } else { format!("{f}.zip") });
    from_release
        .or_else(|| existing.and_then(|e| e.file_name()).map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| format!("{}-{}.zip", sanitize_file_name(&plan.mod_id, "mod"), sanitize_file_name(&plan.new_version, "version")))
}

fn replace_file(plan: &InstallPlan, existing: &Path, archive: &Path, cache: &CachedMods) -> CoreResult<PathBuf> {
    let dir = existing.parent().ok_or_else(|| other(existing, "no parent folder"))?;
    let dest = dir.join(target_file_name(plan, Some(existing)));
    let renamed = !same_path(&dest, existing);
    if renamed && dest.exists() {
        return Err(other(&dest, "Another file in the Mods folder already has the new release's name."));
    }

    // Park the old version: in the cache (kept for rollback) or next to it (removed on success).
    enum Parked {
        Nothing,
        Cached(PathBuf),
        Backup(PathBuf),
    }
    let parked = if !existing.exists() {
        Parked::Nothing
    } else {
        let cached_slot = match (&plan.installed_version, plan.cache_replaced) {
            (Some(v), true) => {
                let name = existing.file_name().map(|n| n.to_string_lossy().to_string());
                match cache.locate(&plan.mod_id, v, name.as_deref()) {
                    Some(_) => None, // already cached; just remove the installed copy later
                    None => Some(cache.path_for(&plan.mod_id, v, name.as_deref())),
                }
            }
            _ => None,
        };
        match cached_slot {
            Some(slot) => {
                move_file(existing, &slot)?;
                Parked::Cached(slot)
            }
            None => {
                let backup = unique_path(existing, ".immbackup");
                move_file(existing, &backup)?;
                Parked::Backup(backup)
            }
        }
    };

    match copy_atomic(archive, &dest) {
        Ok(()) => {
            if let Parked::Backup(b) = &parked {
                let _ = std::fs::remove_file(b);
            }
            Ok(dest)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&dest);
            match &parked {
                Parked::Cached(c) => {
                    let _ = copy_atomic(c, existing);
                }
                Parked::Backup(b) => {
                    let _ = move_file(b, existing);
                }
                Parked::Nothing => {}
            }
            Err(e)
        }
    }
}

/// Largest total size a folder mod may unpack to.
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

fn replace_folder(existing: &Path, archive: &Path) -> CoreResult<PathBuf> {
    let parent = existing.parent().ok_or_else(|| other(existing, "no parent folder"))?;
    let name = existing.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let staging = unique_path(&parent.join(format!(".{name}")), ".svsm-extract");
    let result = (|| {
        let file = std::fs::File::open(archive).map_err(|e| CoreError::io(archive, e))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| other(archive, e.to_string()))?;
        // Refuse zip bombs: no real mod unpacks to anywhere near this.
        let unpacked: u64 = (0..zip.len()).filter_map(|i| zip.by_index_raw(i).ok().map(|f| f.size())).sum();
        if unpacked > MAX_UNPACKED_BYTES {
            return Err(other(archive, "The archive unpacks to more than 2 GB, so it wasn't installed."));
        }
        zip.extract(&staging).map_err(|e| other(&staging, e.to_string()))?;
        // A single top-level folder and no files means the payload is inside it.
        let entries: Vec<_> = std::fs::read_dir(&staging).map_err(|e| CoreError::io(&staging, e))?.flatten().collect();
        let payload = match entries.as_slice() {
            [only] if only.path().is_dir() => only.path(),
            _ => staging.clone(),
        };
        let backup = unique_path(existing, ".immbackup");
        let had_existing = existing.exists();
        if had_existing {
            std::fs::rename(existing, &backup).map_err(|e| CoreError::io(existing, e))?;
        }
        if let Err(e) = std::fs::rename(&payload, existing) {
            if had_existing {
                let _ = std::fs::rename(&backup, existing);
            }
            return Err(CoreError::io(existing, e));
        }
        if had_existing {
            let _ = std::fs::remove_dir_all(&backup);
        }
        Ok(existing.to_path_buf())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn unique_path(base: &Path, suffix: &str) -> PathBuf {
    let mut p = PathBuf::from(format!("{}{suffix}", base.display()));
    let mut i = 1;
    while p.exists() {
        p = PathBuf::from(format!("{}{suffix}{i}", base.display()));
        i += 1;
    }
    p
}

/// Rename, falling back to copy + delete across drives.
fn move_file(from: &Path, to: &Path) -> CoreResult<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_atomic(from, to)?;
    std::fs::remove_file(from).map_err(|e| CoreError::io(from, e))
}

/// Copies to `<dest>.tmp` and renames, so a half-written file never has the final name.
fn copy_atomic(from: &Path, dest: &Path) -> CoreResult<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", dest.display()));
    std::fs::copy(from, &tmp).map_err(|e| CoreError::io(&tmp, e))?;
    crate::fsutil::rename_with_retry(&tmp, dest).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, version: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("modinfo.json", o).unwrap();
        write!(z, r#"{{"modid":"carryon","version":"{version}"}}"#).unwrap();
        z.finish().unwrap();
    }

    fn plan(target: InstallTarget, installed: Option<&str>, file: &str) -> InstallPlan {
        InstallPlan {
            mod_id: "carryon".into(),
            installed_version: installed.map(Into::into),
            new_version: "1.2.0".into(),
            release_file_name: Some(file.into()),
            target,
            cache_replaced: true,
        }
    }

    #[test]
    fn update_renames_and_caches_old_version() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let old = mods.join("CarryOn-1.1.0.zip");
        make_zip(&old, "1.1.0");
        let dl = t.path().join("dl.zip");
        make_zip(&dl, "1.2.0");
        let cache = CachedMods::new(&t.path().join("mgr"));

        let p = plan(InstallTarget::ReplaceFile { existing: old.clone() }, Some("1.1.0"), "CarryOn-1.2.0.zip");
        let new = install_archive(&p, &dl, &cache).unwrap();
        assert_eq!(new, mods.join("CarryOn-1.2.0.zip"));
        assert!(!old.exists());
        let cached = cache.locate("carryon", "1.1.0", Some("CarryOn-1.1.0.zip")).unwrap();
        assert!(cached.ends_with("carryon/1.1.0/CarryOn-1.1.0.zip"));
        // Without a file name it still finds the version (spelling may differ on Windows).
        assert!(cache.locate("carryon", "1.1.0", None).is_some());
        assert_eq!(cache.versions("carryon").len(), 1);
        // No leftovers in the Mods folder.
        assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 1);
    }

    #[test]
    fn update_without_cache_leaves_no_backup() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let old = mods.join("carryon.zip");
        make_zip(&old, "1.1.0");
        let dl = t.path().join("dl.zip");
        make_zip(&dl, "1.2.0");
        let cache = CachedMods::new(&t.path().join("mgr"));
        let mut p = plan(InstallTarget::ReplaceFile { existing: old.clone() }, Some("1.1.0"), "carryon.zip");
        p.cache_replaced = false;
        install_archive(&p, &dl, &cache).unwrap();
        assert!(old.exists());
        assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 1);
        assert!(cache.versions("carryon").is_empty());
    }

    #[test]
    fn rejects_bad_archives_and_keeps_old_file() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let old = mods.join("carryon.zip");
        make_zip(&old, "1.1.0");
        let bad = t.path().join("bad.zip");
        std::fs::write(&bad, "not a zip").unwrap();
        let cache = CachedMods::new(&t.path().join("mgr"));
        let p = plan(InstallTarget::ReplaceFile { existing: old.clone() }, Some("1.1.0"), "x.zip");
        assert!(install_archive(&p, &bad, &cache).is_err());
        assert!(old.exists());
    }

    #[test]
    fn refuses_to_clobber_another_file() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let old = mods.join("a.zip");
        make_zip(&old, "1.1.0");
        std::fs::write(mods.join("b.zip"), "other mod").unwrap();
        let dl = t.path().join("dl.zip");
        make_zip(&dl, "1.2.0");
        let cache = CachedMods::new(&t.path().join("mgr"));
        let p = plan(InstallTarget::ReplaceFile { existing: old.clone() }, Some("1.1.0"), "b.zip");
        assert!(install_archive(&p, &dl, &cache).is_err());
        assert!(old.exists());
        assert_eq!(std::fs::read(mods.join("b.zip")).unwrap(), b"other mod");
    }

    #[test]
    fn fresh_install_and_folder_replace() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        let dl = t.path().join("dl.zip");
        make_zip(&dl, "1.2.0");
        let cache = CachedMods::new(&t.path().join("mgr"));
        let p = plan(InstallTarget::NewFile { mods_dir: mods.clone() }, None, "CarryOn.zip");
        assert_eq!(install_archive(&p, &dl, &cache).unwrap(), mods.join("CarryOn.zip"));
        assert!(install_archive(&p, &dl, &cache).is_err(), "second install refuses to overwrite");

        let folder = mods.join("carryon");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("old.txt"), "x").unwrap();
        let p = plan(InstallTarget::ReplaceFolder { existing: folder.clone() }, Some("1.1.0"), "c.zip");
        install_archive(&p, &dl, &cache).unwrap();
        assert!(folder.join("modinfo.json").exists());
        assert!(!folder.join("old.txt").exists());
        let leftovers: Vec<_> = std::fs::read_dir(&mods).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 2, "{leftovers:?}");
    }

    #[test]
    fn prune_drops_oldest_but_keeps_installed() {
        let t = tempfile::tempdir().unwrap();
        let c = CachedMods::new(t.path());
        let put = |id: &str, v: &str, secs: u64| {
            let p = c.path_for(id, v, None);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![0u8; 100]).unwrap();
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
        };
        put("a", "1.0", 10);
        put("a", "2.0", 20);
        put("b", "1.0", 5);
        put("b", "2.0", 30);
        assert_eq!(c.total_bytes(), 400);
        // b 1.0 is oldest but installed; a 1.0 goes, then a 2.0.
        let freed = c.prune(200, &[("b".into(), "1.0".into())]);
        assert_eq!(freed, 200);
        assert!(c.locate("b", "1.0", None).is_some());
        assert!(c.locate("b", "2.0", None).is_some());
        assert!(c.versions("a").is_empty());
        assert!(!c.dir().join("a").exists());
        c.clear().unwrap();
        assert_eq!(c.total_bytes(), 0);
    }

    #[test]
    fn store_move_takes_the_file_out_of_mods() {
        let t = tempfile::tempdir().unwrap();
        let mods = t.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let a = mods.join("CarryOn.zip");
        make_zip(&a, "1.0.0");
        let c = CachedMods::new(&t.path().join("mgr"));
        let stored = c.store_move("carryon", "1.0.0", Some("CarryOn.zip"), &a).unwrap();
        assert!(!a.exists() && stored.is_file());
        // Same version again: the installed copy is just removed.
        make_zip(&a, "1.0.0");
        assert_eq!(c.store_move("carryon", "1.0.0", Some("CarryOn.zip"), &a).unwrap(), stored);
        assert!(!a.exists());
        // Not a mod archive: left alone.
        std::fs::write(&a, "junk").unwrap();
        assert!(c.store_move("carryon", "2.0.0", None, &a).is_err());
        assert!(a.exists());
    }

    #[test]
    fn cache_paths_match_2x_layout() {
        let c = CachedMods::new(Path::new("/m"));
        assert_eq!(c.path_for("carryon", "1.2.0", Some("CarryOn.zip")), Path::new("/m/Temp Cache/Cached Mods/carryon/1.2.0/CarryOn.zip"));
        assert_eq!(c.path_for("carryon", "1.2.0", None), Path::new("/m/Temp Cache/Cached Mods/carryon/1.2.0/carryon-1.2.0.zip"));
        assert_eq!(c.path_for("a", "1", Some("noext")), Path::new("/m/Temp Cache/Cached Mods/a/1/noext.zip"));
    }
}
