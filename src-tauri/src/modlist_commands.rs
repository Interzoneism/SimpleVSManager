//! Modlists, presets, groups and automatic backups. Logic lives in `svsm_core::modlists`;
//! running a load plan (downloads, installs, removals) happens here.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_specta::Event;

use svsm_core::clientsettings::ClientSettings;
use svsm_core::moddb::parse;
use svsm_core::modlists::configs::{self, ConfigCandidate};
use svsm_core::modlists::groups::{self, GroupDefinition};
use svsm_core::modlists::pdf::{self, PdfExport, PdfModLine};
use svsm_core::modlists::{
    self, from_installed, ApplyPlan, ConfigList, LoadMode, Modlist, ModlistKind, ModlistMeta, ModlistStore, ModlistSummary,
};
use svsm_core::mods::InstalledMod;

use crate::commands::{current_list, ModList};
use crate::error::{AppError, AppResult};
use crate::events::ModlistProgress;
use crate::moddb_commands::{emit, install_new, prune_cache, update_one, UpdateStage};
use crate::state::AppState;

fn store(state: &AppState) -> ModlistStore {
    ModlistStore::new(&state.manager_dir())
}

fn installed(state: &AppState) -> Vec<InstalledMod> {
    state.mods.read().expect("mods lock poisoned").mods.clone()
}

/// Saves the installed mods as an automatic backup. Best effort: failures are only logged.
pub(crate) fn snapshot_installed(state: &AppState, trigger: &str, extra_configs: Vec<modlists::ModlistConfig>) -> Option<String> {
    let mods = installed(state);
    if mods.is_empty() {
        return None;
    }
    let env = state.environment();
    let meta = ModlistMeta { name: trigger.into(), game_version: env.game_version.clone(), ..Default::default() };
    match store(state).snapshot(trigger, from_installed(&mods, meta, true, extra_configs)) {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!("Could not save a backup of the installed mods: {e}");
            None
        }
    }
}

/// (mod id, version) of every mod in every saved modlist, preset, group and backup.
pub(crate) fn referenced_versions(state: &AppState) -> Vec<(String, String)> {
    store(state)
        .list()
        .into_iter()
        .flat_map(|s| s.mods.into_iter().filter_map(|m| Some((m.mod_id, m.version?))))
        .collect()
}

/// Fills in which entries match the installed mods and what each would need to download.
fn annotate(state: &AppState, mut lists: Vec<ModlistSummary>) -> Vec<ModlistSummary> {
    let mods = installed(state);
    let db = state.moddb().ok();
    let s = store(state);
    let registry = crate::cloud_commands::cached_registry(state);
    for sub in lists.iter_mut().filter_map(|l| l.subscription.as_mut()) {
        match registry.iter().find(|e| e.registry_id == sub.registry_id) {
            Some(e) => {
                sub.remote_revision = sub.remote_revision.max(e.revision);
                sub.update_available = sub.remote_revision > sub.synced_revision;
            }
            None => sub.source_gone = !registry.is_empty(),
        }
    }
    for summary in lists.iter_mut().filter(|l| l.error.is_none()) {
        let Ok(loaded) = s.load(&summary.id) else { continue };
        summary.active = modlists::apply::matches_installed(&loaded.modlist, &mods);
        let plan = modlists::plan(&loaded.modlist, &mods, LoadMode::Replace, false);
        let (_, download) = modlists::apply::library_split(&plan, |id, v| db.as_ref().is_some_and(|d| d.cached_mods().locate(id, v, None).is_some()));
        summary.downloads_needed = download;
    }
    lists
}

#[tauri::command]
#[specta::specta]
pub async fn list_modlists(state: State<'_, AppState>) -> AppResult<Vec<ModlistSummary>> {
    if state.mods.read().expect("mods lock poisoned").mods.is_empty() {
        let _ = state.rescan();
    }
    Ok(annotate(&state, store(&state).list()))
}

/// Moves modlists, presets, backups or groups to the system trash.
#[tauri::command]
#[specta::specta]
pub async fn delete_modlists(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<Vec<ModlistSummary>> {
    let s = store(&state);
    s.delete(&ids)?;
    Ok(annotate(&state, s.list()))
}

// ---------------------------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveDefaults {
    /// Player name from clientsettings.json, used as "Made by".
    pub uploader: Option<String>,
    pub game_version: Option<String>,
    /// Config files in `ModConfig`, matched to installed mods where possible.
    pub configs: Vec<ConfigCandidate>,
}

#[tauri::command]
#[specta::specta]
pub async fn modlist_save_defaults(state: State<'_, AppState>) -> AppResult<SaveDefaults> {
    let (data, _) = state.mod_roots()?;
    let mods = installed(&state);
    let known = configs::legacy_config_paths(&state.manager_dir());
    Ok(SaveDefaults {
        uploader: ClientSettings::load(&data).ok().and_then(|c| c.player_name()),
        game_version: state.environment().game_version,
        configs: configs::candidates(&data, &mods, &known),
    })
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChoice {
    pub relative_path: String,
    pub mod_id: Option<String>,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveModlistRequest {
    /// `modlist` (versions, exclusive) or `preset` (only on/off).
    pub kind: ModlistKind,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub game_version: Option<String>,
    pub uploader: Option<String>,
    pub configs: Vec<ConfigChoice>,
    /// Replace an existing file with the same name.
    pub overwrite: bool,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub saved: ModlistSummary,
    /// Config files that couldn't be read (the rest were saved).
    pub warnings: Vec<String>,
}

fn build_from_installed(state: &AppState, req: &SaveModlistRequest) -> AppResult<(Modlist, Vec<String>)> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::msg("Give it a name."));
    }
    let (data, _) = state.mod_roots()?;
    let chosen: Vec<(String, Option<String>)> = req.configs.iter().map(|c| (c.relative_path.clone(), c.mod_id.clone())).collect();
    let (configs, warnings) = configs::read_for_modlist(&data, &chosen);
    let preset = req.kind == ModlistKind::Preset;
    let meta = ModlistMeta {
        name: name.to_string(),
        description: if preset { None } else { req.description.clone() },
        version: if preset { None } else { req.version.clone() },
        game_version: req.game_version.clone().or_else(|| state.environment().game_version),
        uploader: if preset { None } else { req.uploader.clone() },
    };
    Ok((from_installed(&installed(state), meta, !preset, configs), warnings))
}

/// Saves the installed mods as a modlist (`Modlists/`) or preset (`Presets/`).
#[tauri::command]
#[specta::specta]
pub async fn save_modlist(state: State<'_, AppState>, request: SaveModlistRequest) -> AppResult<SaveResult> {
    if !matches!(request.kind, ModlistKind::Modlist | ModlistKind::Preset) {
        return Err(AppError::msg("Only modlists and presets can be saved here."));
    }
    let (list, warnings) = build_from_installed(&state, &request)?;
    let s = store(&state);
    let id = s.save(request.kind, request.name.trim(), &list, request.overwrite)?;
    Ok(SaveResult { saved: s.summary(&id)?, warnings })
}

// ---------------------------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveGroupRequest {
    /// Existing group id; `null` creates one.
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    /// Modlist ids (`Modlists/x.json`) or existing member references, in order.
    pub members: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn save_modlist_group(state: State<'_, AppState>, request: SaveGroupRequest) -> AppResult<ModlistSummary> {
    let s = store(&state);
    let name = request.name.trim();
    if name.is_empty() {
        return Err(AppError::msg("Give the group a name."));
    }
    let mut def = match request.id.as_deref() {
        Some(id) => s.read_group(&s.resolve(id)?.0)?,
        None => GroupDefinition::default(),
    };
    let group_path = s.resolve(&s.id_for_name(ModlistKind::Group, name))?.0;
    let mut members: Vec<String> = Vec::new();
    for m in &request.members {
        let reference = match s.resolve(m) {
            Ok((path, ModlistKind::Group)) => {
                return Err(AppError::msg(format!("A group can't contain another group ({}).", path.display())));
            }
            Ok((path, _)) => groups::create_reference(&group_path, &path),
            Err(_) if def.members.iter().any(|r| groups::references_equal(r, m)) => m.clone(),
            Err(e) => return Err(e.into()),
        };
        if !members.iter().any(|r| groups::references_equal(r, &reference)) {
            members.push(reference);
        }
    }
    let t = |v: &Option<String>| v.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    def.name = Some(name.to_string());
    def.description = t(&request.description);
    def.version = t(&request.version);
    // Forget choices for members that left.
    def.conflict_resolutions.retain(|_, r| members.iter().any(|m| groups::references_equal(m, r)));
    def.members = members;
    let id = s.save_group(request.id.as_deref(), &def)?;
    Ok(s.summary(&id)?)
}

/// Picks which member's entry a group uses for a mod listed differently by several members.
#[tauri::command]
#[specta::specta]
pub async fn resolve_group_conflict(state: State<'_, AppState>, id: String, mod_id: String, reference: String) -> AppResult<ModlistSummary> {
    let s = store(&state);
    let (path, kind) = s.resolve(&id)?;
    if kind != ModlistKind::Group {
        return Err(AppError::msg("That isn't a modlist group."));
    }
    let mut def = s.read_group(&path)?;
    if !def.members.iter().any(|m| groups::references_equal(m, &reference)) {
        return Err(AppError::msg("That modlist isn't in the group."));
    }
    def.conflict_resolutions.retain(|k, _| !k.eq_ignore_ascii_case(&mod_id));
    def.conflict_resolutions.insert(mod_id, reference);
    s.save_group(Some(&id), &def)?;
    Ok(s.summary(&id)?)
}

// ---------------------------------------------------------------------------------------------
// Import / export
// ---------------------------------------------------------------------------------------------

fn checked_file(path: &str, exts: &[&str]) -> AppResult<PathBuf> {
    let p = PathBuf::from(path);
    let ok = p.extension().is_some_and(|e| exts.iter().any(|x| e.to_string_lossy().eq_ignore_ascii_case(x)));
    if !ok {
        return Err(AppError::msg(format!("Pick a {} file.", exts.join(" or ").to_uppercase())));
    }
    Ok(p)
}

/// Copies a modlist file (JSON or a PDF from Simple VS Manager) into `Modlists/`.
#[tauri::command]
#[specta::specta]
pub async fn import_modlist(state: State<'_, AppState>, path: String, overwrite: bool) -> AppResult<ModlistSummary> {
    let p = checked_file(&path, &["json", "pdf"])?;
    let s = store(&state);
    let id = s.import(&p, overwrite)?;
    Ok(s.summary(&id)?)
}

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Json,
    Pdf,
}

/// Writes a saved modlist to a file the user picked, as JSON or as a PDF that 2.3.0 can load.
#[tauri::command]
#[specta::specta]
pub async fn export_modlist(state: State<'_, AppState>, id: String, path: String, format: ExportFormat) -> AppResult<()> {
    let loaded = store(&state).load(&id)?;
    match format {
        ExportFormat::Json => {
            let p = checked_file(&path, &["json"])?;
            let text = modlists::to_json(&loaded.modlist);
            let tmp = p.with_extension("json.tmp");
            std::fs::write(&tmp, text).map_err(|e| AppError::msg(format!("Could not write {}: {e}", p.display())))?;
            std::fs::rename(&tmp, &p).map_err(|e| AppError::msg(format!("Could not write {}: {e}", p.display())))?;
        }
        ExportFormat::Pdf => {
            let p = checked_file(&path, &["pdf"])?;
            write_pdf(&state, &loaded.modlist, &p)?;
        }
    }
    Ok(())
}

fn write_pdf(state: &AppState, list: &Modlist, path: &Path) -> AppResult<()> {
    // Names and Mod DB links from installed mods and the Mod DB cache; ids otherwise.
    let mods = installed(state);
    let entries = list.entries();
    let ids: Vec<String> = entries.iter().filter_map(|m| m.id().map(String::from)).collect();
    let cached = state.moddb().map(|db| db.cached(&ids)).unwrap_or_default();
    let lines = entries
        .iter()
        .filter_map(|m| m.id().map(|id| (id, m.version())))
        .map(|(id, version)| {
            let info = cached.get(&id.to_ascii_lowercase()).and_then(|c| c.raw.as_ref()).map(|raw| parse::build_info(id, raw, None, false, 0.0));
            let title = mods
                .iter()
                .find(|m| m.mod_id.eq_ignore_ascii_case(id))
                .map(|m| m.name.clone())
                .or_else(|| info.as_ref().and_then(|i| i.name.clone()))
                .unwrap_or_else(|| id.to_string());
            PdfModLine { title, version: version.map(String::from), url: info.and_then(|i| i.page_url) }
        })
        .collect();
    // Configs go in the PDF's separate config list, like 2.x.
    let mut body = list.clone();
    let configs: Vec<modlists::ModlistConfig> = list
        .config_files()
        .into_iter()
        .map(|c| modlists::ModlistConfig { mod_id: Some(c.mod_id), file_name: Some(c.file_name), relative_path: c.relative_path, content: Some(c.content) })
        .collect();
    body.configurations = None;
    for m in body.mods.iter_mut().flatten() {
        m.configuration_content = None;
        m.configuration_file_name = None;
    }
    let export = PdfExport { modlist: &body, configs: (!configs.is_empty()).then_some(ConfigList { configurations: Some(configs) }), lines };
    pdf::write_pdf(path, &export)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------------------------

/// What to load: a saved entry (`id`), a file the user picked (`file`, JSON or PDF), or an
/// online modlist (`online`, its registry id).
#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModlistSource {
    pub id: Option<String>,
    pub file: Option<String>,
    pub online: Option<String>,
}

async fn load_source(state: &AppState, source: &ModlistSource) -> AppResult<(Modlist, Vec<groups::GroupConflict>, Option<ModlistKind>)> {
    if let Some(id) = &source.id {
        let l = store(state).load(id)?;
        return Ok((l.modlist, l.conflicts, Some(l.kind)));
    }
    if let Some(file) = &source.file {
        let p = checked_file(file, &["json", "pdf"])?;
        return Ok((modlists::read_modlist_file(&p)?, Vec::new(), None));
    }
    if let Some(registry_id) = &source.online {
        let (_, list) = crate::cloud_commands::fetch_online(state, registry_id).await?;
        return Ok((list, Vec::new(), None));
    }
    Err(AppError::msg("Nothing to load."))
}

/// Never remove anything from the game's own install folder (its `Mods` holds the base game).
fn protect_game_folder(state: &AppState, plan: &mut ApplyPlan) {
    let Ok((_, Some(game))) = state.mod_roots() else { return };
    let mods = installed(state);
    plan.remove.retain(|r| {
        mods.iter().find(|m| m.key == r.key).is_none_or(|m| !Path::new(&m.source_path).starts_with(&game))
    });
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModlistPreview {
    pub name: String,
    pub kind: Option<ModlistKind>,
    pub mod_count: u32,
    pub has_versions: bool,
    pub has_status: bool,
    pub plan: ApplyPlan,
    /// Installs and version switches that come from the mod library (no download).
    pub from_library: u32,
    /// Installs and version switches that need the Mod DB.
    pub to_download: u32,
    /// Config files the list carries (paths under ModConfig).
    pub config_files: Vec<String>,
    pub conflicts: Vec<groups::GroupConflict>,
}

/// Works out what loading a modlist would change, without changing anything.
#[tauri::command]
#[specta::specta]
pub async fn preview_modlist(state: State<'_, AppState>, source: ModlistSource, mode: LoadMode) -> AppResult<ModlistPreview> {
    let (list, conflicts, kind) = load_source(&state, &source).await?;
    state.rescan()?;
    let mut plan = modlists::plan(&list, &installed(&state), mode, true);
    protect_game_folder(&state, &mut plan);
    let config_files = configs::targets(&plan.configs).iter().map(|p| p.display().to_string().replace('\\', "/")).collect();
    let db = state.moddb()?;
    let (from_library, to_download) = modlists::apply::library_split(&plan, |id, v| db.cached_mods().locate(id, v, None).is_some());
    Ok(ModlistPreview {
        from_library,
        to_download,
        name: list.display_name().unwrap_or("Modlist").to_string(),
        kind,
        mod_count: list.entries().len() as u32,
        has_versions: list.has_versions(),
        has_status: list.has_status(),
        plan,
        config_files,
        conflicts,
    })
}

#[derive(Debug, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub installed: u32,
    pub changed: u32,
    /// Removed mods (moved into the mod library, or to the trash when they can't be).
    pub removed: u32,
    /// Of `removed`, how many went into the mod library.
    pub to_library: u32,
    pub toggled: u32,
    pub configs_written: u32,
    pub failures: Vec<String>,
    pub skipped: Vec<String>,
    pub cancelled: bool,
    /// The automatic backup saved before anything changed.
    pub backup_id: Option<String>,
    pub list: Option<ModList>,
}

fn progress<R: tauri::Runtime>(app: &AppHandle<R>, step: u32, total: u32, label: impl Into<String>) {
    let _ = ModlistProgress { step, total, label: label.into() }.emit(app);
}

/// Loads a modlist: removes (Replace), switches versions, installs, sets on/off, writes
/// configs. Saves a backup of the current mods first. Emits `ModlistProgress` and
/// `UpdateProgress` (download percent, key `modlist:<mod id>`).
#[tauri::command]
#[specta::specta]
pub async fn apply_modlist(
    app: AppHandle,
    state: State<'_, AppState>,
    source: ModlistSource,
    mode: LoadMode,
    import_configs: bool,
) -> AppResult<ApplyReport> {
    run_apply(&app, &state, source, mode, import_configs).await
}

pub(crate) async fn run_apply<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    source: ModlistSource,
    mode: LoadMode,
    import_configs: bool,
) -> AppResult<ApplyReport> {
    let _run = state.update_lock.lock().await;
    state.cancel_updates.store(false, Ordering::Relaxed);
    let (list, _, _) = load_source(state, &source).await?;
    let (data, _) = state.mod_roots()?;
    state.rescan()?;
    let mut plan = modlists::plan(&list, &installed(state), mode, import_configs);
    protect_game_folder(state, &mut plan);
    let mut report = ApplyReport { skipped: plan.skipped.clone(), ..Default::default() };
    if plan.is_empty() {
        report.list = Some(current_list(state)?);
        return Ok(report);
    }

    // Backup first, including the config files that are about to be overwritten.
    report.backup_id = snapshot_installed(state, "ModlistLoaded", configs::existing_contents(&data, &plan.configs));

    let net_steps = (plan.install.len() + plan.change_version.len()) as u32;
    let total = plan.remove.len() as u32 + net_steps + 2;
    let mut step = 0;
    let cancelled = || state.cancel_updates.load(Ordering::Relaxed);

    let db = state.moddb()?;
    let env = state.environment();

    // 1. Take mods that aren't in the list out of the Mods folder (Replace), so new files can't
    //    collide with them. Zips go into the mod library for switching back; the rest to the trash.
    if !plan.remove.is_empty() {
        let search_paths = state.mods.read().expect("mods lock poisoned").search_paths.clone();
        let _guard = state.mods_lock.lock().expect("mods lock poisoned");
        for r in &plan.remove {
            step += 1;
            progress(app, step, total, format!("Putting away {}", r.name));
            let result = state.find_mod(&r.key).and_then(|m| {
                let path = Path::new(&m.source_path);
                if !search_paths.iter().any(|sp| Some(sp.as_path()) == path.parent()) {
                    return Err(AppError::msg("not in a mod folder"));
                }
                let version = m.version.as_deref().map(str::trim).filter(|v| !v.is_empty());
                if let (svsm_core::mods::ModSourceKind::Zip, Some(v), false) = (m.source_kind, version, m.mod_id.trim().is_empty()) {
                    if db.cached_mods().store_move(&m.mod_id, v, Some(&m.file_name), path).is_ok() {
                        return Ok(true);
                    }
                }
                svsm_core::mods::delete_mod(path, &search_paths).map(|_| false).map_err(AppError::from)
            });
            match result {
                Ok(to_library) => {
                    report.removed += 1;
                    report.to_library += to_library as u32;
                }
                Err(e) => report.failures.push(format!("{}: {}", r.name, e.message)),
            }
        }
    }
    state.rescan()?;

    // 2. Switch versions of installed mods.
    for c in &plan.change_version {
        if cancelled() {
            report.cancelled = true;
            break;
        }
        step += 1;
        progress(app, step, total, format!("Installing {} {}", c.name, c.to));
        let result = match state.find_mod(&c.key) {
            Ok(m) => update_one(app, state, &db, &env, &m, Some(&c.to)).await.map(|_| ()),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => {
                report.changed += 1;
                emit(app, &c.key, UpdateStage::Done, None, None);
            }
            Err(e) => {
                report.cancelled |= e.message == "Cancelled.";
                report.failures.push(format!("{} {}: {}", c.name, c.to, e.message));
            }
        }
    }

    // 3. Install missing mods.
    for i in &plan.install {
        if cancelled() {
            report.cancelled = true;
            break;
        }
        step += 1;
        let label = match &i.version {
            Some(v) => format!("Downloading {} {v}", i.mod_id),
            None => format!("Downloading {}", i.mod_id),
        };
        progress(app, step, total, label);
        let key = format!("modlist:{}", i.mod_id);
        match install_new(app, state, &db, &env, &key, &i.mod_id, i.version.as_deref()).await {
            Ok(_) => {
                report.installed += 1;
                emit(app, &key, UpdateStage::Done, None, None);
            }
            Err(e) => {
                report.cancelled |= e.message == "Cancelled.";
                let what = match &i.version {
                    Some(v) => format!("{} {v}", i.mod_id),
                    None => i.mod_id.clone(),
                };
                report.failures.push(format!("{what}: {}", e.message));
            }
        }
    }

    // 4. On/off state, for whatever is installed now.
    step += 1;
    progress(app, step, total, "Switching mods on and off");
    state.rescan()?;
    {
        let _guard = state.mods_lock.lock().expect("mods lock poisoned");
        let mods = installed(state);
        let mut cs = ClientSettings::load(&data)?;
        let mut changed = false;
        if let Some(entries) = plan.disabled_entries.clone() {
            changed |= cs.replace_disabled(entries);
        }
        for s in &plan.states {
            for m in mods.iter().filter(|m| m.mod_id.eq_ignore_ascii_case(&s.mod_id)) {
                if m.is_active != s.active {
                    report.toggled += 1;
                }
                changed |= cs.set_active(&m.mod_id, m.version.as_deref(), s.active);
            }
        }
        if changed {
            cs.save()?;
        }
    }

    // 5. Config files.
    step += 1;
    progress(app, step, total, "Writing config files");
    if !plan.configs.is_empty() {
        let (n, errors) = configs::write_configs(&data, &plan.configs);
        report.configs_written = n;
        report.failures.extend(errors.into_iter().map(|e| format!("Config {e}")));
    }

    state.rescan()?;
    prune_cache(state, &db, &env);
    report.list = Some(current_list(state)?);
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Details
// ---------------------------------------------------------------------------------------------

/// What the Mod DB and the mod library know about one mod of a modlist.
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModlistModCard {
    pub mod_id: String,
    pub name: Option<String>,
    pub author: Option<String>,
    pub side: Option<String>,
    pub logo_url: Option<String>,
    pub page_url: Option<String>,
    pub downloads: Option<u32>,
    /// Installed version, if installed.
    pub installed_version: Option<String>,
    /// Versions kept in the mod library.
    pub library_versions: Vec<String>,
    /// Looked up and not on the Mod DB.
    pub not_on_mod_db: bool,
}

/// Mod DB info (names, logos, downloads) and library versions for a modlist's mods. Uses the
/// Mod DB cache; `refresh` asks the Mod DB for anything stale (unless offline).
#[tauri::command]
#[specta::specta]
pub async fn modlist_mod_cards(state: State<'_, AppState>, mod_ids: Vec<String>, refresh: bool) -> AppResult<Vec<ModlistModCard>> {
    let db = state.moddb()?;
    let found = if refresh { db.lookup(&mod_ids, false).await.mods } else { db.cached(&mod_ids) };
    let mods = installed(&state);
    Ok(mod_ids
        .iter()
        .map(|id| {
            let cached = found.get(&id.to_ascii_lowercase());
            let info = cached.and_then(|c| c.raw.as_ref()).map(|raw| parse::build_info(id, raw, None, false, 0.0));
            ModlistModCard {
                mod_id: id.clone(),
                name: info.as_ref().and_then(|i| i.name.clone()),
                author: info.as_ref().and_then(|i| i.author.clone()),
                side: info.as_ref().and_then(|i| i.side.clone()),
                logo_url: info.as_ref().and_then(|i| i.logo_url.clone()),
                page_url: info.as_ref().and_then(|i| i.page_url.clone()),
                downloads: info.as_ref().and_then(|i| i.downloads),
                installed_version: mods.iter().find(|m| m.mod_id.eq_ignore_ascii_case(id)).and_then(|m| m.version.clone()),
                library_versions: db.cached_mods().versions(id).into_iter().map(|v| v.version).collect(),
                not_on_mod_db: cached.is_some_and(|c| c.raw.is_none()),
            }
        })
        .collect())
}
