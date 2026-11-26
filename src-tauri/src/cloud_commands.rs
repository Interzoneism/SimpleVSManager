//! Online modlists (browse, subscribe, publish) and user reports. Logic in `svsm_core::cloud`.

use svsm_core::sync::RwLockExt as _;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use svsm_core::clientsettings::ClientSettings;
use svsm_core::cloud::store::SLOTS;
use svsm_core::cloud::votes::{VoteOption, VoteSummary, Votes};
use svsm_core::cloud::{validator, CloudStore, DetailsEdit, PlayerIdentity, RegistryEntry, SlotInfo};
use svsm_core::modlists::{self, Modlist, ModlistKind, ModlistStore, ModlistSummary, OnlineSubscription, PublishedLink};
use svsm_core::settings::set_flag;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn err(e: svsm_core::cloud::CloudError) -> AppError {
    AppError::msg(e.to_string())
}

pub(crate) fn cache_dir(state: &AppState) -> PathBuf {
    state.manager_dir().join("Temp Cache").join("Firebase Cache")
}

fn player(state: &AppState) -> AppResult<PlayerIdentity> {
    let (data, _) = state.mod_roots()?;
    let cs = ClientSettings::load(&data)?;
    PlayerIdentity::new(cs.player_uid(), cs.player_name()).map_err(err)
}

/// Cached registry listing (no network), for marking subscribed copies.
pub(crate) fn cached_registry(state: &AppState) -> Vec<RegistryEntry> {
    let Ok(auth) = state.cloud_auth() else {
        // Offline: still read the cache file directly.
        let auth = svsm_core::cloud::Authenticator::new(&state.manager_dir(), None, state.cloud_config.clone());
        return auth
            .ok()
            .and_then(|a| CloudStore::new(&a, state.cloud_config.clone(), &cache_dir(state)).ok().map(|s| s.cached_registry()))
            .unwrap_or_default();
    };
    CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(state)).map(|s| s.cached_registry()).unwrap_or_default()
}

/// Full content of an online modlist, as a modlist.
pub(crate) async fn fetch_online(state: &AppState, registry_id: &str) -> AppResult<(RegistryEntry, Modlist)> {
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(state)).map_err(err)?;
    let (entry, content) = cloud
        .registry_content(registry_id)
        .await
        .map_err(err)?
        .ok_or_else(|| AppError::msg("That online modlist no longer exists."))?;
    let mut list = modlists::parse_modlist(&content.to_string()).map_err(AppError::msg)?;
    list.online_subscription = None;
    Ok((entry, list))
}

// ---------------------------------------------------------------------------------------------
// Status and browsing
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub offline: bool,
    /// A cloud identity exists (2.x `firebase-auth.json` or one made here).
    pub has_identity: bool,
    pub player_name: Option<String>,
    /// Why publishing isn't possible yet (no player id/name in clientsettings.json).
    pub player_problem: Option<String>,
    /// The identity is on the old cloud project (publishing and reports need the move).
    pub legacy_identity: bool,
    /// Signing in failed for another reason.
    pub sign_in_problem: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_status(state: State<'_, AppState>) -> AppResult<CloudStatus> {
    let offline = state.environment().settings.offline_mode;
    let has_identity = svsm_core::cloud::auth::read_identity(&state.manager_dir().join(svsm_core::cloud::auth::IDENTITY_FILE)).is_some()
        || svsm_core::cloud::Authenticator::default_backup_file().is_some_and(|b| svsm_core::cloud::auth::read_identity(&b).is_some());
    let (player_name, player_problem) = match player(&state) {
        Ok(p) => (Some(p.name), None),
        Err(e) => (None, Some(e.message)),
    };
    let mut legacy_identity = svsm_core::cloud::migrate::is_legacy_identity(&state.manager_dir());
    let mut sign_in_problem = None;
    if has_identity && !offline && !legacy_identity {
        if let Ok(auth) = state.cloud_auth() {
            match auth.session(false).await {
                Ok(_) => {}
                Err(svsm_core::cloud::CloudError::LegacyIdentity) => legacy_identity = true,
                Err(e) => sign_in_problem = Some(e.to_string()),
            }
        }
    }
    Ok(CloudStatus { offline, has_identity, player_name, player_problem, legacy_identity, sign_in_problem })
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OnlineEntry {
    pub entry: RegistryEntry,
    pub subscribers: u32,
    pub subscribed: bool,
    /// Published by this player.
    pub mine: bool,
    /// Id of the local copy, if there is one.
    pub local_id: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OnlineListing {
    pub entries: Vec<OnlineEntry>,
    pub hidden_modlists: u32,
    pub hidden_authors: u32,
    /// Shown from the cache because the cloud couldn't be reached.
    pub error: Option<String>,
}

fn local_copies(state: &AppState) -> Vec<(String, String)> {
    let s = ModlistStore::new(&state.manager_dir());
    s.list()
        .into_iter()
        .filter_map(|l| Some((l.subscription.as_ref()?.registry_id.clone(), l.id)))
        .collect()
}

/// Public modlists, newest first, without the ones the user hid.
#[tauri::command]
#[specta::specta]
pub async fn online_modlists(state: State<'_, AppState>) -> AppResult<OnlineListing> {
    let settings = state.environment().settings;
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    let (entries, error) = match cloud.registry().await {
        Ok(e) => (e, None),
        Err(e) => (cloud.cached_registry(), Some(e.to_string())),
    };
    let subs = cloud.subscriptions().await.unwrap_or_default();
    let owned: HashSet<String> = match player(&state) {
        Ok(p) => cloud.my_slots(&p).await.unwrap_or_default().into_iter().filter_map(|s| s.registry_id).collect(),
        Err(_) => HashSet::new(),
    };
    let copies = local_copies(&state);
    let hidden_author = |e: &RegistryEntry| e.uploader.as_deref().is_some_and(|u| settings.hidden_online_authors.iter().any(|h| h.eq_ignore_ascii_case(u)));
    let hidden_list = |e: &RegistryEntry| settings.hidden_online_modlists.iter().any(|h| h == &e.registry_id);
    let visible: Vec<OnlineEntry> = entries
        .into_iter()
        .filter(|e| owned.contains(&e.registry_id) || !(hidden_list(e) || hidden_author(e)))
        .map(|e| OnlineEntry {
            subscribers: subs.counts.get(&e.registry_id).copied().unwrap_or(0),
            subscribed: subs.mine.contains(&e.registry_id),
            mine: owned.contains(&e.registry_id),
            local_id: copies.iter().find(|(r, _)| r == &e.registry_id).map(|(_, id)| id.clone()),
            entry: e,
        })
        .collect();
    Ok(OnlineListing {
        entries: visible,
        hidden_modlists: settings.hidden_online_modlists.len() as u32,
        hidden_authors: settings.hidden_online_authors.len() as u32,
        error,
    })
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HideRequest {
    pub registry_id: Option<String>,
    pub author: Option<String>,
    /// Show everything again.
    pub clear: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn hide_online(state: State<'_, AppState>, request: HideRequest) -> AppResult<()> {
    state.update_settings(|s| {
        if request.clear {
            s.hidden_online_modlists.clear();
            s.hidden_online_authors.clear();
        }
        if let Some(id) = request.registry_id.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
            if !s.hidden_online_modlists.iter().any(|h| h == id) {
                s.hidden_online_modlists.push(id.to_string());
            }
        }
        if let Some(a) = request.author.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            set_flag(&mut s.hidden_online_authors, a, true);
        }
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Subscriptions (local copies that follow an online modlist)
// ---------------------------------------------------------------------------------------------

fn write_copy(state: &AppState, name: &str, list: &Modlist, overwrite_id: Option<&str>) -> AppResult<String> {
    let s = ModlistStore::new(&state.manager_dir());
    match overwrite_id {
        Some(id) => {
            let (path, kind) = s.resolve(id)?;
            if kind != ModlistKind::Modlist {
                return Err(AppError::msg("Only modlists can follow an online modlist."));
            }
            std::fs::write(&path, modlists::to_json(list)).map_err(|e| AppError::msg(format!("Could not write {}: {e}", path.display())))?;
            Ok(id.to_string())
        }
        None => {
            let mut candidate = name.to_string();
            let mut n = 2;
            while s.resolve(&s.id_for_name(ModlistKind::Modlist, &candidate))?.0.exists() {
                candidate = format!("{name} ({n})");
                n += 1;
            }
            Ok(s.save(ModlistKind::Modlist, &candidate, list, false)?)
        }
    }
}

/// Saves a local copy of an online modlist. `follow` = subscribe (the copy can be updated when
/// the author publishes a new revision); otherwise it's an ordinary local modlist.
#[tauri::command]
#[specta::specta]
pub async fn save_online_copy(state: State<'_, AppState>, registry_id: String, follow: bool) -> AppResult<ModlistSummary> {
    let (entry, mut list) = fetch_online(&state, &registry_id).await?;
    let name = list.display_name().unwrap_or("Online modlist").to_string();
    let mut warning = None;
    if follow {
        let now = chrono::Utc::now().to_rfc3339();
        list.online_subscription = Some(OnlineSubscription {
            registry_id: Some(registry_id.clone()),
            synced_revision: entry.revision,
            last_known_remote_revision: entry.revision,
            subscribed_at_utc: Some(now.clone()),
            last_synced_at_utc: Some(now),
            last_checked_at_utc: None,
        });
        let auth = state.cloud_auth()?;
        let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
        // Counting subscribers is best effort (the database may not allow it yet).
        if let Err(e) = cloud.subscribe(&registry_id).await {
            warning = Some(e.to_string());
        }
    }
    let id = write_copy(&state, &name, &list, None)?;
    if let Some(w) = warning {
        tracing::warn!("Subscribed locally, but the subscriber count wasn't updated: {w}");
    }
    Ok(ModlistStore::new(&state.manager_dir()).summary(&id)?)
}

/// Replaces a subscribed copy with the latest online revision.
#[tauri::command]
#[specta::specta]
pub async fn sync_subscription(state: State<'_, AppState>, id: String) -> AppResult<ModlistSummary> {
    let s = ModlistStore::new(&state.manager_dir());
    let local = s.load(&id)?.modlist;
    let sub = local.online_subscription.clone().ok_or_else(|| AppError::msg("That modlist doesn't follow an online modlist."))?;
    let registry_id = sub.registry_id.clone().unwrap_or_default();
    let (entry, mut list) = fetch_online(&state, &registry_id)
        .await
        .map_err(|e| AppError::msg(format!("{} You can stop following it to keep it as a normal modlist.", e.message)))?;
    // A followed list is exactly what the author published, icon included.
    let now = chrono::Utc::now().to_rfc3339();
    list.online_subscription = Some(OnlineSubscription {
        synced_revision: entry.revision,
        last_known_remote_revision: entry.revision,
        last_synced_at_utc: Some(now.clone()),
        last_checked_at_utc: Some(now),
        ..sub
    });
    write_copy(&state, "", &list, Some(&id))?;
    Ok(s.summary(&id)?)
}

/// Stops following: the local copy stays as a normal modlist.
#[tauri::command]
#[specta::specta]
pub async fn unsubscribe(state: State<'_, AppState>, id: String) -> AppResult<ModlistSummary> {
    let s = ModlistStore::new(&state.manager_dir());
    let mut list = s.load(&id)?.modlist;
    let registry_id = list.online_subscription.take().and_then(|o| o.registry_id);
    write_copy(&state, "", &list, Some(&id))?;
    if let (Some(rid), Ok(auth)) = (registry_id, state.cloud_auth()) {
        if let Ok(cloud) = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)) {
            let _ = cloud.unsubscribe(&rid).await;
        }
    }
    Ok(s.summary(&id)?)
}

// ---------------------------------------------------------------------------------------------
// Publishing: a saved modlist marked as published in one of the player's five cloud slots
// ---------------------------------------------------------------------------------------------

pub(crate) const SLOT_LIMIT_MESSAGE: &str =
    "You already have 5 published modlists, the most the cloud allows. Unpublish one of them first, then publish this one.";

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudSlots {
    pub player_name: String,
    /// All five slots; `null` where empty.
    pub slots: Vec<Option<CloudSlotView>>,
    /// New subscribers since last acknowledged, per registry id.
    pub new_subscribers: Vec<(String, u32)>,
    /// Saved modlists were created or linked for published lists (refresh the modlists).
    pub local_changes: bool,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudSlotView {
    pub info: SlotInfo,
    pub subscribers: u32,
    /// The saved modlist this online modlist is published from.
    pub local_id: Option<String>,
}

fn link(registry_id: &str, revision: f64, fingerprint: String) -> PublishedLink {
    PublishedLink {
        registry_id: Some(registry_id.to_string()),
        revision,
        published_at_utc: Some(chrono::Utc::now().to_rfc3339()),
        fingerprint: Some(fingerprint),
    }
}

fn same_mods(local: &ModlistSummary, remote: &Modlist) -> bool {
    let key = |id: &str, v: Option<&str>| (id.to_ascii_lowercase(), v.unwrap_or("").to_string());
    let mut a: Vec<_> = local.mods.iter().map(|m| key(&m.mod_id, m.version.as_deref())).collect();
    let mut b: Vec<_> = remote.entries().iter().filter_map(|m| Some(key(m.id()?, m.version()))).collect();
    a.sort();
    b.sort();
    a == b
}

/// Makes every published online modlist a saved modlist marked as published, so nothing is
/// listed twice and nothing is lost:
///
/// * a saved modlist already linked to a slot stays linked (links to slots that are gone,
///   because the list was unpublished elsewhere, are cleared),
/// * otherwise a saved modlist with the same name and mods (published by 2.x, or by 3.x before
///   publishing was part of saved modlists) is linked,
/// * otherwise the online content is saved as a new modlist.
///
/// Returns registry id → saved modlist id, and whether any file changed.
pub(crate) fn reconcile_published(state: &AppState, used: &[(SlotInfo, Value)]) -> (std::collections::HashMap<String, String>, bool) {
    let s = ModlistStore::new(&state.manager_dir());
    let lists: Vec<ModlistSummary> = s.list().into_iter().filter(|l| l.kind == ModlistKind::Modlist && l.error.is_none()).collect();
    let live: HashSet<&str> = used.iter().filter_map(|(i, _)| i.registry_id.as_deref()).collect();
    let mut map = std::collections::HashMap::new();
    let mut changed = false;
    let clear = |id: &str| -> bool {
        match s.load(id) {
            Ok(mut loaded) => {
                loaded.modlist.published = None;
                s.overwrite(id, &loaded.modlist).map_err(|e| tracing::warn!("Couldn't unmark {id} as published: {e}")).is_ok()
            }
            Err(_) => false,
        }
    };
    for l in &lists {
        let Some(p) = &l.published else { continue };
        if live.contains(p.registry_id.as_str()) && !map.contains_key(&p.registry_id) {
            map.insert(p.registry_id.clone(), l.id.clone());
        } else if clear(&l.id) {
            changed = true;
        }
    }
    for (info, content) in used {
        let Some(rid) = info.registry_id.as_deref() else { continue };
        if map.contains_key(rid) {
            continue;
        }
        let Ok(mut remote) = modlists::parse_modlist(&content.to_string()) else {
            tracing::warn!("Published modlist {rid} couldn't be read; it isn't shown under saved modlists.");
            continue;
        };
        remote.online_subscription = None;
        remote.published = None;
        let fingerprint = remote.publish_fingerprint();
        let name = remote.display_name().unwrap_or("Published modlist").to_string();
        let taken: HashSet<&String> = map.values().collect();
        let matching = lists.iter().find(|l| {
            l.published.is_none() && l.subscription.is_none() && !taken.contains(&l.id) && l.name.eq_ignore_ascii_case(&name) && same_mods(l, &remote)
        });
        let result = match matching {
            Some(l) => s.load(&l.id).map_err(AppError::from).and_then(|mut loaded| {
                loaded.modlist.published = Some(link(rid, info.revision, fingerprint));
                s.overwrite(&l.id, &loaded.modlist)?;
                Ok(l.id.clone())
            }),
            None => {
                remote.published = Some(link(rid, info.revision, fingerprint));
                write_copy(state, &name, &remote, None)
            }
        };
        match result {
            Ok(id) => {
                map.insert(rid.to_string(), id);
                changed = true;
            }
            Err(e) => tracing::warn!("Couldn't save published modlist {rid} as a saved modlist: {}", e.message),
        }
    }
    (map, changed)
}

#[tauri::command]
#[specta::specta]
pub async fn my_cloud_slots(state: State<'_, AppState>) -> AppResult<CloudSlots> {
    let me = player(&state)?;
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    let used = cloud.my_slots_with_content(&me).await.map_err(err)?;
    let (locals, local_changes) = reconcile_published(&state, &used);
    let subs = cloud.subscriptions().await.unwrap_or_default();
    let ids: Vec<String> = used.iter().filter_map(|(s, _)| s.registry_id.clone()).collect();
    let notices = cloud.owner_notices(&ids).await.unwrap_or_default();
    let slots = SLOTS
        .iter()
        .map(|name| {
            used.iter().find(|(s, _)| s.slot == *name).map(|(info, _)| CloudSlotView {
                subscribers: info.registry_id.as_ref().and_then(|r| subs.counts.get(r)).copied().unwrap_or(0),
                local_id: info.registry_id.as_ref().and_then(|r| locals.get(r)).cloned(),
                info: info.clone(),
            })
        })
        .collect();
    Ok(CloudSlots { player_name: me.name, slots, new_subscribers: notices.into_iter().collect(), local_changes })
}

#[tauri::command]
#[specta::specta]
pub async fn acknowledge_subscriber_notices(state: State<'_, AppState>, registry_ids: Vec<String>) -> AppResult<()> {
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    cloud.acknowledge_notices(&registry_ids).await.map_err(err)
}

/// Publishes a saved modlist, or publishes its changes when it already is published (a new
/// revision for followers only when the mods or configs changed). A new one takes the first
/// free cloud slot; with all five used, the user has to unpublish one first.
#[tauri::command]
#[specta::specta]
pub async fn publish_saved_modlist(state: State<'_, AppState>, id: String) -> AppResult<ModlistSummary> {
    let me = player(&state)?;
    let s = ModlistStore::new(&state.manager_dir());
    let loaded = s.load(&id)?;
    if loaded.kind != ModlistKind::Modlist {
        return Err(AppError::msg("Only saved modlists can be published, not groups or backups. Load it and save your mods as a modlist first."));
    }
    let mut list = loaded.modlist;
    if list.online_subscription.is_some() {
        return Err(AppError::msg("This modlist follows someone else's online modlist. Stop following it first to publish it as your own."));
    }
    let texts = [list.name.as_deref(), list.description.as_deref(), Some(me.name.as_str())];
    if let Some(word) = validator::find_blocked(texts.into_iter().flatten()) {
        return Err(AppError::msg(format!("Online modlists can't contain \"{word}\". Change the name or description first.")));
    }
    let content: Value = serde_json::from_str(&modlists::to_json(&list)).map_err(|e| AppError::msg(e.to_string()))?;
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    let used = cloud.my_slots(&me).await.map_err(err)?;
    let current = list.published.as_ref().and_then(|p| p.id()).and_then(|rid| used.iter().find(|u| u.registry_id.as_deref() == Some(rid)));
    let (slot, replaces) = match current {
        Some(u) => (u.slot.clone(), u.registry_id.clone()),
        None => {
            let free = SLOTS.iter().find(|n| !used.iter().any(|u| u.slot == **n)).ok_or_else(|| AppError::msg(SLOT_LIMIT_MESSAGE))?;
            (free.to_string(), None)
        }
    };
    let written = cloud.publish(&me, &slot, &content, replaces.as_deref()).await.map_err(err)?;
    let fingerprint = list.publish_fingerprint();
    list.published = Some(link(&written.registry_id, written.revision, fingerprint));
    s.overwrite(&id, &list)?;
    Ok(s.summary(&id)?)
}

/// Removes a saved modlist from the cloud. The saved modlist stays as a normal one, and
/// followers keep their copies (they get no more updates).
#[tauri::command]
#[specta::specta]
pub async fn unpublish_modlist(state: State<'_, AppState>, id: String) -> AppResult<ModlistSummary> {
    let s = ModlistStore::new(&state.manager_dir());
    let loaded = s.load(&id)?;
    let mut list = loaded.modlist;
    let Some(rid) = list.published.as_ref().and_then(|p| p.id()).map(String::from) else {
        return Ok(s.summary(&id)?);
    };
    let me = player(&state)?;
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    let used = cloud.my_slots(&me).await.map_err(err)?;
    if let Some(u) = used.iter().find(|u| u.registry_id.as_deref() == Some(rid.as_str())) {
        cloud.delete_slot(&me, &u.slot).await.map_err(err)?;
    }
    list.published = None;
    s.overwrite(&id, &list)?;
    Ok(s.summary(&id)?)
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditPublishedRequest {
    pub registry_id: String,
    pub edit: DetailsEdit,
}

/// Changes the description, game version or icon of one of the player's published modlists,
/// online and in its saved modlist. The name stays as it is, and so does the revision
/// (followers aren't told about it).
#[tauri::command]
#[specta::specta]
pub async fn edit_published_modlist(state: State<'_, AppState>, request: EditPublishedRequest) -> AppResult<SlotInfo> {
    let me = player(&state)?;
    let e = &request.edit;
    let texts = [e.description.as_deref(), e.game_version.as_deref()];
    if let Some(word) = validator::find_blocked(texts.into_iter().flatten()) {
        return Err(AppError::msg(format!("Online modlists can't contain \"{word}\". Change the description first.")));
    }
    let auth = state.cloud_auth()?;
    let cloud = CloudStore::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    let written = cloud.edit_details(&me, &request.registry_id, e).await.map_err(err)?;
    apply_edit_locally(&state, &request.registry_id, e);
    let slots = cloud.my_slots(&me).await.map_err(err)?;
    slots.into_iter().find(|s| s.slot == written.slot).ok_or_else(|| AppError::msg("Saved, but the slot couldn't be read back."))
}

/// The same detail edit on the saved modlist that is published as `registry_id`. Best effort.
fn apply_edit_locally(state: &AppState, registry_id: &str, e: &DetailsEdit) {
    let s = ModlistStore::new(&state.manager_dir());
    let Some(local) = s.list().into_iter().find(|l| l.published.as_ref().is_some_and(|p| p.registry_id == registry_id)) else { return };
    let Ok(loaded) = s.load(&local.id) else { return };
    let mut list = loaded.modlist;
    let in_sync = list.published.as_ref().and_then(|p| p.fingerprint.clone()) == Some(list.publish_fingerprint());
    let text = |v: &Option<String>| v.as_ref().map(|t| Some(t.trim().to_string()).filter(|t| !t.is_empty()));
    if let Some(d) = text(&e.description) {
        list.description = d;
    }
    if let Some(g) = text(&e.game_version) {
        list.game_version = g;
    }
    if let Some(i) = &e.icon {
        list.icon = modlists::clean_icon(Some(i));
    }
    if let Some(c) = &e.icon_color {
        list.icon_color = modlists::clean_icon(Some(c));
    }
    if in_sync {
        let fingerprint = list.publish_fingerprint();
        if let Some(p) = list.published.as_mut() {
            p.fingerprint = Some(fingerprint);
        }
    }
    if let Err(err) = s.overwrite(&local.id, &list) {
        tracing::warn!("Couldn't save the new details in {}: {err}", local.id);
    }
}

// ---------------------------------------------------------------------------------------------
// User reports
// ---------------------------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn mod_reports(state: State<'_, AppState>, mod_id: String, version: String) -> AppResult<VoteSummary> {
    let auth = state.cloud_auth()?;
    let votes = Votes::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    votes.summary(&mod_id, &version).await.map_err(err)
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub mod_id: String,
    pub version: String,
    /// `null` removes your report.
    pub option: Option<VoteOption>,
    pub comment: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn submit_mod_report(state: State<'_, AppState>, request: ReportRequest) -> AppResult<VoteSummary> {
    let auth = state.cloud_auth()?;
    let votes = Votes::new(&auth, state.cloud_config.clone(), &cache_dir(&state)).map_err(err)?;
    match request.option {
        Some(option) => {
            if let Some(word) = request.comment.as_deref().and_then(|c| validator::find_blocked([c])) {
                return Err(AppError::msg(format!("Reports can't contain \"{word}\".")));
            }
            let game = state.environment().game_version.ok_or_else(|| AppError::msg("The game version isn't known yet."))?;
            votes.submit(&request.mod_id, &request.version, &game, option, request.comment.as_deref()).await.map_err(err)
        }
        None => votes.remove(&request.mod_id, &request.version).await.map_err(err),
    }
}

// ---------------------------------------------------------------------------------------------
// "Confirm working mods" after long play sessions
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageMod {
    pub key: String,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub game_version: String,
    pub sessions: u32,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UsagePrompt {
    pub due: bool,
    pub long_sessions: u32,
    pub mods: Vec<UsageMod>,
}

/// Scans the game logs for new long play sessions and says whether to ask about mods.
#[tauri::command]
#[specta::specta]
pub async fn check_play_sessions(state: State<'_, AppState>) -> AppResult<UsagePrompt> {
    let env = state.environment();
    let none = UsagePrompt { due: false, long_sessions: 0, mods: Vec::new() };
    if !env.settings.mod_usage_tracking {
        return Ok(none);
    }
    let Ok((data, _)) = state.mod_roots() else { return Ok(none) };
    let Some(game_version) = env.game_version.clone() else { return Ok(none) };
    let starts = svsm_core::sessions::long_sessions(&data.join("Logs"));
    let installed = state.mods.read_safe().mods.clone();
    let active: Vec<(String, String)> = installed
        .iter()
        .filter(|m| m.is_active && !m.mod_id.trim().is_empty() && !svsm_core::mods::is_base_game_mod(&m.mod_id))
        .filter_map(|m| Some((m.mod_id.clone(), m.version.clone()?)))
        .collect();
    let first = !env.settings.mod_usage_scanned;
    let mut usage = env.settings.mod_usage.clone();
    let new = usage.record(&starts, &active, &game_version, first);
    if new > 0 || first {
        let saved = usage.clone();
        state.update_settings(|s| {
            s.mod_usage = saved;
            s.mod_usage_scanned = true;
        })?;
    }
    let mods = usage
        .mods
        .iter()
        .filter_map(|(key, n)| {
            let mut p = key.splitn(3, '|');
            let (id, version, gv) = (p.next()?, p.next()?, p.next()?);
            let name = installed.iter().find(|m| m.mod_id.eq_ignore_ascii_case(id)).map(|m| m.name.clone()).unwrap_or_else(|| id.to_string());
            Some(UsageMod { key: key.clone(), mod_id: id.into(), name, version: version.into(), game_version: gv.into(), sessions: *n })
        })
        .collect();
    Ok(UsagePrompt { due: usage.prompt_due(), long_sessions: usage.long_sessions, mods })
}

/// Forget mods the user answered or skipped; `stop` turns the prompt off for good.
#[tauri::command]
#[specta::specta]
pub async fn answer_play_sessions(state: State<'_, AppState>, keys: Vec<String>, stop: bool) -> AppResult<()> {
    state.update_settings(|s| {
        s.mod_usage.clear(&keys);
        if stop {
            s.mod_usage_tracking = false;
            s.mod_usage = Default::default();
        }
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Moving an identity off the old cloud project
// ---------------------------------------------------------------------------------------------

/// Moves the cloud identity and its modlists from the old project to the current one. The old
/// identity is kept as `firebase-auth.legacy.json`; on failure everything is put back.
#[tauri::command]
#[specta::specta]
pub async fn move_cloud_identity(state: State<'_, AppState>) -> AppResult<svsm_core::cloud::migrate::MigrationReport> {
    if state.environment().settings.offline_mode {
        return Err(err(svsm_core::cloud::CloudError::Offline));
    }
    let manager = state.manager_dir();
    if !svsm_core::cloud::migrate::is_legacy_identity(&manager) {
        return Err(AppError::msg("Your cloud identity is already on the current project."));
    }
    let me = player(&state)?;
    let _run = state.update_lock.lock().await;
    state.reset_cloud();
    let report = svsm_core::cloud::migrate::migrate(
        &manager,
        svsm_core::cloud::Authenticator::default_backup_file(),
        &me,
        state.legacy_cloud_config.clone(),
        state.cloud_config.clone(),
        &cache_dir(&state),
    )
    .await
    .map_err(err);
    state.reset_cloud();
    report
}
