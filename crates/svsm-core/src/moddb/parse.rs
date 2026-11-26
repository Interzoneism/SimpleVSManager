//! Turns raw Mod DB JSON into [`ModDbInfo`] / [`ModDbListEntry`]. Port of the parsing in
//! `ModDatabaseService.cs`. Raw JSON is what gets cached, so a change of target game version
//! never needs a new download.

use serde_json::Value;

use super::changelog::html_to_text;
use super::dates::parse_utc_ms;
use super::json::{int, status_code, string, strings};
use super::model::{ModDbInfo, ModDbListEntry, ModDbRelease, ModDbScreenshot, NamedTag};
use crate::version;

pub const SITE_URL: &str = "https://mods.vintagestory.at";

/// Result of `/api/mod/<id>`: the `mod` object, or `None` when the Mod DB doesn't know it
/// (it answers HTTP 200 with `{"statuscode":"404"}`).
pub fn mod_from_response(body: &str) -> Result<Option<Value>, String> {
    let root: Value = serde_json::from_str(body.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    if status_code(&root).is_some_and(|c| c != 200) {
        return Ok(None);
    }
    Ok(root.get("mod").filter(|m| m.is_object()).cloned())
}

/// Result of `/api/mods`: the raw entries.
pub fn list_from_response(body: &str) -> Result<Vec<Value>, String> {
    let root: Value = serde_json::from_str(body.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    if let Some(code) = status_code(&root).filter(|c| *c != 200) {
        return Err(format!("the Mod DB answered with status {code}"));
    }
    Ok(match root.get("mods") {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    })
}

/// Result of `/api/tags` or `/api/gameversions`.
pub fn tags_from_response(body: &str, key: &str) -> Result<Vec<NamedTag>, String> {
    let root: Value = serde_json::from_str(body.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let items = root.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|t| {
            Some(NamedTag {
                id: string(t, "tagid")?,
                name: string(t, "name")?,
                color: string(t, "color"),
            })
        })
        .collect())
}

fn count(v: &Value, key: &str) -> Option<u32> {
    int(v, key).map(|n| n.clamp(0, u32::MAX as i64) as u32)
}

pub fn page_url(asset_id: Option<u32>, url_alias: Option<&str>) -> Option<String> {
    match (url_alias.filter(|a| !a.is_empty()), asset_id) {
        (Some(alias), _) => Some(format!("{SITE_URL}/{alias}")),
        (None, Some(id)) => Some(format!("{SITE_URL}/show/mod/{id}")),
        _ => None,
    }
}

/// Does a release tagged with `tags` support `target`? No target = everything is compatible.
pub fn tags_support_target(tags: &[String], target: Option<&str>, exact: bool) -> bool {
    let Some(target) = target.and_then(version::normalize) else {
        return true;
    };
    tags.iter().any(|t| version::supports_version(t, &target, exact))
}

fn release(v: &Value, target: Option<&str>, exact: bool) -> Option<ModDbRelease> {
    let download_url = string(v, "mainfile").filter(|u| u.starts_with("http://") || u.starts_with("https://"))?;
    let version = string(v, "modversion").or_else(|| string(v, "version"))?;
    let game_versions = strings(v, "tags");
    Some(ModDbRelease {
        compatible: tags_support_target(&game_versions, target, exact),
        version,
        download_url,
        file_name: string(v, "filename"),
        game_versions,
        changelog: string(v, "changelog").and_then(|c| html_to_text(&c)),
        downloads: count(v, "downloads"),
        created_ms: string(v, "created").and_then(|d| parse_utc_ms(&d)),
    })
}

/// The newest release's version of a raw `mod` object (the Mod DB lists releases newest first).
pub fn latest_version(raw: &Value) -> Option<String> {
    raw.get("releases")?.as_array()?.iter().filter(|r| r.is_object()).find_map(|r| release(r, None, false)).map(|r| r.version)
}

/// Builds the info for one mod. `target` is the effective target game version (`None` = any).
pub fn build_info(mod_id: &str, raw: &Value, target: Option<&str>, exact: bool, fetched_ms: f64) -> ModDbInfo {
    let releases: Vec<ModDbRelease> = raw
        .get("releases")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter(|r| r.is_object()).filter_map(|r| release(r, target, exact)).collect())
        .unwrap_or_default();
    let latest_compatible_index = releases.iter().position(|r| r.compatible).map(|i| i as u32);
    let asset_id = count(raw, "assetid").filter(|&a| a > 0);
    let url_alias = string(raw, "urlalias");
    ModDbInfo {
        mod_id: mod_id.to_string(),
        asset_id,
        name: string(raw, "name"),
        author: string(raw, "author"),
        page_url: page_url(asset_id, url_alias.as_deref()),
        logo_url: db_logo(raw),
        side: string(raw, "side"),
        tags: strings(raw, "tags"),
        downloads: count(raw, "downloads"),
        follows: count(raw, "follows"),
        comments: count(raw, "comments"),
        trending_points: count(raw, "trendingpoints"),
        created_ms: string(raw, "created").and_then(|d| parse_utc_ms(&d)),
        last_released_ms: string(raw, "lastreleased").and_then(|d| parse_utc_ms(&d)),
        homepage_url: string(raw, "homepageurl"),
        source_code_url: string(raw, "sourcecodeurl"),
        issue_tracker_url: string(raw, "issuetrackerurl"),
        wiki_url: string(raw, "wikiurl"),
        screenshots: screenshots(raw),
        releases,
        latest_compatible_index,
        fetched_ms,
    }
}

/// Screenshots of a raw `mod` object. Skips entries without a usable web address (2.x also
/// dropped ones with an invalid file id).
pub fn screenshots(raw: &Value) -> Vec<ModDbScreenshot> {
    let web = |s: Option<String>| s.filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    raw.get("screenshots")
        .and_then(Value::as_array)
        .map(|all| {
            all.iter()
                .filter(|s| s.get("fileid").and_then(Value::as_i64).is_none_or(|id| id > 0))
                .filter_map(|s| {
                    Some(ModDbScreenshot {
                        url: web(string(s, "mainfile"))?,
                        thumbnail_url: web(string(s, "thumbnailfilename")),
                        file_name: string(s, "filename"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The picture the Mod DB shows on its mod cards (`logofiledb`), from a raw `mod` object.
pub fn db_logo(raw: &Value) -> Option<String> {
    string(raw, "logofiledb").or_else(|| string(raw, "logofile"))
}

/// The mod's description (HTML) from a raw `mod` object, as plain text.
pub fn description_text(raw: &Value) -> Option<String> {
    string(raw, "text").and_then(|t| html_to_text(&t))
}

pub fn list_entry(v: &Value) -> Option<ModDbListEntry> {
    let numeric_id = count(v, "modid")?;
    let mod_ids = strings(v, "modidstrs");
    let asset_id = count(v, "assetid").filter(|&a| a > 0);
    Some(ModDbListEntry {
        mod_id: mod_ids.first().cloned().unwrap_or_else(|| numeric_id.to_string()),
        mod_ids,
        numeric_id,
        asset_id,
        name: string(v, "name").unwrap_or_else(|| numeric_id.to_string()),
        summary: string(v, "summary").map(|s| super::changelog::decode_entities(&s)),
        author: string(v, "author"),
        url_alias: string(v, "urlalias"),
        side: string(v, "side"),
        kind: string(v, "type"),
        logo_url: string(v, "logofiledb").or_else(|| string(v, "logo")),
        tags: strings(v, "tags"),
        downloads: count(v, "downloads").unwrap_or(0),
        follows: count(v, "follows").unwrap_or(0),
        comments: count(v, "comments").unwrap_or(0),
        trending_points: count(v, "trendingpoints").unwrap_or(0),
        last_released_ms: string(v, "lastreleased").and_then(|d| parse_utc_ms(&d)),
    })
}
