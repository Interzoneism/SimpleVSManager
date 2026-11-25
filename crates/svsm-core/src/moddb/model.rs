//! Data shapes the UI sees. Counts are `u32` and timestamps are Unix milliseconds as `f64`
//! because TypeScript has no 64-bit integers.

use serde::{Deserialize, Serialize};

/// One downloadable release of a mod.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModDbRelease {
    pub version: String,
    pub download_url: String,
    pub file_name: Option<String>,
    /// Game versions the author tagged this release with.
    pub game_versions: Vec<String>,
    /// Supports the target game version (always true when the target is "latest").
    pub compatible: bool,
    /// Plain text, converted from the Mod DB's HTML.
    pub changelog: Option<String>,
    pub downloads: Option<u32>,
    pub created_ms: Option<f64>,
}

/// Everything the Mod DB says about one mod, evaluated against a target game version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModDbInfo {
    /// The id this entry was looked up by (the installed mod's id).
    pub mod_id: String,
    pub asset_id: Option<u32>,
    pub name: Option<String>,
    pub author: Option<String>,
    pub page_url: Option<String>,
    pub logo_url: Option<String>,
    pub side: Option<String>,
    /// Category tags ("QoL", "Storage", ...).
    pub tags: Vec<String>,
    pub downloads: Option<u32>,
    pub follows: Option<u32>,
    pub comments: Option<u32>,
    pub trending_points: Option<u32>,
    pub created_ms: Option<f64>,
    pub last_released_ms: Option<f64>,
    pub homepage_url: Option<String>,
    pub source_code_url: Option<String>,
    pub issue_tracker_url: Option<String>,
    pub wiki_url: Option<String>,
    /// Newest first, as the Mod DB lists them.
    pub releases: Vec<ModDbRelease>,
    /// Index into `releases` of the newest release that supports the target game version.
    pub latest_compatible_index: Option<u32>,
    /// When this data was downloaded.
    pub fetched_ms: f64,
}

impl ModDbInfo {
    pub fn latest_release(&self) -> Option<&ModDbRelease> {
        self.releases.first()
    }

    pub fn latest_compatible(&self) -> Option<&ModDbRelease> {
        self.latest_compatible_index.and_then(|i| self.releases.get(i as usize))
    }

    pub fn find_release(&self, version: &str) -> Option<&ModDbRelease> {
        let norm = crate::version::normalize(version);
        self.releases.iter().find(|r| {
            r.version.eq_ignore_ascii_case(version)
                || (norm.is_some() && crate::version::normalize(&r.version) == norm && !has_suffix(&r.version))
        })
    }
}

/// Pre-release suffixes make "2.0.0-rc.1" and "2.0.0.1" normalize alike; prefer exact matches.
fn has_suffix(v: &str) -> bool {
    v.chars().any(|c| c.is_ascii_alphabetic())
}

/// What the installed-mods list needs per mod: small, no changelogs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModDbSummary {
    pub mod_id: String,
    pub asset_id: Option<u32>,
    pub page_url: Option<String>,
    pub logo_url: Option<String>,
    pub tags: Vec<String>,
    pub downloads: Option<u32>,
    pub side: Option<String>,
    pub latest_version: Option<String>,
    pub latest_compatible_version: Option<String>,
    /// Game versions the installed release was tagged with.
    pub installed_release_game_versions: Vec<String>,
    /// The installed release doesn't list the target game version.
    pub installed_may_be_incompatible: bool,
    pub update: Option<UpdateOffer>,
    /// The newest release is skipped by the user (so `update` may be `None`).
    pub skipped_version: Option<String>,
    pub excluded_from_bulk_update: bool,
    pub release_count: u32,
    pub fetched_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    pub version: String,
    pub compatible: bool,
    pub message: String,
}

/// One entry from the Mod DB's mod list (`/api/mods`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModDbListEntry {
    /// Primary string id (`modidstrs[0]`), or the numeric id if there is none.
    pub mod_id: String,
    pub mod_ids: Vec<String>,
    pub numeric_id: u32,
    pub asset_id: Option<u32>,
    pub name: String,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub url_alias: Option<String>,
    pub side: Option<String>,
    pub kind: Option<String>,
    pub logo_url: Option<String>,
    pub tags: Vec<String>,
    pub downloads: u32,
    pub follows: u32,
    pub comments: u32,
    pub trending_points: u32,
    pub last_released_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NamedTag {
    /// Kept as a string: game-version tag ids are large negative numbers.
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}
