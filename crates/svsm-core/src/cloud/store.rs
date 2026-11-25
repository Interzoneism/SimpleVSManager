//! Cloud modlists in the Realtime Database (REST). Same paths and payloads as 2.x, plus the
//! revisions and subscriptions from SVSM-Dev 8f81f69:
//!
//! * `owners/{player uid}` = Firebase user id that owns that player's slots,
//! * `users/{player uid}/{slot1..5}` = `{registryId, content, dateAdded, revision, createdAt, updatedAt}`,
//! * `registry/{id}` (full content) and `registrySummaries/{id}` (no configs) are public,
//! * `registryOwners/{id}` = Firebase user id,
//! * `modlistSubscriptions/{id}/{user}` and `modlistSubscriptionNotices/{id}/{user}` = timestamps.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::auth::Authenticator;
use super::{encode_component, http_client, now_iso, parse_iso_ms, sanitize_key, CloudConfig, CloudError, CloudResult};

pub const SLOTS: [&str; 5] = ["slot1", "slot2", "slot3", "slot4", "slot5"];

/// The player identity from clientsettings.json (`playeruid`, `playername`).
#[derive(Debug, Clone)]
pub struct PlayerIdentity {
    pub uid: String,
    pub name: String,
}

impl PlayerIdentity {
    pub fn new(uid: Option<String>, name: Option<String>) -> CloudResult<Self> {
        let uid = uid.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).ok_or_else(|| {
            CloudError::Invalid("clientsettings.json has no player id yet. Start the game once so it creates one.".into())
        })?;
        let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).ok_or_else(|| {
            CloudError::Invalid("clientsettings.json has no player name. Set one in the game before using online modlists.".into())
        })?;
        Ok(Self { uid, name })
    }

    pub fn key(&self) -> String {
        sanitize_key(&self.uid)
    }
}

/// One public modlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub registry_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    /// Card icon (Symbols Nerd Font glyph name) chosen by the author.
    pub icon: Option<String>,
    pub version: Option<String>,
    pub uploader: Option<String>,
    pub game_version: Option<String>,
    pub mods: Vec<RegistryMod>,
    pub revision: f64,
    pub created_ms: Option<f64>,
    pub updated_ms: Option<f64>,
    /// Full content (with configs) or only the summary.
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RegistryMod {
    pub mod_id: String,
    pub version: Option<String>,
}

/// One of the player's own cloud slots.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SlotInfo {
    pub slot: String,
    pub registry_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub revision: f64,
    pub updated_ms: Option<f64>,
    pub mod_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionSnapshot {
    pub counts: HashMap<String, u32>,
    pub mine: HashSet<String>,
}

/// Changes to a published modlist's details. `None` leaves a field as it is; an empty string
/// clears it. The name can't be changed (followers know the list by it).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DetailsEdit {
    pub description: Option<String>,
    pub version: Option<String>,
    pub game_version: Option<String>,
    pub icon: Option<String>,
}

impl DetailsEdit {
    /// The published content with these changes applied.
    pub fn apply(&self, content: &Value) -> Value {
        let mut map = content.as_object().cloned().unwrap_or_default();
        let set = |map: &mut Map<String, Value>, key: &str, value: Option<String>| {
            if let Some(v) = value {
                let v = v.trim().to_string();
                if v.is_empty() {
                    map.remove(key);
                } else {
                    map.insert(key.into(), Value::String(v));
                }
            }
        };
        set(&mut map, "description", self.description.clone());
        set(&mut map, "version", self.version.clone());
        if self.game_version.is_some() {
            map.remove("vsVersion");
            set(&mut map, "gameVersion", self.game_version.clone());
        }
        let icon = self.icon.as_ref().map(|i| crate::modlists::clean_icon(Some(i)).unwrap_or_default());
        set(&mut map, "icon", icon);
        Value::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    pub registry_id: String,
    pub slot: String,
    pub revision: f64,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty()).map(String::from)
}

fn revision(v: &Value) -> f64 {
    v.get("revision").and_then(Value::as_f64).map(|r| r.max(1.0)).unwrap_or(1.0)
}

/// Parses `{content, dateAdded, revision, createdAt, updatedAt}`.
pub fn parse_entry(registry_id: &str, node: &Value, complete: bool) -> Option<RegistryEntry> {
    let content = node.get("content").filter(|c| c.is_object())?;
    let date_added = s(node, "dateAdded").and_then(|d| parse_iso_ms(&d));
    let mods = content
        .get("mods")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| Some(RegistryMod { mod_id: s(m, "modId")?, version: s(m, "version") }))
                .collect()
        })
        .unwrap_or_default();
    Some(RegistryEntry {
        registry_id: registry_id.to_string(),
        name: s(content, "name"),
        description: s(content, "description"),
        icon: crate::modlists::clean_icon(s(content, "icon").as_deref()),
        version: s(content, "version"),
        uploader: s(content, "uploader").or_else(|| s(content, "uploaderName")),
        game_version: s(content, "gameVersion").or_else(|| s(content, "vsVersion")),
        mods,
        revision: revision(node),
        created_ms: s(node, "createdAt").and_then(|d| parse_iso_ms(&d)).or(date_added),
        updated_ms: s(node, "updatedAt").and_then(|d| parse_iso_ms(&d)).or(date_added),
        complete,
    })
}

/// The registry summary 2.x wrote next to the full entry (no configs, small).
pub fn summary_json(content: &Value, date_added: &str, revision: f64, created: &str, updated: &str) -> Value {
    let mods: Vec<Value> = content
        .get("mods")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|m| Some(json!({ "modId": s(m, "modId")?, "version": s(m, "version") }))).collect())
        .unwrap_or_default();
    let game = s(content, "gameVersion").or_else(|| s(content, "vsVersion"));
    let uploader = s(content, "uploader").or_else(|| s(content, "uploaderName"));
    json!({
        "content": {
            "name": s(content, "name"), "description": s(content, "description"), "icon": s(content, "icon"), "version": s(content, "version"),
            "uploader": uploader, "uploaderName": uploader, "gameVersion": game, "vsVersion": game, "mods": mods,
        },
        "dateAdded": date_added, "revision": revision, "createdAt": created, "updatedAt": updated,
    })
}

/// Sets uploader fields the way the database rules expect (`uploaderId` = sanitized player id).
pub fn with_uploader(content: &Value, identity: &PlayerIdentity) -> Value {
    let mut map = content.as_object().cloned().unwrap_or_default();
    // Local subscription bookkeeping never goes public.
    map.remove("onlineSubscription");
    map.insert("uploader".into(), Value::String(identity.name.clone()));
    map.insert("uploaderName".into(), Value::String(identity.name.clone()));
    map.insert("uploaderId".into(), Value::String(identity.key()));
    Value::Object(map)
}

#[derive(Serialize, Deserialize, Default)]
struct CacheFile {
    etag: Option<String>,
    entries: Vec<RegistryEntry>,
}

pub struct CloudStore<'a> {
    auth: &'a Authenticator,
    config: CloudConfig,
    http: reqwest::Client,
    cache_dir: PathBuf,
}

struct Reply {
    status: reqwest::StatusCode,
    body: String,
    etag: Option<String>,
    user_id: String,
}

impl<'a> CloudStore<'a> {
    /// `cache_dir`: where registry listings are cached (`Temp Cache/Firebase Cache`).
    pub fn new(auth: &'a Authenticator, config: CloudConfig, cache_dir: &Path) -> CloudResult<Self> {
        Ok(Self { auth, config, http: http_client()?, cache_dir: cache_dir.to_path_buf() })
    }

    fn url(&self, path: &[&str], token: Option<&str>, query: Option<&str>) -> String {
        let mut u = self.config.db_url.trim_end_matches('/').to_string();
        for p in path {
            u.push('/');
            u.push_str(&encode_component(p));
        }
        if path.is_empty() {
            // The database root is `<db>/.json`; without the slash ".json" ends up on the host.
            u.push('/');
        }
        u.push_str(".json");
        let mut params: Vec<String> = Vec::new();
        if let Some(t) = token {
            params.push(format!("auth={}", encode_component(t)));
        }
        if let Some(q) = query {
            params.push(q.to_string());
        }
        if !params.is_empty() {
            u.push('?');
            u.push_str(&params.join("&"));
        }
        u
    }

    /// GET of a publicly readable path: no sign-in needed (works with any identity, or none).
    async fn get_public(&self, path: &[&str], etag: Option<&str>) -> CloudResult<Reply> {
        let mut req = self.http.get(self.url(path, None, None)).header("X-Firebase-ETag", "true");
        if let Some(e) = etag {
            req = req.header("If-None-Match", e);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let etag = resp.headers().get("ETag").and_then(|v| v.to_str().ok()).map(String::from);
        let body = resp.text().await.unwrap_or_default();
        Ok(Reply { status, body, etag, user_id: self.auth.user_id().unwrap_or_default() })
    }

    /// Sends with the current token; on 401/403 refreshes once and retries.
    async fn send(&self, method: reqwest::Method, path: &[&str], query: Option<&str>, body: Option<String>, etag: Option<&str>) -> CloudResult<Reply> {
        let mut retried = false;
        loop {
            let session = self.auth.session(retried).await?;
            let mut req = self.http.request(method.clone(), self.url(path, Some(&session.id_token), query));
            if let Some(b) = &body {
                req = req.header("Content-Type", "application/json").body(b.clone());
            }
            if method == reqwest::Method::GET {
                req = req.header("X-Firebase-ETag", "true");
                if let Some(e) = etag {
                    req = req.header("If-None-Match", e);
                }
            }
            let resp = req.send().await?;
            let status = resp.status();
            if matches!(status.as_u16(), 401 | 403) && !retried {
                self.auth.invalidate();
                retried = true;
                continue;
            }
            let etag = resp.headers().get("ETag").and_then(|v| v.to_str().ok()).map(String::from);
            let body = resp.text().await.unwrap_or_default();
            return Ok(Reply { status, body, etag, user_id: session.user_id });
        }
    }

    fn ok(r: &Reply, what: &str) -> CloudResult<()> {
        match r.status.as_u16() {
            200..=299 => Ok(()),
            401 | 403 => Err(CloudError::Denied(what.to_string())),
            code => Err(CloudError::BadResponse(format!("{what}: HTTP {code} {}", r.body.chars().take(200).collect::<String>()))),
        }
    }

    fn json(r: &Reply) -> CloudResult<Value> {
        if r.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&r.body).map_err(|e| CloudError::BadResponse(e.to_string()))
    }

    // --- Public registry -------------------------------------------------------------------

    fn cache_path(&self, name: &str) -> PathBuf {
        self.cache_dir.join(format!("svsm3-{name}.json"))
    }

    fn read_cache(&self, name: &str) -> Option<CacheFile> {
        serde_json::from_str(&std::fs::read_to_string(self.cache_path(name)).ok()?).ok()
    }

    fn write_cache(&self, name: &str, cache: &CacheFile) {
        let _ = std::fs::create_dir_all(&self.cache_dir);
        let _ = std::fs::write(self.cache_path(name), serde_json::to_string(cache).unwrap_or_default());
    }

    /// Cached listing without network (for offline mode).
    pub fn cached_registry(&self) -> Vec<RegistryEntry> {
        self.read_cache("registry-summaries").map(|c| c.entries).unwrap_or_default()
    }

    /// Every public modlist (summaries; falls back to the full registry on old databases).
    pub async fn registry(&self) -> CloudResult<Vec<RegistryEntry>> {
        let entries = self.fetch_listing("registrySummaries", "registry-summaries", false).await?;
        if !entries.is_empty() {
            return Ok(entries);
        }
        self.fetch_listing("registry", "registry-full", true).await
    }

    async fn fetch_listing(&self, path: &str, cache_name: &str, complete: bool) -> CloudResult<Vec<RegistryEntry>> {
        let cached = self.read_cache(cache_name);
        let r = self.get_public(&[path], cached.as_ref().and_then(|c| c.etag.as_deref())).await?;
        if r.status == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(c) = cached {
                return Ok(c.entries);
            }
        }
        if r.status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        Self::ok(&r, "Reading online modlists")?;
        let v = Self::json(&r)?;
        let mut entries: Vec<RegistryEntry> = v
            .as_object()
            .map(|m| m.iter().filter_map(|(id, node)| parse_entry(id, node, complete)).collect())
            .unwrap_or_default();
        entries.sort_by(|a, b| b.updated_ms.partial_cmp(&a.updated_ms).unwrap_or(std::cmp::Ordering::Equal));
        self.write_cache(cache_name, &CacheFile { etag: r.etag.clone(), entries: entries.clone() });
        Ok(entries)
    }

    /// One public modlist with its full content (configs included), as modlist JSON.
    pub async fn registry_content(&self, registry_id: &str) -> CloudResult<Option<(RegistryEntry, Value)>> {
        let r = self.get_public(&["registry", registry_id], None).await?;
        if r.status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::ok(&r, "Reading the online modlist")?;
        let v = Self::json(&r)?;
        Ok(parse_entry(registry_id, &v, true).map(|e| (e, v.get("content").cloned().unwrap_or(Value::Null))))
    }

    // --- Subscriptions -------------------------------------------------------------------------

    pub async fn subscriptions(&self) -> CloudResult<SubscriptionSnapshot> {
        let r = self.get_public(&["modlistSubscriptions"], None).await?;
        let mut snap = SubscriptionSnapshot::default();
        if matches!(r.status.as_u16(), 401 | 403 | 404) {
            // Rules not deployed yet, or nothing there.
            return Ok(snap);
        }
        Self::ok(&r, "Reading subscriptions")?;
        if let Some(map) = Self::json(&r)?.as_object() {
            for (id, subs) in map {
                let Some(subs) = subs.as_object() else { continue };
                let live: Vec<&String> = subs.iter().filter(|(_, v)| !v.is_null()).map(|(k, _)| k).collect();
                if live.iter().any(|k| **k == r.user_id) {
                    snap.mine.insert(id.clone());
                }
                snap.counts.insert(id.clone(), live.len() as u32);
            }
        }
        Ok(snap)
    }

    /// Returns false when already subscribed.
    pub async fn subscribe(&self, registry_id: &str) -> CloudResult<bool> {
        let user = self.auth.session(false).await?.user_id;
        let existing = self.send(reqwest::Method::GET, &["modlistSubscriptions", registry_id, &user], None, None, None).await?;
        if existing.status.is_success() && !Self::json(&existing).unwrap_or(Value::Null).is_null() {
            return Ok(false);
        }
        let stamp = json!({ ".sv": "timestamp" });
        let patch = json!({
            format!("/modlistSubscriptions/{registry_id}/{user}"): stamp,
            format!("/modlistSubscriptionNotices/{registry_id}/{user}"): stamp,
        });
        let r = self.send(reqwest::Method::PATCH, &[], None, Some(patch.to_string()), None).await?;
        Self::ok(&r, "Subscribing")?;
        Ok(true)
    }

    pub async fn unsubscribe(&self, registry_id: &str) -> CloudResult<()> {
        let user = self.auth.session(false).await?.user_id;
        let r = self.send(reqwest::Method::DELETE, &["modlistSubscriptions", registry_id, &user], None, None, None).await?;
        if r.status != reqwest::StatusCode::NOT_FOUND {
            Self::ok(&r, "Unsubscribing")?;
        }
        Ok(())
    }

    /// New subscribers per owned modlist since the owner last looked.
    pub async fn owner_notices(&self, registry_ids: &[String]) -> CloudResult<BTreeMap<String, u32>> {
        let mut out = BTreeMap::new();
        for id in registry_ids {
            let r = self.send(reqwest::Method::GET, &["modlistSubscriptionNotices", id], None, None, None).await?;
            if !r.status.is_success() {
                continue;
            }
            let n = Self::json(&r)?.as_object().map(|m| m.values().filter(|v| !v.is_null()).count()).unwrap_or(0);
            if n > 0 {
                out.insert(id.clone(), n as u32);
            }
        }
        Ok(out)
    }

    pub async fn acknowledge_notices(&self, registry_ids: &[String]) -> CloudResult<()> {
        if registry_ids.is_empty() {
            return Ok(());
        }
        let patch: Map<String, Value> = registry_ids.iter().map(|id| (format!("/modlistSubscriptionNotices/{id}"), Value::Null)).collect();
        let r = self.send(reqwest::Method::PATCH, &[], None, Some(Value::Object(patch).to_string()), None).await?;
        Self::ok(&r, "Clearing subscriber notices")
    }

    // --- The player's own slots ----------------------------------------------------------------

    async fn ensure_ownership(&self, identity: &PlayerIdentity) -> CloudResult<()> {
        let key = identity.key();
        let r = self.send(reqwest::Method::GET, &["owners", &key], None, None, None).await?;
        let owner = if r.status.is_success() { Self::json(&r)?.as_str().map(String::from) } else { None };
        match owner {
            Some(o) if o == r.user_id => Ok(()),
            Some(_) => Err(CloudError::Invalid(
                "Online modlists for this player are owned by another installation (a different firebase-auth.json).".into(),
            )),
            None => {
                let claim = self.send(reqwest::Method::PUT, &["owners", &key], None, Some(Value::String(r.user_id.clone()).to_string()), None).await?;
                if matches!(claim.status.as_u16(), 401 | 403) {
                    return Err(CloudError::Invalid(
                        "Online modlists for this player are owned by another installation (a different firebase-auth.json).".into(),
                    ));
                }
                Self::ok(&claim, "Claiming the player's cloud slots")
            }
        }
    }

    async fn read_slots(&self, identity: &PlayerIdentity) -> CloudResult<Map<String, Value>> {
        self.ensure_ownership(identity).await?;
        let r = self.send(reqwest::Method::GET, &["users", &identity.key()], None, None, None).await?;
        if r.status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Map::new());
        }
        Self::ok(&r, "Reading your online modlists")?;
        Ok(Self::json(&r)?.as_object().cloned().unwrap_or_default())
    }

    /// Slot contents without claiming anything (for moving off the old project). Empty when
    /// this account doesn't own the player's slots.
    pub async fn slot_contents(&self, identity: &PlayerIdentity) -> CloudResult<Vec<(String, Value)>> {
        let key = identity.key();
        let owner = self.send(reqwest::Method::GET, &["owners", &key], None, None, None).await?;
        let owned = owner.status.is_success() && Self::json(&owner)?.as_str() == Some(owner.user_id.as_str());
        if !owned {
            return Ok(Vec::new());
        }
        let r = self.send(reqwest::Method::GET, &["users", &key], None, None, None).await?;
        if r.status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        Self::ok(&r, "Reading your old online modlists")?;
        let all = Self::json(&r)?;
        Ok(SLOTS
            .iter()
            .filter_map(|slot| Some((slot.to_string(), all.get(*slot)?.get("content").filter(|c| c.is_object())?.clone())))
            .collect())
    }

    pub async fn my_slots(&self, identity: &PlayerIdentity) -> CloudResult<Vec<SlotInfo>> {
        let slots = self.read_slots(identity).await?;
        Ok(SLOTS
            .iter()
            .filter_map(|slot| {
                let node = slots.get(*slot)?;
                let content = node.get("content")?;
                Some(SlotInfo {
                    slot: slot.to_string(),
                    registry_id: s(node, "registryId"),
                    name: s(content, "name"),
                    version: s(content, "version"),
                    revision: revision(node),
                    updated_ms: s(node, "updatedAt").or_else(|| s(node, "dateAdded")).and_then(|d| parse_iso_ms(&d)),
                    mod_count: content.get("mods").and_then(Value::as_array).map(|a| a.len() as u32).unwrap_or(0),
                })
            })
            .collect())
    }

    /// Publishes `content` (modlist JSON) to a slot. `expected_registry_id` = updating that
    /// modlist (fails if the slot changed meanwhile); `None` = the slot must be empty.
    pub async fn publish(&self, identity: &PlayerIdentity, slot: &str, content: &Value, expected_registry_id: Option<&str>) -> CloudResult<WriteResult> {
        if !SLOTS.contains(&slot) {
            return Err(CloudError::Invalid(format!("Unknown slot {slot}.")));
        }
        if !content.is_object() {
            return Err(CloudError::Invalid("A modlist must be a JSON object.".into()));
        }
        let slots = self.read_slots(identity).await?;
        let existing = slots.get(slot);
        let existing_id = existing.and_then(|n| s(n, "registryId"));
        match (expected_registry_id, &existing_id, existing) {
            (None, _, Some(_)) => return Err(CloudError::Invalid(format!("Cloud {slot} is already in use."))),
            (Some(want), have, _) if have.as_deref() != Some(want) => {
                return Err(CloudError::Invalid("That online modlist changed meanwhile. Refresh and try again.".into()));
            }
            _ => {}
        }
        let registry_id = existing_id.unwrap_or_else(new_id);
        let now = now_iso();
        let created = existing.and_then(|n| s(n, "createdAt").or_else(|| s(n, "dateAdded"))).unwrap_or_else(|| now.clone());
        let rev = existing.map(|n| revision(n) + 1.0).unwrap_or(1.0);
        let content = with_uploader(content, identity);
        let session_user = self.auth.session(false).await?.user_id;
        let key = identity.key();
        let patch = json!({
            format!("/users/{key}/{slot}"): { "registryId": registry_id, "content": content, "dateAdded": now, "revision": rev, "createdAt": created, "updatedAt": now },
            format!("/registryOwners/{registry_id}"): session_user,
            format!("/registry/{registry_id}"): { "content": content, "dateAdded": now, "revision": rev, "createdAt": created, "updatedAt": now },
            format!("/registrySummaries/{registry_id}"): summary_json(&content, &now, rev, &created, &now),
        });
        let r = self.send(reqwest::Method::PATCH, &[], None, Some(patch.to_string()), None).await?;
        Self::ok(&r, "Publishing the modlist")?;
        Ok(WriteResult { registry_id, slot: slot.to_string(), revision: rev })
    }

    /// Changes the details of one of the player's published modlists (a new revision with the
    /// same mods and configs). Fails if it isn't in one of the player's slots.
    pub async fn edit_details(&self, identity: &PlayerIdentity, registry_id: &str, edit: &DetailsEdit) -> CloudResult<WriteResult> {
        let slots = self.read_slots(identity).await?;
        let (slot, node) = SLOTS
            .iter()
            .find_map(|slot| slots.get(*slot).filter(|n| s(n, "registryId").as_deref() == Some(registry_id)).map(|n| (*slot, n)))
            .ok_or_else(|| CloudError::Invalid("That modlist isn't one of yours.".into()))?;
        let content = node.get("content").filter(|c| c.is_object()).ok_or_else(|| CloudError::Invalid("That online modlist is empty.".into()))?;
        self.publish(identity, slot, &edit.apply(content), Some(registry_id)).await
    }

    /// Removes a slot and its public copy (subscribers keep their local copies).
    pub async fn delete_slot(&self, identity: &PlayerIdentity, slot: &str) -> CloudResult<()> {
        if !SLOTS.contains(&slot) {
            return Err(CloudError::Invalid(format!("Unknown slot {slot}.")));
        }
        let slots = self.read_slots(identity).await?;
        let key = identity.key();
        let mut patch = Map::new();
        patch.insert(format!("/users/{key}/{slot}"), Value::Null);
        if let Some(id) = slots.get(slot).and_then(|n| s(n, "registryId")) {
            for p in ["registry", "registryOwners", "registrySummaries", "modlistSubscriptions", "modlistSubscriptionNotices"] {
                patch.insert(format!("/{p}/{id}"), Value::Null);
            }
        }
        let r = self.send(reqwest::Method::PATCH, &[], None, Some(Value::Object(patch).to_string()), None).await?;
        if r.status != reqwest::StatusCode::NOT_FOUND {
            Self::ok(&r, "Deleting the online modlist")?;
        }
        Ok(())
    }
}

fn new_id() -> String {
    // 32 hex characters, like 2.x's `Guid.ToString("n")`.
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::auth::IDENTITY_FILE;
    use mockito::Matcher;

    fn setup() -> (mockito::ServerGuard, tempfile::TempDir, Authenticator) {
        let server = mockito::Server::new();
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join(IDENTITY_FILE),
            r#"{"idToken":"tok","refreshToken":"r","expirationUtc":"2999-01-01T00:00:00Z","userId":"fb1"}"#,
        )
        .unwrap();
        let auth = Authenticator::new(t.path(), None, CloudConfig::for_tests(&server.url())).unwrap();
        (server, t, auth)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    fn me() -> PlayerIdentity {
        PlayerIdentity::new(Some("abc.def/1".into()), Some("Martin".into())).unwrap()
    }

    #[test]
    fn root_url_keeps_a_slash_before_json() {
        let (server, t, auth) = setup();
        let mut cfg = CloudConfig::for_tests(&server.url());
        cfg.db_url = "https://example-rtdb.firebasedatabase.app/".into();
        let store = CloudStore::new(&auth, cfg, &t.path().join("cache")).unwrap();
        assert_eq!(store.url(&[], Some("t"), None), "https://example-rtdb.firebasedatabase.app/.json?auth=t");
        assert_eq!(store.url(&["a", "b"], None, None), "https://example-rtdb.firebasedatabase.app/a/b.json");
    }

    #[test]
    fn network_errors_do_not_include_the_token() {
        let rt = rt();
        let err = rt
            .block_on(reqwest::Client::new().get("http://nonexistent.invalid/x.json?auth=SECRET").send())
            .unwrap_err();
        let msg = CloudError::from(err).to_string();
        assert!(!msg.contains("SECRET"), "{msg}");
        assert!(msg.contains("nonexistent.invalid/x.json"), "{msg}");
    }

    #[test]
    fn registry_listing_uses_etag_cache() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), &t.path().join("cache")).unwrap();
        let first = server
            .mock("GET", "/db/registrySummaries.json")
            .match_query(Matcher::Missing)
            .with_header("ETag", "e1")
            .with_body(r#"{"new":{"content":{"name":"New","mods":[{"modId":"a","version":"1"}]},"dateAdded":"2026-08-10T10:00:00Z","createdAt":"2026-08-09T10:00:00Z","updatedAt":"2026-08-10T10:00:00Z","revision":4},
                         "legacy":{"content":{"name":"Legacy","uploaderName":"Bob","vsVersion":"1.20"},"dateAdded":"2025-01-02T03:04:05Z"}}"#)
            .expect(1)
            .create();
        let rt = rt();
        let list = rt.block_on(store.registry()).unwrap();
        first.assert();
        assert_eq!(list[0].registry_id, "new");
        assert_eq!(list[0].revision, 4.0);
        assert_eq!(list[1].revision, 1.0);
        assert_eq!(list[1].created_ms, list[1].updated_ms);
        assert_eq!((list[1].uploader.as_deref(), list[1].game_version.as_deref()), (Some("Bob"), Some("1.20")));

        let second = server
            .mock("GET", "/db/registrySummaries.json")
            .match_query(Matcher::Any)
            .match_header("If-None-Match", "e1")
            .with_status(304)
            .expect(1)
            .create();
        assert_eq!(rt.block_on(store.registry()).unwrap().len(), 2);
        second.assert();
        assert_eq!(store.cached_registry().len(), 2);
    }

    #[test]
    fn publish_writes_all_locations_with_revisions() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), t.path()).unwrap();
        server.mock("GET", "/db/owners/abc_def_1.json").match_query(Matcher::Any).with_body("\"fb1\"").create();
        server
            .mock("GET", "/db/users/abc_def_1.json")
            .match_query(Matcher::Any)
            .with_body(r#"{"slot2":{"registryId":"rid","content":{"name":"Old"},"revision":2,"createdAt":"2026-01-01T00:00:00Z"}}"#)
            .create();
        let patch = server
            .mock("PATCH", "/db/.json")
            .match_query(Matcher::Any)
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex(r#""/users/abc_def_1/slot2":\{"#.into()),
                Matcher::Regex(r#""/registryOwners/rid":"fb1""#.into()),
                Matcher::Regex(r#""revision":3\.0"#.into()),
                Matcher::Regex(r#""uploaderId":"abc_def_1""#.into()),
                Matcher::Regex(r#""createdAt":"2026-01-01T00:00:00Z""#.into()),
            ]))
            .with_body("{}")
            .expect(1)
            .create();
        let rt = rt();
        let content = json!({"name": "Mine", "mods": [{"modId": "x"}], "onlineSubscription": {"registryId": "zzz"}});
        let r = rt.block_on(store.publish(&me(), "slot2", &content, Some("rid"))).unwrap();
        assert_eq!((r.registry_id.as_str(), r.revision), ("rid", 3.0));
        patch.assert();
        // Wrong expectations are refused before writing.
        assert!(rt.block_on(store.publish(&me(), "slot2", &content, None)).is_err());
        assert!(rt.block_on(store.publish(&me(), "slot2", &content, Some("other"))).is_err());
        assert!(rt.block_on(store.publish(&me(), "slot9", &content, None)).is_err());
        assert!(!with_uploader(&content, &me()).as_object().unwrap().contains_key("onlineSubscription"));
    }

    #[test]
    fn details_edits_keep_name_and_mods() {
        let content = json!({"name": "Mine", "description": "old", "vsVersion": "1.20", "version": "1", "mods": [{"modId": "x"}]});
        let edit = DetailsEdit { description: Some(" new ".into()), version: Some("".into()), game_version: Some("1.21".into()), icon: Some("md-sword".into()) };
        let out = edit.apply(&content);
        assert_eq!(out, json!({"name": "Mine", "description": "new", "gameVersion": "1.21", "icon": "md-sword", "mods": [{"modId": "x"}]}));
        let untouched = DetailsEdit::default().apply(&content);
        assert_eq!(untouched, content);
        let bad_icon = DetailsEdit { icon: Some("<x>".into()), ..Default::default() }.apply(&out);
        assert!(bad_icon.get("icon").is_none());
    }

    #[test]
    fn editing_details_publishes_a_new_revision() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), t.path()).unwrap();
        server.mock("GET", "/db/owners/abc_def_1.json").match_query(Matcher::Any).with_body("\"fb1\"").create();
        server
            .mock("GET", "/db/users/abc_def_1.json")
            .match_query(Matcher::Any)
            .with_body(r#"{"slot3":{"registryId":"rid","content":{"name":"Mine","description":"old","mods":[{"modId":"x"}]},"revision":4}}"#)
            .create();
        let patch = server
            .mock("PATCH", "/db/.json")
            .match_query(Matcher::Any)
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex(r#""/users/abc_def_1/slot3":\{"#.into()),
                Matcher::Regex(r#""description":"new""#.into()),
                Matcher::Regex(r#""name":"Mine""#.into()),
                Matcher::Regex(r#""revision":5\.0"#.into()),
            ]))
            .with_body("{}")
            .expect(1)
            .create();
        let rt = rt();
        let edit = DetailsEdit { description: Some("new".into()), ..Default::default() };
        assert_eq!(rt.block_on(store.edit_details(&me(), "rid", &edit)).unwrap().revision, 5.0);
        patch.assert();
        assert!(rt.block_on(store.edit_details(&me(), "someone-else", &edit)).is_err());
    }

    #[test]
    fn someone_elses_slots_are_refused() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), t.path()).unwrap();
        server.mock("GET", "/db/owners/abc_def_1.json").match_query(Matcher::Any).with_body("\"someone\"").create();
        let err = rt().block_on(store.my_slots(&me())).unwrap_err();
        assert!(err.to_string().contains("another installation"));
    }

    #[test]
    fn subscriptions_are_read_without_signing_in() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), t.path()).unwrap();
        server
            .mock("GET", "/db/modlistSubscriptions.json")
            .match_query(Matcher::Missing)
            .with_body(r#"{"r1":{"fb1":1,"x":2},"r2":{"y":3,"z":null}}"#)
            .create();
        let snap = rt().block_on(store.subscriptions()).unwrap();
        assert_eq!(snap.counts["r1"], 2);
        assert_eq!(snap.counts["r2"], 1);
        assert!(snap.mine.contains("r1") && !snap.mine.contains("r2"));
    }

    #[test]
    fn retries_once_with_a_refreshed_token() {
        let (mut server, t, auth) = setup();
        let store = CloudStore::new(&auth, CloudConfig::for_tests(&server.url()), t.path()).unwrap();
        server.mock("GET", "/db/owners/abc_def_1.json").match_query(Matcher::UrlEncoded("auth".into(), "tok".into())).with_status(401).create();
        server.mock("POST", "/token").match_query(Matcher::Any).with_body(r#"{"id_token":"tok2","expires_in":"3600","user_id":"fb1"}"#).create();
        server.mock("GET", "/db/owners/abc_def_1.json").match_query(Matcher::UrlEncoded("auth".into(), "tok2".into())).with_body("\"fb1\"").create();
        server.mock("GET", "/db/users/abc_def_1.json").match_query(Matcher::Any).with_body(r#"{"slot1":{"registryId":"r","content":{"name":"A","mods":[1,2]}}}"#).create();
        let slots = rt().block_on(store.my_slots(&me())).unwrap();
        assert_eq!((slots[0].slot.as_str(), slots[0].mod_count), ("slot1", 2));
    }

    #[test]
    fn icons_are_published_in_the_summary() {
        let content = json!({"name": "N", "icon": "md-sword", "mods": []});
        let summary = summary_json(&content, "d", 1.0, "c", "u");
        assert_eq!(summary["content"]["icon"], "md-sword");
        assert_eq!(parse_entry("r", &summary, false).unwrap().icon.as_deref(), Some("md-sword"));
        let bad = json!({"content": {"icon": "<script>"}});
        assert_eq!(parse_entry("r", &bad, false).unwrap().icon, None);
    }

    #[test]
    fn ids_look_like_2x_ids() {
        let a = new_id();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
