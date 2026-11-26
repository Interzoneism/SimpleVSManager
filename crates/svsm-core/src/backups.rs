//! Backups of the whole `VintagestoryData` folder, in the 2.x format so both apps can restore
//! each other's backups (port of `DataBackupService`):
//!
//! ```text
//! <backup root>/                     "Data Folder Backups" in the manager folder, or a custom one
//!   2026-11-24-001.zip               data-backup.json + Data/<everything except Cache and Saves>
//!   .saves/<SHA256>.file | .dir      each save stored once, shared by every backup
//! ```
//!
//! `data-backup.json` (PascalCase): `Id`, `CreatedOnUtc`, `SourceDataDirectory`,
//! `VintageStoryVersion`, `Saves: [{RelativePath, Hash, IsDirectory, Size}]`.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};

pub const BACKUPS_DIR: &str = "Data Folder Backups";
pub const MANIFEST: &str = "data-backup.json";
const SAVES_STORE: &str = ".saves";
const DATA: &str = "Data";
const SAVES: &str = "Saves";
const CACHE: &str = "Cache";
/// Where a restore keeps the data folder's previous contents until the backup is in place.
pub const RESTORE_STAGING: &str = ".svsm-restore-previous";

/// Moves `names` from `staging` back into `data_dir`, then removes `staging` if it's empty.
fn put_back(data_dir: &Path, staging: &Path, names: &[std::ffi::OsString]) {
    for n in names {
        if let Err(e) = std::fs::rename(staging.join(n), data_dir.join(n)) {
            tracing::error!("Could not move {} back into the data folder: {e}", staging.join(n).display());
        }
    }
    let _ = std::fs::remove_dir(staging);
}

/// Refuses to empty a folder that isn't a Vintage Story data folder: one without
/// `clientsettings.json` (or at least `Mods` plus another folder the game makes), a folder
/// holding the game itself, or a drive root. An empty or missing folder is fine.
pub fn check_restore_target(data_dir: &Path) -> CoreResult<()> {
    let mut entries = match std::fs::read_dir(data_dir) {
        Ok(rd) => rd.flatten().peekable(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io(data_dir, e)),
    };
    if entries.peek().is_none() {
        return Ok(());
    }
    let refuse = |why: &str| Err(io(data_dir, format!("{why} Nothing was changed.")));
    if data_dir.parent().is_none() || data_dir.canonicalize().ok().is_some_and(|c| c.parent().is_none()) {
        return refuse("This is a drive root, not a Vintage Story data folder.");
    }
    if crate::paths::find_game_executable(crate::platform::Os::Windows, data_dir).is_some() {
        return refuse("This folder holds the game itself, not only its data.");
    }
    let game_dirs = ["Saves", "Logs", "ModConfig", "Playerdata", "Cache", "Backups", "Macros"];
    let looks_right = data_dir.join("clientsettings.json").is_file()
        || (data_dir.join("Mods").is_dir() && game_dirs.iter().any(|d| data_dir.join(d).is_dir()));
    if !looks_right {
        return refuse("This isn't a Vintage Story data folder.");
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Manifest {
    #[serde(alias = "id")]
    pub id: Option<String>,
    #[serde(alias = "createdOnUtc")]
    pub created_on_utc: String,
    #[serde(alias = "sourceDataDirectory")]
    pub source_data_directory: Option<String>,
    #[serde(alias = "vintageStoryVersion")]
    pub vintage_story_version: Option<String>,
    #[serde(alias = "saves")]
    pub saves: Vec<SaveRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SaveRef {
    #[serde(alias = "relativePath")]
    pub relative_path: String,
    #[serde(alias = "hash")]
    pub hash: String,
    #[serde(alias = "isDirectory")]
    pub is_directory: bool,
    #[serde(alias = "size")]
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    /// File or folder name inside the backup root.
    pub id: String,
    pub path: String,
    pub created_ms: Option<f64>,
    pub source_data_directory: Option<String>,
    pub game_version: Option<String>,
    pub size_bytes: f64,
    pub save_count: u32,
}

fn io(path: &Path, e: impl std::fmt::Display) -> CoreError {
    CoreError::io(path, std::io::Error::other(e.to_string()))
}

fn same_dir(a: &str, b: &Path) -> bool {
    let a = Path::new(a.trim_end_matches(['/', '\\']));
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().trim_end_matches(['/', '\\']).to_lowercase()
    } else {
        a == b
    }
}

fn same_version(a: Option<&str>, b: Option<&str>) -> bool {
    match (a.and_then(crate::version::normalize), b.and_then(crate::version::normalize)) {
        (Some(x), Some(y)) => x == y,
        // Game version unknown on both sides (no game folder found): treat as the same.
        (None, None) => true,
        _ => false,
    }
}

/// Byte progress: (done, total, label).
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64, &str);

pub struct DataBackups {
    root: PathBuf,
}

impl DataBackups {
    /// `custom` = the user's backup location (2.x `customDataBackupLocation`).
    pub fn new(manager_dir: &Path, custom: Option<&Path>) -> Self {
        Self { root: custom.map(Path::to_path_buf).unwrap_or_else(|| manager_dir.join(BACKUPS_DIR)) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn store(&self) -> PathBuf {
        self.root.join(SAVES_STORE)
    }

    fn store_path(&self, s: &SaveRef) -> PathBuf {
        self.store().join(format!("{}{}", s.hash, if s.is_directory { ".dir" } else { ".file" }))
    }

    // --- Listing -----------------------------------------------------------------------------

    fn read_manifest(path: &Path) -> Option<Manifest> {
        let text = if path.is_dir() {
            std::fs::read_to_string(path.join(MANIFEST)).ok()?
        } else {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
            let mut entry = zip.by_name(MANIFEST).ok()?;
            let mut s = String::new();
            entry.read_to_string(&mut s).ok()?;
            s
        };
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    }

    fn entries(&self) -> Vec<(PathBuf, Manifest)> {
        let Ok(rd) = std::fs::read_dir(&self.root) else { return Vec::new() };
        rd.flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().is_some_and(|n| n != SAVES_STORE))
            .filter(|p| p.is_dir() || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")))
            .filter_map(|p| Some((p.clone(), Self::read_manifest(&p)?)))
            .filter(|(_, m)| !m.created_on_utc.is_empty())
            .collect()
    }

    /// Every backup, newest first.
    pub fn list(&self) -> Vec<BackupSummary> {
        let mut out: Vec<BackupSummary> = self
            .entries()
            .into_iter()
            .map(|(p, m)| BackupSummary {
                id: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                path: p.display().to_string(),
                created_ms: parse_dotnet_date(&m.created_on_utc),
                source_data_directory: m.source_data_directory.clone(),
                game_version: m.vintage_story_version.clone(),
                size_bytes: if p.is_dir() { dir_size(&p) as f64 } else { std::fs::metadata(&p).map(|x| x.len() as f64).unwrap_or(0.0) },
                save_count: m.saves.len() as u32,
            })
            .collect();
        out.sort_by(|a, b| b.created_ms.partial_cmp(&a.created_ms).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    fn find(&self, id: &str) -> CoreResult<(PathBuf, Manifest)> {
        if id.contains(['/', '\\']) || id.starts_with('.') {
            return Err(io(Path::new(id), "not a backup"));
        }
        let p = self.root.join(id);
        let m = Self::read_manifest(&p).ok_or_else(|| io(&p, "This backup can't be read."))?;
        Ok((p, m))
    }

    // --- Creating ----------------------------------------------------------------------------

    fn next_id(&self) -> String {
        let prefix = chrono::Local::now().format("%Y-%m-%d-").to_string();
        let highest = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                let rest = n.strip_prefix(&prefix)?.trim_end_matches(".zip").to_string();
                rest.split('_').next()?.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        let mut id = format!("{prefix}{:03}", highest + 1);
        let mut n = 1;
        while self.root.join(&id).exists() || self.root.join(format!("{id}.zip")).exists() {
            id = format!("{prefix}{:03}_{n}", highest + 1);
            n += 1;
        }
        id
    }

    /// Backs up `data_dir`. Saves already in the store aren't copied again.
    pub fn create(&self, data_dir: &Path, game_version: Option<&str>, progress: Progress, cancelled: &dyn Fn() -> bool) -> CoreResult<BackupSummary> {
        if !data_dir.is_dir() {
            return Err(io(data_dir, "The data folder doesn't exist."));
        }
        std::fs::create_dir_all(self.store()).map_err(|e| io(&self.store(), e))?;
        let id = self.next_id();
        let zip_path = self.root.join(format!("{id}.zip"));
        let tmp = self.root.join(format!("{id}.zip.tmp"));

        // Plan: ordinary files, and top-level entries of Saves.
        let mut files: Vec<(PathBuf, String, u64)> = Vec::new();
        let mut saves: Vec<(PathBuf, String, bool)> = Vec::new();
        let root_canon = self.root.canonicalize().ok();
        for e in std::fs::read_dir(data_dir).map_err(|e| io(data_dir, e))?.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case(CACHE) || root_canon.as_ref().is_some_and(|r| p.canonicalize().ok().as_ref() == Some(r)) {
                continue;
            }
            if p.is_dir() && name.eq_ignore_ascii_case(SAVES) {
                for s in std::fs::read_dir(&p).map_err(|e| io(&p, e))?.flatten() {
                    let sp = s.path();
                    saves.push((sp.clone(), s.file_name().to_string_lossy().to_string(), sp.is_dir()));
                }
                continue;
            }
            walk(&p, &name, &mut files, root_canon.as_deref());
        }
        let total: u64 = files.iter().map(|f| f.2.max(1)).sum::<u64>() + saves.iter().map(|s| if s.2 { dir_size(&s.0) } else { file_len(&s.0) }.max(1)).sum::<u64>();
        let mut done = 0u64;

        let result = (|| -> CoreResult<Manifest> {
            let file = std::fs::File::create(&tmp).map_err(|e| io(&tmp, e))?;
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(1)).large_file(true);
            let mut buf = vec![0u8; 256 * 1024];
            for (src, rel, len) in &files {
                if cancelled() {
                    return Err(io(data_dir, "Cancelled."));
                }
                progress(done, total, rel);
                zip.start_file(format!("{DATA}/{rel}"), opts).map_err(|e| io(&tmp, e))?;
                let mut f = match std::fs::File::open(src) {
                    Ok(f) => f,
                    // Locked or vanished files (the game may be writing logs) are skipped.
                    Err(_) => continue,
                };
                loop {
                    let n = f.read(&mut buf).map_err(|e| io(src, e))?;
                    if n == 0 {
                        break;
                    }
                    zip.write_all(&buf[..n]).map_err(|e| io(&tmp, e))?;
                }
                done += len.max(&1);
            }
            let mut manifest = Manifest {
                id: Some(id.clone()),
                created_on_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                source_data_directory: Some(data_dir.display().to_string().trim_end_matches(['/', '\\']).to_string()),
                vintage_story_version: game_version.and_then(crate::version::normalize).or(game_version.map(String::from)),
                saves: Vec::new(),
            };
            if !saves.is_empty() {
                zip.add_directory(format!("{DATA}/{SAVES}/"), opts).map_err(|e| io(&tmp, e))?;
            }
            for (src, rel, is_dir) in &saves {
                if cancelled() {
                    return Err(io(data_dir, "Cancelled."));
                }
                progress(done, total, &format!("Saves/{rel}"));
                let hash = if *is_dir { hash_dir(src)? } else { hash_file(src)? };
                let size = if *is_dir { dir_size(src) } else { file_len(src) };
                let s = SaveRef { relative_path: rel.clone(), hash, is_directory: *is_dir, size: size as f64 };
                let dest = self.store_path(&s);
                if !dest.exists() {
                    let staging = PathBuf::from(format!("{}.tmp", dest.display()));
                    if *is_dir {
                        copy_dir(src, &staging)?;
                    } else {
                        std::fs::copy(src, &staging).map_err(|e| io(src, e))?;
                    }
                    std::fs::rename(&staging, &dest).map_err(|e| io(&dest, e))?;
                }
                done += size.max(1);
                manifest.saves.push(s);
            }
            zip.start_file(MANIFEST, opts).map_err(|e| io(&tmp, e))?;
            zip.write_all(serde_json::to_string_pretty(&manifest).expect("manifest serializes").as_bytes()).map_err(|e| io(&tmp, e))?;
            zip.finish().map_err(|e| io(&tmp, e))?;
            Ok(manifest)
        })();
        match result {
            Ok(_) => {
                std::fs::rename(&tmp, &zip_path).map_err(|e| io(&zip_path, e))?;
                progress(total, total, "Done");
                Ok(self.list().into_iter().find(|b| b.path == zip_path.display().to_string()).expect("just created"))
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.cleanup_store();
                Err(e)
            }
        }
    }

    // --- Deleting / pruning ------------------------------------------------------------------

    pub fn delete(&self, id: &str) -> CoreResult<()> {
        let (p, _) = self.find(id)?;
        if p.is_dir() {
            std::fs::remove_dir_all(&p).map_err(|e| io(&p, e))?;
        } else {
            std::fs::remove_file(&p).map_err(|e| io(&p, e))?;
        }
        self.cleanup_store();
        Ok(())
    }

    /// Keeps the newest `max` backups of this data folder and game version (2.x rule).
    pub fn prune(&self, data_dir: &Path, game_version: Option<&str>, max: u32) -> u32 {
        if max == 0 {
            return 0;
        }
        let mut mine: Vec<(PathBuf, Option<f64>)> = self
            .entries()
            .into_iter()
            .filter(|(_, m)| m.source_data_directory.as_deref().is_some_and(|s| same_dir(s, data_dir)))
            .filter(|(_, m)| same_version(m.vintage_story_version.as_deref(), game_version))
            .map(|(p, m)| (p, parse_dotnet_date(&m.created_on_utc)))
            .collect();
        mine.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut n = 0;
        for (p, _) in mine.into_iter().skip(max as usize) {
            let ok = if p.is_dir() { std::fs::remove_dir_all(&p).is_ok() } else { std::fs::remove_file(&p).is_ok() };
            n += ok as u32;
        }
        if n > 0 {
            self.cleanup_store();
        }
        n
    }

    /// Removes saves no backup refers to.
    fn cleanup_store(&self) {
        let used: HashSet<String> = self.entries().into_iter().flat_map(|(_, m)| m.saves.into_iter().map(|s| s.hash.to_uppercase())).collect();
        let Ok(rd) = std::fs::read_dir(self.store()) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let hash = name.strip_suffix(".dir").or_else(|| name.strip_suffix(".file"));
            if let Some(h) = hash {
                if !used.contains(&h.to_uppercase()) {
                    let p = e.path();
                    let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                }
            }
        }
    }

    // --- Restoring ---------------------------------------------------------------------------

    /// Replaces the contents of `data_dir` with the backup (like 2.x: everything in the data
    /// folder is replaced, including Cache). Make a backup of the current state first.
    ///
    /// Refuses a folder that doesn't look like a Vintage Story data folder. The current
    /// contents are first moved aside into a staging folder inside `data_dir` (a rename, so
    /// nothing is copied); if anything fails they are moved back, and they are only deleted
    /// once the whole backup is in place.
    pub fn restore(&self, id: &str, data_dir: &Path, progress: Progress) -> CoreResult<()> {
        let (path, manifest) = self.find(id)?;
        // Check everything is there before touching the data folder.
        for s in &manifest.saves {
            if !self.store_path(s).exists() {
                return Err(io(&self.store_path(s), format!("The save \"{}\" is missing from the backup store.", s.relative_path)));
            }
        }
        check_restore_target(data_dir)?;
        let backup_root_canon = self.root.canonicalize().ok();
        std::fs::create_dir_all(data_dir).map_err(|e| io(data_dir, e))?;
        let staging = data_dir.join(RESTORE_STAGING);
        if staging.exists() {
            return Err(io(
                &staging,
                "An earlier restore was interrupted. Move what you need from this folder back into the data folder, then delete it.",
            ));
        }
        let keep = |p: &Path| p == staging || backup_root_canon.as_ref().is_some_and(|r| p.canonicalize().ok().as_ref() == Some(r));
        // 1. Move the current contents aside.
        std::fs::create_dir_all(&staging).map_err(|e| io(&staging, e))?;
        let mut moved: Vec<std::ffi::OsString> = Vec::new();
        for e in std::fs::read_dir(data_dir).map_err(|e| io(data_dir, e))?.flatten() {
            let p = e.path();
            if keep(&p) {
                continue; // the backups themselves may live in the data folder
            }
            if let Err(err) = std::fs::rename(&p, staging.join(e.file_name())) {
                put_back(data_dir, &staging, &moved);
                return Err(io(&p, format!("{err}. Nothing was changed. Is Vintage Story still running?")));
            }
            moved.push(e.file_name());
        }
        // 2. Unpack the backup; on failure clear the half-restored files and put things back.
        match self.unpack(&path, &manifest, data_dir, progress) {
            Ok(()) => {
                if let Err(e) = std::fs::remove_dir_all(&staging) {
                    tracing::warn!("Restored, but the previous data in {} could not be removed: {e}", staging.display());
                }
                Ok(())
            }
            Err(err) => {
                for e in std::fs::read_dir(data_dir).into_iter().flatten().flatten() {
                    let p = e.path();
                    if !keep(&p) {
                        let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                    }
                }
                put_back(data_dir, &staging, &moved);
                Err(err)
            }
        }
    }

    fn unpack(&self, path: &Path, manifest: &Manifest, data_dir: &Path, progress: Progress) -> CoreResult<()> {
        let total = manifest.saves.iter().map(|s| s.size as u64).sum::<u64>() + 1;
        let mut done = 0u64;
        if path.is_dir() {
            copy_dir(&path.join(DATA), data_dir)?;
        } else {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(path).map_err(|e| io(path, e))?).map_err(|e| io(path, e))?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| io(path, e))?;
                let Some(name) = entry.enclosed_name() else { continue };
                let Ok(rel) = name.strip_prefix(DATA) else { continue };
                if rel.as_os_str().is_empty() {
                    continue;
                }
                let dest = data_dir.join(rel);
                progress(done, total, &rel.display().to_string());
                if entry.is_dir() {
                    std::fs::create_dir_all(&dest).map_err(|e| io(&dest, e))?;
                    continue;
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
                }
                let mut out = std::fs::File::create(&dest).map_err(|e| io(&dest, e))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| io(&dest, e))?;
            }
        }
        let saves_dir = data_dir.join(SAVES);
        for s in &manifest.saves {
            progress(done, total, &format!("Saves/{}", s.relative_path));
            let rel = Path::new(&s.relative_path);
            if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                continue;
            }
            let dest = saves_dir.join(rel);
            if s.is_directory {
                copy_dir(&self.store_path(s), &dest)?;
            } else {
                std::fs::create_dir_all(&saves_dir).map_err(|e| io(&saves_dir, e))?;
                std::fs::copy(self.store_path(s), &dest).map_err(|e| io(&dest, e))?;
            }
            done += s.size as u64;
        }
        let _ = std::fs::remove_dir_all(data_dir.join(CACHE));
        progress(total, total, "Done");
        Ok(())
    }
}

fn walk(p: &Path, rel: &str, out: &mut Vec<(PathBuf, String, u64)>, skip: Option<&Path>) {
    if skip.is_some_and(|s| p.canonicalize().ok().as_deref() == Some(s)) {
        return;
    }
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                walk(&e.path(), &format!("{rel}/{}", e.file_name().to_string_lossy()), out, skip);
            }
        }
    } else if p.is_file() {
        out.push((p.to_path_buf(), rel.to_string(), file_len(p)));
    }
}

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let ep = e.path();
            if ep.is_dir() {
                stack.push(ep);
            } else {
                total += file_len(&ep);
            }
        }
    }
    total
}

fn copy_dir(src: &Path, dest: &Path) -> CoreResult<()> {
    std::fs::create_dir_all(dest).map_err(|e| io(dest, e))?;
    for e in std::fs::read_dir(src).map_err(|e| io(src, e))?.flatten() {
        let (s, d) = (e.path(), dest.join(e.file_name()));
        if s.is_dir() {
            copy_dir(&s, &d)?;
        } else {
            std::fs::copy(&s, &d).map_err(|e| io(&s, e))?;
        }
    }
    Ok(())
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// SHA-256 of a file, upper-case hex (2.x `Convert.ToHexString`).
pub fn hash_file(p: &Path) -> CoreResult<String> {
    let mut f = std::fs::File::open(p).map_err(|e| io(p, e))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| io(p, e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex_upper(&h.finalize()))
}

/// 2.x folder hash: `D:<rel>` for every folder, then `F:<rel>` + bytes for every file, each
/// list sorted case-insensitively by full path, `/` separators.
pub fn hash_dir(p: &Path) -> CoreResult<String> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| io(&d, e))?.flatten() {
            let ep = e.path();
            if ep.is_dir() {
                dirs.push(ep.clone());
                stack.push(ep);
            } else {
                files.push(ep);
            }
        }
    }
    let key = |x: &PathBuf| x.display().to_string().to_uppercase();
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    let rel = |x: &Path| x.strip_prefix(p).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let mut h = Sha256::new();
    for d in &dirs {
        h.update(format!("D:{}", rel(d)).as_bytes());
    }
    let mut buf = vec![0u8; 256 * 1024];
    for f in &files {
        h.update(format!("F:{}", rel(f)).as_bytes());
        let mut file = std::fs::File::open(f).map_err(|e| io(f, e))?;
        loop {
            let n = file.read(&mut buf).map_err(|e| io(f, e))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
    }
    Ok(hex_upper(&h.finalize()))
}

/// .NET `DateTime` round-trip strings ("2026-11-24T19:00:00.1234567Z", sometimes without zone).
fn parse_dotnet_date(s: &str) -> Option<f64> {
    let s = s.trim();
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_millis() as f64)
        .ok()
        .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|d| d.and_utc().timestamp_millis() as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(root: &Path) -> PathBuf {
        let d = root.join("VintagestoryData");
        std::fs::create_dir_all(d.join("Mods")).unwrap();
        std::fs::create_dir_all(d.join("Saves/World A")).unwrap();
        std::fs::create_dir_all(d.join("Cache")).unwrap();
        std::fs::write(d.join("clientsettings.json"), "{}").unwrap();
        std::fs::write(d.join("Mods/a.zip"), "zip").unwrap();
        std::fs::write(d.join("Saves/one.vcdbs"), "save1").unwrap();
        std::fs::write(d.join("Saves/World A/data.bin"), "save2").unwrap();
        std::fs::write(d.join("Cache/junk"), "x").unwrap();
        d
    }

    #[test]
    fn backup_dedups_saves_and_restores() {
        let t = tempfile::tempdir().unwrap();
        let d = data(t.path());
        let b = DataBackups::new(&t.path().join("mgr"), None);
        let mut calls = 0;
        let first = b.create(&d, Some("1.22.7"), &mut |_, _, _| calls += 1, &|| false).unwrap();
        assert!(calls > 0);
        assert!(first.id.ends_with("-001.zip"));
        assert_eq!(first.save_count, 2);
        let second = b.create(&d, Some("1.22.7"), &mut |_, _, _| {}, &|| false).unwrap();
        assert!(second.id.ends_with("-002.zip"));
        assert_eq!(std::fs::read_dir(b.root().join(SAVES_STORE)).unwrap().count(), 2, "saves stored once");

        // The zip has the 2.x layout.
        let mut z = zip::ZipArchive::new(std::fs::File::open(&first.path).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert!(names.contains(&"data-backup.json".to_string()));
        assert!(names.contains(&"Data/Mods/a.zip".to_string()));
        assert!(!names.iter().any(|n| n.contains("Cache") || n.contains("one.vcdbs")));
        let m: serde_json::Value = {
            let mut s = String::new();
            z.by_name(MANIFEST).unwrap().read_to_string(&mut s).unwrap();
            serde_json::from_str(&s).unwrap()
        };
        assert!(m.get("CreatedOnUtc").is_some() && m["Saves"][0].get("Hash").is_some());

        // Change things, then restore the first backup.
        std::fs::write(d.join("Mods/new.zip"), "n").unwrap();
        std::fs::remove_file(d.join("Saves/one.vcdbs")).unwrap();
        b.restore(&first.id, &d, &mut |_, _, _| {}).unwrap();
        assert!(!d.join("Mods/new.zip").exists());
        assert_eq!(std::fs::read_to_string(d.join("Saves/one.vcdbs")).unwrap(), "save1");
        assert_eq!(std::fs::read_to_string(d.join("Saves/World A/data.bin")).unwrap(), "save2");
        assert!(!d.join("Cache").exists());

        // Pruning keeps the newest per folder + version and tidies the store.
        b.create(&d, Some("1.22.7"), &mut |_, _, _| {}, &|| false).unwrap();
        assert_eq!(b.prune(&d, Some("1.22.7"), 1), 2);
        assert_eq!(b.list().len(), 1);
        b.delete(&b.list()[0].id).unwrap();
        assert_eq!(std::fs::read_dir(b.root().join(SAVES_STORE)).unwrap().count(), 0);
    }

    #[test]
    fn reads_2x_uncompressed_backups_and_refuses_bad_ids() {
        let t = tempfile::tempdir().unwrap();
        let b = DataBackups::new(t.path(), None);
        let dir = b.root().join("2025-01-01-001");
        std::fs::create_dir_all(dir.join("Data/Mods")).unwrap();
        std::fs::write(dir.join("Data/Mods/x.zip"), "x").unwrap();
        std::fs::write(
            dir.join(MANIFEST),
            r#"{"Id":"2025-01-01-001","CreatedOnUtc":"2025-01-01T10:00:00.1234567Z","SourceDataDirectory":"C:\\VSData","VintageStoryVersion":"1.21.0","Saves":[]}"#,
        )
        .unwrap();
        let list = b.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].game_version.as_deref(), Some("1.21.0"));
        assert!(list[0].created_ms.is_some());
        let target = t.path().join("restore");
        b.restore("2025-01-01-001", &target, &mut |_, _, _| {}).unwrap();
        assert!(target.join("Mods/x.zip").exists());
        assert!(b.restore("../evil", &target, &mut |_, _, _| {}).is_err());
    }

    #[test]
    fn missing_saves_stop_before_anything_is_deleted() {
        let t = tempfile::tempdir().unwrap();
        let d = data(t.path());
        let b = DataBackups::new(&t.path().join("mgr"), None);
        let made = b.create(&d, None, &mut |_, _, _| {}, &|| false).unwrap();
        std::fs::remove_dir_all(b.root().join(SAVES_STORE)).unwrap();
        assert!(b.restore(&made.id, &d, &mut |_, _, _| {}).is_err());
        assert!(d.join("Mods/a.zip").exists(), "data folder untouched");
    }

    #[test]
    fn refuses_folders_that_are_not_data_folders() {
        let t = tempfile::tempdir().unwrap();
        let d = data(t.path());
        let b = DataBackups::new(&t.path().join("mgr"), None);
        let made = b.create(&d, None, &mut |_, _, _| {}, &|| false).unwrap();
        // A documents folder with a Mods folder in it is not enough.
        let docs = t.path().join("Documents");
        std::fs::create_dir_all(docs.join("Mods")).unwrap();
        std::fs::write(docs.join("taxes.pdf"), "keep me").unwrap();
        assert!(b.restore(&made.id, &docs, &mut |_, _, _| {}).is_err());
        assert!(docs.join("taxes.pdf").exists());
        // Nor is a portable game folder.
        let game = t.path().join("Vintagestory");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("clientsettings.json"), "{}").unwrap();
        std::fs::write(game.join("Vintagestory.exe"), "exe").unwrap();
        assert!(b.restore(&made.id, &game, &mut |_, _, _| {}).is_err());
        assert!(game.join("Vintagestory.exe").exists());
    }

    #[test]
    fn failed_restore_puts_the_previous_data_back() {
        let t = tempfile::tempdir().unwrap();
        let d = data(t.path());
        let b = DataBackups::new(&t.path().join("mgr"), None);
        // A 2.x folder backup whose save is listed as a folder but stored as a file: copying it fails.
        let dir = b.root().join("2025-01-01-001");
        std::fs::create_dir_all(dir.join("Data/Mods")).unwrap();
        std::fs::write(dir.join("Data/Mods/other.zip"), "o").unwrap();
        std::fs::create_dir_all(b.store()).unwrap();
        std::fs::write(b.store().join("ABC.dir"), "not a folder").unwrap();
        std::fs::write(
            dir.join(MANIFEST),
            r#"{"Id":"x","CreatedOnUtc":"2025-01-01T10:00:00Z","Saves":[{"RelativePath":"w","Hash":"ABC","IsDirectory":true,"Size":1}]}"#,
        )
        .unwrap();
        assert!(b.restore("2025-01-01-001", &d, &mut |_, _, _| {}).is_err());
        assert_eq!(std::fs::read_to_string(d.join("Mods/a.zip")).unwrap(), "zip");
        assert_eq!(std::fs::read_to_string(d.join("Saves/one.vcdbs")).unwrap(), "save1");
        assert!(!d.join("Mods/other.zip").exists());
        assert!(!d.join(RESTORE_STAGING).exists());
    }

    #[test]
    fn folder_hash_is_stable() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("w/sub")).unwrap();
        std::fs::write(t.path().join("w/sub/b"), "1").unwrap();
        std::fs::write(t.path().join("w/A"), "2").unwrap();
        let h = hash_dir(&t.path().join("w")).unwrap();
        assert_eq!(h.len(), 64);
        assert_eq!(h, hash_dir(&t.path().join("w")).unwrap());
        std::fs::write(t.path().join("w/A"), "3").unwrap();
        assert_ne!(h, hash_dir(&t.path().join("w")).unwrap());
    }
}
