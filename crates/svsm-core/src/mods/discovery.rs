//! Finds installed mods the way the game does. Port of `ModDiscoveryService.cs`.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

use super::model::{InstalledMod, ModSourceKind};
use super::modinfo::{parse_modinfo, to_mod_id, ModInfo};
use super::{apply_load_statuses, is_base_game_mod};
use crate::clientsettings::ClientSettings;

const CODE_MOD_DESCRIPTION: &str = "This code mod is not packaged with a modinfo.json file. \
Pack the mod into a zip or folder with modinfo.json so it can be managed.";
const CODE_MOD_ERROR: &str = "Metadata unavailable for code-only mods.";
/// Icons larger than this are ignored (a broken mod shouldn't blow up memory).
const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct DiscoveryResult {
    pub mods: Vec<InstalledMod>,
    /// Icon bytes by `InstalledMod::icon_key`.
    pub icons: HashMap<String, Vec<u8>>,
    /// Folders that were scanned, in order.
    pub search_paths: Vec<PathBuf>,
}

/// Folders the game searches for mods, in order: `<data>/Mods`, `<data>/ModsByServer`,
/// `modPaths` from clientsettings.json, then the paths listed in the latest client log.
/// The game's own `<game>/Mods` (base game content) is excluded.
pub fn search_paths(settings: &ClientSettings, game_dir: Option<&Path>) -> Vec<PathBuf> {
    let data_dir = settings.data_dir();
    let mut out: Vec<PathBuf> = vec![data_dir.join("Mods"), data_dir.join("ModsByServer")];

    let mut bases = vec![data_dir.to_path_buf()];
    if let Some(g) = game_dir {
        bases.push(g.to_path_buf());
    }
    for raw in settings.mod_paths() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            out.push(p);
        } else {
            out.extend(bases.iter().map(|b| b.join(&p)));
        }
    }
    out.extend(paths_from_latest_log(&data_dir.join("Logs")));

    let game_mods = game_dir.map(|g| g.join("Mods"));
    let mut seen: Vec<String> = Vec::new();
    out.into_iter()
        .map(|p| normalize(&p))
        .filter(|p| game_mods.as_ref().is_none_or(|gm| !same_path(p, &normalize(gm))))
        .filter(|p| !is_game_install_mods_dir(p))
        .filter(|p| {
            let k = path_key(p);
            if seen.contains(&k) {
                false
            } else {
                seen.push(k);
                true
            }
        })
        .collect()
}

/// Scans all search paths and returns the mods with load statuses applied.
pub fn discover_mods(settings: &ClientSettings, game_dir: Option<&Path>) -> DiscoveryResult {
    let search_paths = search_paths(settings, game_dir);

    // Collect candidate entries in a stable order (search path order, then name).
    let mut entries: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for dir in &search_paths {
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        let mut children: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
        children.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for child in children {
            let k = path_key(&child);
            if !seen.contains(&k) {
                seen.push(k);
                entries.push(child);
            }
        }
    }

    let loaded: Vec<(InstalledMod, Option<Vec<u8>>)> =
        entries.par_iter().filter_map(|p| load_entry(p)).collect();

    let mut icons = HashMap::new();
    let mut mods = Vec::with_capacity(loaded.len());
    for (mut m, icon) in loaded {
        if is_base_game_mod(&m.mod_id) {
            continue;
        }
        if let Some(bytes) = icon {
            let key = icon_key(&m.source_path);
            icons.insert(key.clone(), bytes);
            m.icon_key = Some(key);
        }
        m.is_active = !settings.is_disabled(&m.mod_id, m.version.as_deref());
        mods.push(m);
    }

    apply_load_statuses(&mut mods, |id, v| settings.is_disabled(id, v));
    DiscoveryResult { mods, icons, search_paths }
}

/// Loads one directory entry. `None` for things that aren't mods (other files, folders
/// without modinfo.json).
fn load_entry(path: &Path) -> Option<(InstalledMod, Option<Vec<u8>>)> {
    let meta = std::fs::metadata(path).ok()?;
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();

    if meta.is_dir() {
        return load_folder(path, &file_name, &meta);
    }
    match ext.as_str() {
        "zip" => Some(load_zip(path, &file_name, &meta)),
        "cs" | "dll" => Some((code_mod(path, &file_name, &meta, ext == "cs"), None)),
        _ => None,
    }
}

fn load_folder(dir: &Path, file_name: &str, meta: &std::fs::Metadata) -> Option<(InstalledMod, Option<Vec<u8>>)> {
    let modinfo = find_child_ci(dir, "modinfo.json")?;
    let base = blank(dir, file_name, ModSourceKind::Folder, meta);
    let text = match std::fs::read_to_string(&modinfo) {
        Ok(t) => t,
        Err(e) => return Some((with_error(base, file_name, e.to_string()), None)),
    };
    match parse_modinfo(&text, file_name) {
        Ok(info) => {
            let icon = info
                .icon_path
                .as_deref()
                .and_then(|rel| safe_join(dir, rel))
                .filter(|p| p.is_file())
                .or_else(|| find_child_ci(dir, "modicon.png"))
                .and_then(|p| read_small(&p));
            Some((with_info(base, info), icon))
        }
        Err(e) => Some((with_error(base, file_name, e), None)),
    }
}

fn load_zip(path: &Path, file_name: &str, meta: &std::fs::Metadata) -> (InstalledMod, Option<Vec<u8>>) {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = blank(path, file_name, ModSourceKind::Zip, meta);

    let result = (|| -> Result<(ModInfo, Option<Vec<u8>>), String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
        let names: Vec<String> = archive.file_names().filter_map(|n| n.ok()).map(|n| n.into_owned()).collect();

        let info_name = find_zip_entry(&names, "modinfo.json").ok_or("Missing modinfo.json")?;
        let text = read_zip_entry(&mut archive, &info_name, 1024 * 1024)?;
        let text = String::from_utf8_lossy(&text).to_string();
        let info = parse_modinfo(&text, &stem)?;

        let icon = info
            .icon_path
            .as_deref()
            .and_then(|p| find_zip_entry(&names, p))
            .or_else(|| find_zip_entry(&names, "modicon.png"))
            .and_then(|n| read_zip_entry(&mut archive, &n, MAX_ICON_BYTES).ok());
        Ok((info, icon))
    })();

    match result {
        Ok((info, icon)) => (with_info(base, info), icon),
        Err(e) => (with_error(base, &stem, e), None),
    }
}

fn code_mod(path: &Path, file_name: &str, meta: &std::fs::Metadata, is_cs: bool) -> InstalledMod {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let kind = if is_cs { ModSourceKind::SourceCode } else { ModSourceKind::Assembly };
    let mut m = blank(path, file_name, kind, meta);
    m.mod_id = to_mod_id(&stem);
    m.name = stem;
    m.description = Some(CODE_MOD_DESCRIPTION.into());
    m.error = Some(CODE_MOD_ERROR.into());
    m
}

fn blank(path: &Path, file_name: &str, kind: ModSourceKind, meta: &std::fs::Metadata) -> InstalledMod {
    let source_path = path.display().to_string();
    InstalledMod {
        key: source_path.clone(),
        mod_id: String::new(),
        name: file_name.to_string(),
        manifest_name: None,
        version: None,
        network_version: None,
        description: None,
        website: None,
        authors: vec![],
        contributors: vec![],
        dependencies: vec![],
        side: None,
        required_on_client: None,
        required_on_server: None,
        source_path,
        source_kind: kind,
        file_name: file_name.to_string(),
        size_bytes: (!meta.is_dir()).then_some(meta.len() as f64),
        modified_ms: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64),
        icon_key: None,
        error: None,
        load_error: None,
        missing_dependencies: vec![],
        dependency_has_errors: false,
        duplicates: vec![],
        is_active: true,
    }
}

fn with_info(mut m: InstalledMod, info: ModInfo) -> InstalledMod {
    m.mod_id = info.mod_id;
    m.name = info.name;
    m.manifest_name = info.manifest_name;
    m.version = info.version;
    m.network_version = info.network_version;
    m.description = info.description;
    m.website = info.website;
    m.authors = info.authors;
    m.contributors = info.contributors;
    m.dependencies = info.dependencies;
    m.side = info.side;
    m.required_on_client = info.required_on_client;
    m.required_on_server = info.required_on_server;
    m
}

fn with_error(mut m: InstalledMod, hint_name: &str, error: String) -> InstalledMod {
    m.name = hint_name.to_string();
    m.mod_id = to_mod_id(hint_name);
    m.error = Some(error);
    m
}

/// Finds a zip entry by normalized path, then by file name anywhere in the archive
/// (case-insensitive, like the WPF app).
fn find_zip_entry(names: &[String], wanted: &str) -> Option<String> {
    let norm = |s: &str| {
        let mut n = s.trim().replace('\\', "/");
        while let Some(rest) = n.strip_prefix("./") {
            n = rest.to_string();
        }
        n.trim_start_matches('/').to_lowercase()
    };
    let target = norm(wanted);
    if target.is_empty() {
        return None;
    }
    if let Some(n) = names.iter().find(|n| norm(n) == target) {
        return Some(n.clone());
    }
    let target_file = target.rsplit('/').next().unwrap_or(&target).to_string();
    names
        .iter()
        .filter(|n| !n.ends_with('/'))
        .find(|n| norm(n).rsplit('/').next() == Some(target_file.as_str()))
        .cloned()
}

fn read_zip_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = archive.by_name(name).map_err(|e| e.to_string())?;
    if entry.size() > limit {
        return Err(format!("{name} is too large"));
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.take(limit).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let len = std::fs::metadata(path).ok()?.len();
    (len <= MAX_ICON_BYTES).then(|| std::fs::read(path).ok()).flatten()
}

/// Case-insensitive lookup of a direct child (Linux file systems are case-sensitive, mod
/// authors on Windows aren't careful).
fn find_child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// Joins a relative path from a manifest, refusing anything that escapes `base`.
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel.trim());
    if rel.is_absolute() {
        return None;
    }
    let mut out = base.to_path_buf();
    for c in rel.components() {
        match c {
            std::path::Component::Normal(p) => out.push(p),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Parses "Will search the following paths for mods:" from the newest client log.
fn paths_from_latest_log(logs_dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(logs_dir) else { return vec![] };
    let mut logs: Vec<(std::time::SystemTime, PathBuf, bool)> = read
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().to_lowercase();
            if !name.ends_with(".txt") && !name.ends_with(".log") {
                return None;
            }
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, p, name.starts_with("client-main")))
        })
        .collect();
    // Prefer client-main logs, newest first.
    logs.sort_by(|a, b| b.2.cmp(&a.2).then(b.0.cmp(&a.0)));
    let Some((_, log, _)) = logs.into_iter().next() else { return vec![] };
    let Ok(bytes) = std::fs::read(&log) else { return vec![] };
    parse_log_mod_paths(&String::from_utf8_lossy(&bytes))
}

pub(crate) fn parse_log_mod_paths(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut collecting = false;
    for line in text.lines() {
        let msg = match line.find(']') {
            Some(i) => line[i + 1..].trim_start(),
            None => line.trim_start(),
        };
        if !collecting {
            if msg.to_lowercase().contains("will search the following paths for mods:") {
                collecting = true;
            }
            continue;
        }
        let mut t = msg.trim();
        if t.is_empty() {
            break;
        }
        if let Some(stripped) = t.strip_suffix("(Not found?)") {
            t = stripped.trim_end();
        }
        let looks_like_path = t.contains('/')
            || t.contains('\\')
            || (t.len() >= 2 && t.as_bytes()[0].is_ascii_alphabetic() && t.as_bytes()[1] == b':');
        if !looks_like_path {
            break;
        }
        out.push(PathBuf::from(t));
    }
    out
}

/// The game's own install `Mods` folder holds base content; the WPF app skipped any path
/// containing `/Vintagestory/Mods`. Only skip it when it really holds the base game mods: a
/// data folder such as `~/Vintagestory` (common with `--dataPath`) has a `Mods` folder too.
fn is_game_install_mods_dir(p: &Path) -> bool {
    let s = p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
    s.ends_with("/vintagestory/mods")
        && std::fs::read_dir(p).into_iter().flatten().flatten().any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            ["vssurvivalmod", "vsessentials", "vscreativemod"].iter().any(|b| name.starts_with(b))
        })
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Comparison key for paths: case-insensitive on Windows and macOS (default volumes are),
/// exact on Linux.
fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

fn icon_key(source_path: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source_path.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for (name, data) in files {
            z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    fn data_dir_with_settings(disabled: &[&str]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("Mods")).unwrap();
        let v = serde_json::json!({ "stringListSettings": { "disabledMods": disabled, "modPaths": ["Mods"] } });
        std::fs::write(t.path().join("clientsettings.json"), v.to_string()).unwrap();
        t
    }

    #[test]
    fn discovers_zip_folder_and_code_mods() {
        let t = data_dir_with_settings(&["foldermod@1.0"]);
        let mods = t.path().join("Mods");
        write_zip(
            &mods.join("ZipMod_2.0.zip"),
            &[
                ("modinfo.json", br#"{"modid":"zipmod","name":"Zip Mod","version":"2.0","dependencies":{"foldermod":"1.0"}}"#),
                ("assets/modicon.png", b"PNGDATA"),
            ],
        );
        std::fs::create_dir_all(mods.join("FolderMod")).unwrap();
        std::fs::write(mods.join("FolderMod/ModInfo.json"), r#"{"modid":"foldermod","version":"1.0"}"#).unwrap();
        std::fs::write(mods.join("Script.cs"), "class X {}").unwrap();
        std::fs::write(mods.join("readme.txt"), "not a mod").unwrap();
        std::fs::create_dir_all(mods.join("EmptyFolder")).unwrap();
        write_zip(&mods.join("broken.zip"), &[("other.txt", b"x")]);
        std::fs::write(mods.join("garbage.zip"), b"not a zip").unwrap();

        let s = ClientSettings::load(t.path()).unwrap();
        let r = discover_mods(&s, None);
        let ids: Vec<_> = r.mods.iter().map(|m| m.mod_id.as_str()).collect();
        assert_eq!(ids, ["broken", "foldermod", "garbage", "script", "zipmod"]);

        let zip = r.mods.iter().find(|m| m.mod_id == "zipmod").unwrap();
        assert_eq!(zip.name, "Zip Mod");
        assert!(zip.is_active);
        assert_eq!(r.icons[zip.icon_key.as_ref().unwrap()], b"PNGDATA");
        // Its dependency is disabled, so it won't load.
        assert!(zip.dependency_has_errors);

        let folder = r.mods.iter().find(|m| m.mod_id == "foldermod").unwrap();
        assert!(!folder.is_active);
        assert_eq!(folder.source_kind, ModSourceKind::Folder);

        let broken = r.mods.iter().find(|m| m.mod_id == "broken").unwrap();
        assert_eq!(broken.error.as_deref(), Some("Missing modinfo.json"));
        assert!(r.mods.iter().find(|m| m.mod_id == "garbage").unwrap().error.is_some());
        assert_eq!(r.mods.iter().find(|m| m.mod_id == "script").unwrap().source_kind, ModSourceKind::SourceCode);
    }

    #[test]
    fn base_game_mods_are_hidden() {
        let t = data_dir_with_settings(&[]);
        write_zip(&t.path().join("Mods/vs.zip"), &[("modinfo.json", br#"{"modid":"VSSurvivalMod"}"#)]);
        let s = ClientSettings::load(t.path()).unwrap();
        assert!(discover_mods(&s, None).mods.is_empty());
    }

    #[test]
    fn search_paths_resolve_and_dedupe() {
        let t = data_dir_with_settings(&[]);
        let extra = t.path().join("extra");
        let v = serde_json::json!({ "stringListSettings": { "modPaths": ["Mods", extra, "./Mods"] } });
        std::fs::write(t.path().join("clientsettings.json"), v.to_string()).unwrap();
        let game = t.path().join("game");
        let s = ClientSettings::load(t.path()).unwrap();
        let paths = search_paths(&s, Some(&game));
        assert_eq!(
            paths,
            vec![t.path().join("Mods"), t.path().join("ModsByServer"), extra]
        );
    }

    #[test]
    fn log_paths() {
        let log = "1.1.2025 10:00:00 [Notification] Will search the following paths for mods:\n\
                   1.1.2025 10:00:00 [Notification]     C:\\Games\\Vintagestory\\Mods\n\
                   1.1.2025 10:00:00 [Notification]     /home/x/extra (Not found?)\n\
                   1.1.2025 10:00:00 [Notification] Found 3 mods\n";
        assert_eq!(
            parse_log_mod_paths(log),
            vec![PathBuf::from("C:\\Games\\Vintagestory\\Mods"), PathBuf::from("/home/x/extra")]
        );
    }

    #[test]
    fn zip_entry_lookup() {
        let names = vec!["Foo/ModInfo.JSON".to_string(), "textures/".to_string()];
        assert_eq!(find_zip_entry(&names, "modinfo.json").as_deref(), Some("Foo/ModInfo.JSON"));
        assert_eq!(find_zip_entry(&names, ""), None);
    }

    #[test]
    fn safe_join_blocks_escape() {
        assert!(safe_join(Path::new("/m"), "../x.png").is_none());
        assert_eq!(safe_join(Path::new("/m"), "./a/b.png"), Some(PathBuf::from("/m/a/b.png")));
    }
}
