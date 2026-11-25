//! Mod DB (mods.vintagestory.at): lookups for installed mods, updates, and the mod browser.
//! Port of `ModApiService`, `ModDatabaseService`, `ModDatabaseCacheService`,
//! `ModUpdateService` and `ModCacheLocator`.

pub mod browse;
pub mod cache;
pub mod changelog;
pub mod client;
pub mod dates;
pub mod install;
mod json;
pub mod model;
pub mod parse;
pub mod updates;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use futures_util::stream::{self, StreamExt};

pub use browse::{browse, BrowseEntry, BrowsePage, BrowseQuery, BrowseSort};
pub use cache::{CachedMod, ModDbCache};
pub use client::{DownloadProgress, ModDbClient, NetError, NetResult};
pub use install::{install_archive, CachedMods, CachedVersion, InstallPlan, InstallTarget};
pub use model::{ModDbInfo, ModDbListEntry, ModDbRelease, ModDbScreenshot, ModDbSummary, NamedTag, UpdateOffer};

/// At most this many Mod DB requests run at once (same as 2.x).
pub const MAX_CONCURRENT_REQUESTS: usize = 4;
/// Cached mod data younger than this is used without asking the Mod DB again.
pub const MOD_MAX_AGE_MS: f64 = 30.0 * 60_000.0;
/// The full catalogue is ~4 MB; refresh it at most every few hours unless asked.
pub const LIST_MAX_AGE_MS: f64 = 3.0 * 3_600_000.0;
pub const TAGS_MAX_AGE_MS: f64 = 24.0 * 3_600_000.0;
/// Card thumbnails rarely change; look them up again after a week.
pub const THUMB_MAX_AGE_MS: f64 = 7.0 * 24.0 * 3_600_000.0;

/// The manager's own Mod DB page (for the "new version of the manager" notice).
pub const MANAGER_MOD_ID: &str = "5545";

/// Result of looking up several mods.
#[derive(Debug, Default)]
pub struct Lookup {
    /// By lower-case mod id. Missing = never looked up and nothing cached.
    pub mods: HashMap<String, CachedMod>,
    /// Mods that were downloaded just now.
    pub fetched: u32,
    /// The first network error, if any request failed (cached data is used for those).
    pub error: Option<String>,
}

pub struct ModDb {
    client: ModDbClient,
    cache: ModDbCache,
    cached_mods: CachedMods,
    temp_dir: PathBuf,
    offline: AtomicBool,
    counter: AtomicU64,
    /// Card thumbnails by lower-case mod id, loaded from disk on first use.
    thumbs: Mutex<Option<HashMap<String, Thumb>>>,
}

/// The Mod DB's card thumbnail (`logofiledb`) for one mod. The catalogue only has `logo`,
/// which is often a different (older or cropped) image than the one the Mod DB shows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Thumb {
    url: Option<String>,
    at: f64,
}

const THUMBS_CACHE: &str = "thumbnails";

impl ModDb {
    pub fn new(manager_dir: &Path, offline: bool) -> NetResult<Self> {
        Ok(Self::with_client(ModDbClient::new()?, manager_dir, offline))
    }

    pub fn with_client(client: ModDbClient, manager_dir: &Path, offline: bool) -> Self {
        Self {
            client,
            cache: ModDbCache::new(manager_dir),
            cached_mods: CachedMods::new(manager_dir),
            temp_dir: manager_dir.join("Temp Cache").join("svsm3 Downloads"),
            offline: AtomicBool::new(offline),
            counter: AtomicU64::new(0),
            thumbs: Mutex::new(None),
        }
    }

    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::Relaxed);
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Relaxed)
    }

    pub fn cache(&self) -> &ModDbCache {
        &self.cache
    }

    pub fn cached_mods(&self) -> &CachedMods {
        &self.cached_mods
    }

    /// Cached data only; never touches the network.
    pub fn cached(&self, ids: &[String]) -> HashMap<String, CachedMod> {
        ids.iter()
            .filter_map(|id| Some((id.to_ascii_lowercase(), self.cache.load_mod(id)?)))
            .collect()
    }

    /// Looks up mods, using cached data younger than [`MOD_MAX_AGE_MS`] unless `force`.
    pub async fn lookup(&self, ids: &[String], force: bool) -> Lookup {
        let now = dates::now_ms();
        let mut out = Lookup { mods: self.cached(ids), ..Default::default() };
        if self.is_offline() {
            return out;
        }
        let mut seen = std::collections::HashSet::new();
        let stale: Vec<String> = ids
            .iter()
            .filter(|id| seen.insert(id.to_ascii_lowercase()))
            .filter(|id| {
                force
                    || out
                        .mods
                        .get(&id.to_ascii_lowercase())
                        .is_none_or(|c| now - c.fetched_ms > MOD_MAX_AGE_MS || c.fetched_ms > now)
            })
            .cloned()
            .collect();

        let results: Vec<(String, NetResult<Option<serde_json::Value>>)> = stream::iter(stale)
            .map(|id| async move {
                let r = self.client.fetch_mod(&id).await;
                (id, r)
            })
            .buffer_unordered(MAX_CONCURRENT_REQUESTS)
            .collect()
            .await;

        let fetched_ms = dates::now_ms();
        for (id, r) in results {
            match r {
                Ok(raw) => {
                    let _ = self.cache.store_mod(&id, raw.as_ref(), fetched_ms);
                    out.mods.insert(id.to_ascii_lowercase(), CachedMod { raw, fetched_ms });
                    out.fetched += 1;
                }
                Err(e) => {
                    out.error.get_or_insert_with(|| e.to_string());
                }
            }
        }
        out
    }

    /// One mod, refreshed if stale (or `force`). Falls back to the cache on network errors.
    pub async fn get_mod(&self, id: &str, force: bool) -> (Option<CachedMod>, Option<String>) {
        let mut l = self.lookup(&[id.to_string()], force).await;
        (l.mods.remove(&id.to_ascii_lowercase()), l.error)
    }

    /// The Mod DB card thumbnail (`logofiledb`) for each mod id, as the Mod DB's own mod list
    /// shows it. Uses (in order) the thumbnail cache, cached mod data, then `/api/mod/<id>`
    /// (which also warms the details cache). Mods that couldn't be looked up are left out, so
    /// the caller keeps the catalogue's `logo`; `None` = the mod has no thumbnail.
    pub async fn thumbnails(&self, ids: &[String]) -> HashMap<String, Option<String>> {
        let now = dates::now_ms();
        let mut out = HashMap::new();
        let mut missing = Vec::new();
        let mut changed = false;
        {
            let mut guard = self.thumbs.lock().expect("thumbs lock poisoned");
            let map = guard.get_or_insert_with(|| self.cache.load(THUMBS_CACHE).map(|(m, _)| m).unwrap_or_default());
            for id in ids {
                let key = id.to_ascii_lowercase();
                if out.contains_key(&key) {
                    continue;
                }
                match map.get(&key) {
                    Some(t) if now - t.at <= THUMB_MAX_AGE_MS && t.at <= now => {
                        out.insert(key, t.url.clone());
                    }
                    _ => {
                        // Cached mod data (any age) is good enough for a picture.
                        if let Some(raw) = self.cache.load_mod(&key).and_then(|c| c.raw) {
                            let url = parse::db_logo(&raw);
                            map.insert(key.clone(), Thumb { url: url.clone(), at: now });
                            out.insert(key, url);
                            changed = true;
                        } else {
                            out.insert(key.clone(), None);
                            missing.push(key);
                        }
                    }
                }
            }
        }
        for k in &missing {
            out.remove(k);
        }
        if !missing.is_empty() && !self.is_offline() {
            let results: Vec<(String, NetResult<Option<serde_json::Value>>)> = stream::iter(missing)
                .map(|id| async move {
                    let r = self.client.fetch_mod(&id).await;
                    (id, r)
                })
                .buffer_unordered(MAX_CONCURRENT_REQUESTS)
                .collect()
                .await;
            let fetched_ms = dates::now_ms();
            let mut guard = self.thumbs.lock().expect("thumbs lock poisoned");
            let map = guard.get_or_insert_with(HashMap::new);
            for (id, r) in results {
                if let Ok(raw) = r {
                    let _ = self.cache.store_mod(&id, raw.as_ref(), fetched_ms);
                    let url = raw.as_ref().and_then(parse::db_logo);
                    map.insert(id.clone(), Thumb { url: url.clone(), at: fetched_ms });
                    out.insert(id, url);
                    changed = true;
                }
            }
        }
        if changed {
            let guard = self.thumbs.lock().expect("thumbs lock poisoned");
            if let Some(map) = guard.as_ref() {
                let _ = self.cache.store(THUMBS_CACHE, map, now);
            }
        }
        out
    }

    async fn cached_or_fetch<T, F, Fut>(&self, name: &str, max_age: f64, force: bool, fetch: F) -> NetResult<(T, f64)>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = NetResult<T>>,
    {
        let cached: Option<(T, f64)> = self.cache.load(name);
        let now = dates::now_ms();
        let fresh = cached.as_ref().is_some_and(|(_, at)| now - at <= max_age && *at <= now);
        if self.is_offline() || (fresh && !force) {
            return cached.ok_or(NetError::Offline);
        }
        match fetch().await {
            Ok(data) => {
                let _ = self.cache.store(name, &data, now);
                Ok((data, now))
            }
            Err(e) => cached.ok_or(e),
        }
    }

    /// The whole catalogue (every mod on the Mod DB), cached for [`LIST_MAX_AGE_MS`].
    pub async fn catalogue(&self, force: bool) -> NetResult<(Vec<ModDbListEntry>, f64)> {
        self.cached_or_fetch("catalogue", LIST_MAX_AGE_MS, force, || async {
            let raw = self.client.fetch_mod_list(None).await?;
            Ok(raw.iter().filter_map(parse::list_entry).collect::<Vec<_>>())
        })
        .await
    }

    /// Mod DB ids (numeric) of mods that have a release for one game version.
    pub async fn catalogue_ids_for_game_version(&self, tag_id: &str, force: bool) -> NetResult<Vec<u32>> {
        let name = format!("catalogue-gv-{tag_id}");
        let (ids, _) = self
            .cached_or_fetch(&name, LIST_MAX_AGE_MS, force, || async {
                let raw = self.client.fetch_mod_list(Some(tag_id)).await?;
                Ok(raw.iter().filter_map(parse::list_entry).map(|e| e.numeric_id).collect::<Vec<_>>())
            })
            .await?;
        Ok(ids)
    }

    pub async fn tags(&self) -> NetResult<Vec<NamedTag>> {
        Ok(self.cached_or_fetch("tags", TAGS_MAX_AGE_MS, false, || self.client.fetch_tags()).await?.0)
    }

    pub async fn game_versions(&self) -> NetResult<Vec<NamedTag>> {
        Ok(self.cached_or_fetch("gameversions", TAGS_MAX_AGE_MS, false, || self.client.fetch_game_versions()).await?.0)
    }

    /// Gets a release archive: from the version cache if present (works offline), otherwise
    /// downloaded, validated and (if `keep`) stored in the cache. Returns the archive path and
    /// whether it is a temporary file the caller should delete after installing.
    pub async fn fetch_release(
        &self,
        mod_id: &str,
        release: &ModDbRelease,
        keep: bool,
        progress: impl FnMut(DownloadProgress),
        cancelled: impl Fn() -> bool,
    ) -> NetResult<(PathBuf, bool)> {
        if let Some(p) = self.cached_mods.locate(mod_id, &release.version, release.file_name.as_deref()) {
            if install::validate_archive(&p).is_ok() {
                return Ok((p, false));
            }
        }
        if self.is_offline() {
            return Err(NetError::Offline);
        }
        let io = |p: &Path, e: String| NetError::Io { path: p.display().to_string(), message: e };
        std::fs::create_dir_all(&self.temp_dir).map_err(|e| io(&self.temp_dir, e.to_string()))?;
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let tmp = self.temp_dir.join(format!("{}-{}-{n}.zip", std::process::id(), cache::sanitize_file_name(mod_id, "mod")));
        self.client.download(&release.download_url, &tmp, progress, cancelled).await?;
        if let Err(e) = install::validate_archive(&tmp) {
            let _ = std::fs::remove_file(&tmp);
            return Err(NetError::BadResponse(e.to_string()));
        }
        if keep {
            if let Ok(stored) = self.cached_mods.store_copy(mod_id, &release.version, release.file_name.as_deref(), &tmp) {
                let _ = std::fs::remove_file(&tmp);
                return Ok((stored, false));
            }
        }
        Ok((tmp, true))
    }
}

#[cfg(test)]
mod tests;
