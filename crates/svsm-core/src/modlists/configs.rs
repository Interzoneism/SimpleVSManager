//! Mod config files (`VintagestoryData/ModConfig`) carried inside modlists.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use super::format::{ConfigFile, ModlistConfig};
use crate::error::{CoreError, CoreResult};
use crate::mods::InstalledMod;

pub const MOD_CONFIG_DIR: &str = "ModConfig";
const EXTENSIONS: [&str; 3] = ["json", "yaml", "yml"];

/// A config file that could go into a saved modlist.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConfigCandidate {
    /// Path under ModConfig, with `/`.
    pub relative_path: String,
    /// Installed mod it most likely belongs to.
    pub mod_id: Option<String>,
    pub mod_name: Option<String>,
    pub size_bytes: f64,
}

/// A safe path under ModConfig: no `..`, no absolute parts, `/` separated. `None` if unusable.
fn clean_relative(rel: &str) -> Option<PathBuf> {
    let rel = rel.trim().replace('\\', "/");
    let mut out = PathBuf::new();
    for part in rel.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." || part.contains(':') {
            return None;
        }
        out.push(crate::moddb::cache::sanitize_file_name(part, "_"));
    }
    (out.components().count() > 0 && out.components().all(|c| matches!(c, Component::Normal(_)))).then_some(out)
}

fn target_for(file: &ConfigFile) -> PathBuf {
    file.relative_path
        .as_deref()
        .and_then(clean_relative)
        // The relative path's file name is the real one; 2.x kept it when present.
        .unwrap_or_else(|| PathBuf::from(&file.file_name))
}

/// Paths (under ModConfig) the files will be written to, made unique like 2.x
/// (`name_1.json` when two configs claim the same path).
pub fn targets(files: &[ConfigFile]) -> Vec<PathBuf> {
    let mut used = HashSet::new();
    files
        .iter()
        .map(|f| {
            let base = target_for(f);
            let mut candidate = base.clone();
            let mut n = 1;
            while !used.insert(candidate.to_string_lossy().to_lowercase()) {
                let stem = base.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let ext = base.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                candidate = base.with_file_name(format!("{stem}_{n}{ext}"));
                n += 1;
            }
            candidate
        })
        .collect()
}

/// Current contents of the files that writing `files` would overwrite (for the backup).
pub fn existing_contents(data_dir: &Path, files: &[ConfigFile]) -> Vec<ModlistConfig> {
    let dir = data_dir.join(MOD_CONFIG_DIR);
    files
        .iter()
        .zip(targets(files))
        .filter_map(|(f, rel)| {
            let content = std::fs::read_to_string(dir.join(&rel)).ok()?;
            Some(ModlistConfig {
                mod_id: Some(f.mod_id.clone()),
                file_name: rel.file_name().map(|n| n.to_string_lossy().to_string()),
                relative_path: Some(rel_string(&rel)),
                content: Some(content),
            })
        })
        .collect()
}

fn rel_string(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Writes config files into `<data>/ModConfig`. Returns how many were written and the errors.
pub fn write_configs(data_dir: &Path, files: &[ConfigFile]) -> (u32, Vec<String>) {
    let dir = data_dir.join(MOD_CONFIG_DIR);
    let mut written = 0;
    let mut errors = Vec::new();
    for (f, rel) in files.iter().zip(targets(files)) {
        let path = dir.join(&rel);
        let result = (|| -> CoreResult<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
            }
            let tmp = PathBuf::from(format!("{}.tmp", path.display()));
            crate::fsutil::write_synced(&tmp, f.content.as_bytes())?;
            crate::fsutil::rename_with_retry(&tmp, &path)
        })();
        match result {
            Ok(()) => written += 1,
            Err(e) => errors.push(format!("{}: {e}", rel_string(&rel))),
        }
    }
    (written, errors)
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Guesses which installed mod a config file belongs to from its name (or its folder's).
pub fn guess_mod<'a>(relative_path: &str, installed: &'a [InstalledMod]) -> Option<&'a InstalledMod> {
    let mut names: Vec<String> = Vec::new();
    let path = Path::new(relative_path);
    if let Some(stem) = path.file_stem() {
        names.push(norm(&stem.to_string_lossy()));
    }
    if let Some(first) = relative_path.replace('\\', "/").split('/').next().filter(|_| relative_path.contains(['/', '\\'])) {
        names.push(norm(first));
    }
    let mut best: Option<(&InstalledMod, usize)> = None;
    for m in installed {
        for key in [norm(&m.mod_id), norm(&m.name)] {
            if key.len() < 3 {
                continue;
            }
            let hit = names.iter().any(|n| n.starts_with(&key) || (n.len() >= 4 && key.starts_with(n.as_str())));
            if hit && best.is_none_or(|(_, len)| key.len() > len) {
                best = Some((m, key.len()));
            }
        }
    }
    best.map(|(m, _)| m)
}

/// Config files under `<data>/ModConfig` (json/yaml, three levels deep), matched to mods.
/// `known` maps lower-case mod ids to file paths the user (or 2.x) linked explicitly.
pub fn candidates(data_dir: &Path, installed: &[InstalledMod], known: &[(String, PathBuf)]) -> Vec<ConfigCandidate> {
    let root = data_dir.join(MOD_CONFIG_DIR);
    let mut out = Vec::new();
    let mut stack = vec![(root.clone(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 2 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let ok_ext = p.extension().is_some_and(|x| EXTENSIONS.iter().any(|k| x.to_string_lossy().eq_ignore_ascii_case(k)));
            if !ok_ext {
                continue;
            }
            let Ok(rel) = p.strip_prefix(&root) else { continue };
            let rel = rel_string(rel);
            let linked = known
                .iter()
                .find(|(_, path)| path == &p)
                .and_then(|(id, _)| installed.iter().find(|m| m.mod_id.eq_ignore_ascii_case(id)));
            let m = linked.or_else(|| guess_mod(&rel, installed));
            out.push(ConfigCandidate {
                relative_path: rel,
                mod_id: m.map(|m| m.mod_id.clone()),
                mod_name: m.map(|m| m.name.clone()),
                size_bytes: e.metadata().map(|m| m.len() as f64).unwrap_or(0.0),
            });
        }
    }
    out.sort_by_key(|c| c.relative_path.to_lowercase());
    out
}

/// Reads chosen config files for saving into a modlist. `mod_id` falls back to the file name.
pub fn read_for_modlist(data_dir: &Path, chosen: &[(String, Option<String>)]) -> (Vec<ModlistConfig>, Vec<String>) {
    let root = data_dir.join(MOD_CONFIG_DIR);
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (rel, mod_id) in chosen {
        let Some(clean) = clean_relative(rel) else {
            errors.push(format!("{rel}: not inside the ModConfig folder"));
            continue;
        };
        match std::fs::read_to_string(root.join(&clean)) {
            Ok(content) => {
                let file_name = clean.file_name().map(|n| n.to_string_lossy().to_string());
                let id = mod_id
                    .clone()
                    .filter(|m| !m.trim().is_empty())
                    .unwrap_or_else(|| clean.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default());
                out.push(ModlistConfig { mod_id: Some(id), file_name, relative_path: Some(rel_string(&clean)), content: Some(content) });
            }
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }
    out.sort_by(|a, b| {
        let k = |c: &ModlistConfig| (c.mod_id.clone().unwrap_or_default().to_lowercase(), c.file_name.clone().unwrap_or_default().to_lowercase());
        k(a).cmp(&k(b))
    });
    (out, errors)
}

/// Reads 2.x's `SimpleVSManagerModConfigPaths.json` (mod id -> folder + file name).
pub fn legacy_config_paths(manager_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(manager_dir.join("SimpleVSManagerModConfigPaths.json")) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id, v) in map {
        let dir = v.get("directoryPath").and_then(|d| d.as_str());
        let file = v.get("fileName").and_then(|d| d.as_str());
        if let (Some(dir), Some(file)) = (dir, file) {
            out.push((id.to_ascii_lowercase(), Path::new(dir).join(file)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cf(mod_id: &str, name: &str, rel: Option<&str>, content: &str) -> ConfigFile {
        ConfigFile { mod_id: mod_id.into(), file_name: name.into(), relative_path: rel.map(Into::into), content: content.into() }
    }

    #[test]
    fn writes_inside_modconfig_only() {
        let t = tempfile::tempdir().unwrap();
        let files = vec![
            cf("a", "a.json", None, "1"),
            cf("b", "b.json", Some("../../evil.json"), "2"),
            cf("c", "c.json", Some("sub\\c.json"), "3"),
            cf("a2", "a.json", None, "4"),
        ];
        let (n, errors) = write_configs(t.path(), &files);
        assert_eq!((n, errors.len()), (4, 0));
        let dir = t.path().join(MOD_CONFIG_DIR);
        assert_eq!(std::fs::read_to_string(dir.join("a.json")).unwrap(), "1");
        assert_eq!(std::fs::read_to_string(dir.join("b.json")).unwrap(), "2", "bad relative path falls back to the file name");
        assert_eq!(std::fs::read_to_string(dir.join("sub/c.json")).unwrap(), "3");
        assert_eq!(std::fs::read_to_string(dir.join("a_1.json")).unwrap(), "4");
        assert!(!t.path().join("evil.json").exists());
        assert!(!t.path().parent().unwrap().join("evil.json").exists());

        let old = existing_contents(t.path(), &files[..1]);
        assert_eq!(old[0].content.as_deref(), Some("1"));
    }

    #[test]
    fn finds_and_reads_candidates() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join(MOD_CONFIG_DIR);
        std::fs::create_dir_all(dir.join("rustboundmagic")).unwrap();
        std::fs::write(dir.join("ShowCraftable.json"), "{}").unwrap();
        std::fs::write(dir.join("HandyTweaks_config.json"), "{}").unwrap();
        std::fs::write(dir.join("rustboundmagic/spells.yaml"), "a: 1").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        std::fs::write(dir.join("unknown.json"), "{}").unwrap();

        let mut a = crate::modlists::apply::tests_support::installed("showcraftable", "Show Craftable");
        a.key = "a".into();
        let b = crate::modlists::apply::tests_support::installed("handytweaks", "Handy Tweaks");
        let c = crate::modlists::apply::tests_support::installed("rustboundmagic", "Rustbound Magic");
        let mods = [a, b, c];
        let found = candidates(t.path(), &mods, &[]);
        let pairs: Vec<_> = found.iter().map(|c| (c.relative_path.as_str(), c.mod_id.as_deref())).collect();
        assert_eq!(
            pairs,
            [
                ("HandyTweaks_config.json", Some("handytweaks")),
                ("rustboundmagic/spells.yaml", Some("rustboundmagic")),
                ("ShowCraftable.json", Some("showcraftable")),
                ("unknown.json", None),
            ]
        );
        let (read, errors) = read_for_modlist(t.path(), &[("unknown.json".into(), None), ("../x.json".into(), None)]);
        assert_eq!(read[0].mod_id.as_deref(), Some("unknown"));
        assert_eq!(errors.len(), 1);
    }
}

// ---------------------------------------------------------------------------------------------
// Editing config files
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConfigFormat {
    Json,
    Yaml,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConfigDocument {
    pub relative_path: String,
    pub format: ConfigFormat,
    pub text: String,
    /// Parsed JSON for the tree editor (`null` for YAML or when the file doesn't parse).
    #[cfg_attr(feature = "specta", specta(type = Option<String>))]
    #[serde(serialize_with = "serialize_tree")]
    pub tree: Option<serde_json::Value>,
    pub parse_error: Option<String>,
}

/// The tree goes to the UI as JSON text (keeps key order and big numbers exact).
fn serialize_tree<S: serde::Serializer>(v: &Option<serde_json::Value>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => s.serialize_some(&serde_json::to_string(v).unwrap_or_default()),
        None => s.serialize_none(),
    }
}

fn format_of(rel: &Path) -> ConfigFormat {
    match rel.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()) {
        Some(e) if e == "yaml" || e == "yml" => ConfigFormat::Yaml,
        _ => ConfigFormat::Json,
    }
}

/// Checks text for the file's format. JSON may have comments and trailing commas (the game
/// reads it leniently); YAML must parse.
pub fn validate(format: ConfigFormat, text: &str) -> Result<Option<serde_json::Value>, String> {
    match format {
        ConfigFormat::Json => {
            let t = text.trim_start_matches('\u{feff}');
            serde_json::from_str::<serde_json::Value>(t)
                .or_else(|_| json5::from_str::<serde_json::Value>(t))
                .map(Some)
                .map_err(|e| format!("Not valid JSON: {e}"))
        }
        ConfigFormat::Yaml => serde_norway::from_str::<serde_norway::Value>(text).map(|_| None).map_err(|e| format!("Not valid YAML: {e}")),
    }
}

pub fn read_config(data_dir: &Path, relative_path: &str) -> CoreResult<ConfigDocument> {
    let rel = clean_relative(relative_path).ok_or_else(|| CoreError::io(relative_path, std::io::Error::other("not inside the ModConfig folder")))?;
    let path = data_dir.join(MOD_CONFIG_DIR).join(&rel);
    let text = std::fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))?;
    let format = format_of(&rel);
    let (tree, parse_error) = match validate(format, &text) {
        Ok(t) => (t, None),
        Err(e) => (None, Some(e)),
    };
    Ok(ConfigDocument { relative_path: rel_string(&rel), format, text, tree, parse_error })
}

/// Saves a config file after checking it parses. The previous version is kept in
/// `backup_dir` (one copy per file) so a bad edit can be undone.
pub fn save_config(data_dir: &Path, relative_path: &str, text: &str, backup_dir: &Path) -> CoreResult<ConfigDocument> {
    let rel = clean_relative(relative_path).ok_or_else(|| CoreError::io(relative_path, std::io::Error::other("not inside the ModConfig folder")))?;
    let path = data_dir.join(MOD_CONFIG_DIR).join(&rel);
    validate(format_of(&rel), text).map_err(|message| CoreError::Parse { path: path.clone(), message })?;
    if path.is_file() {
        let keep = backup_dir.join(&rel);
        if let Some(parent) = keep.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::copy(&path, &keep).map_err(|e| CoreError::io(&keep, e))?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    crate::fsutil::write_synced(&tmp, text.as_bytes())?;
    crate::fsutil::rename_with_retry(&tmp, &path)?;
    read_config(data_dir, relative_path)
}

/// The copy kept before the last save, if any.
pub fn previous_version(backup_dir: &Path, relative_path: &str) -> Option<String> {
    std::fs::read_to_string(backup_dir.join(clean_relative(relative_path)?)).ok()
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn edits_json_and_yaml_safely() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join(MOD_CONFIG_DIR);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.json"), "{ // comment\n \"x\": 1, }").unwrap();
        std::fs::write(dir.join("sub/b.yaml"), "a: 1\n").unwrap();
        let backups = t.path().join("bak");

        let doc = read_config(t.path(), "a.json").unwrap();
        assert_eq!(doc.format, ConfigFormat::Json);
        assert_eq!(doc.tree.as_ref().unwrap()["x"], 1);
        assert!(save_config(t.path(), "a.json", "{ broken", &backups).is_err());
        assert!(std::fs::read_to_string(dir.join("a.json")).unwrap().contains("comment"), "bad text never written");
        save_config(t.path(), "a.json", "{\"x\": 2}", &backups).unwrap();
        assert!(previous_version(&backups, "a.json").unwrap().contains("comment"));

        let y = read_config(t.path(), "sub/b.yaml").unwrap();
        assert_eq!((y.format, y.tree.is_none()), (ConfigFormat::Yaml, true));
        assert!(save_config(t.path(), "sub/b.yaml", "a: [1, 2\n", &backups).is_err());
        save_config(t.path(), "sub/b.yaml", "a: 2\n", &backups).unwrap();
        assert!(read_config(t.path(), "../outside.json").is_err());
    }
}
