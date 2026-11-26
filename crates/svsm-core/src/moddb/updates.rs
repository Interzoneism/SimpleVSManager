//! Update availability. Port of `ModListItemViewModel.InitializeUpdateAvailability`.

use super::model::{ModDbInfo, ModDbRelease, ModDbSummary, UpdateOffer};
use super::parse::tags_support_target;
use crate::version;

/// The release a mod would be updated to, ignoring skips: the newest compatible release, or
/// the newest release when it has no game-version tags at all.
pub fn update_candidate(info: &ModDbInfo) -> Option<&ModDbRelease> {
    info.latest_compatible()
        .or_else(|| info.latest_release().filter(|r| r.game_versions.is_empty()))
}

fn same_version(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The release matching the installed version.
pub fn installed_release<'a>(info: &'a ModDbInfo, installed: Option<&str>) -> Option<&'a ModDbRelease> {
    let installed = installed?.trim();
    if installed.is_empty() {
        return None;
    }
    info.releases.iter().find(|r| same_version(&r.version, installed)).or_else(|| {
        let n = version::normalize(installed)?;
        info.releases.iter().find(|r| version::normalize(&r.version).as_deref() == Some(n.as_str()))
    })
}

fn offer_message(info: &ModDbInfo, release: &ModDbRelease) -> String {
    if release.compatible {
        return format!("Update available: {}", release.version);
    }
    match info.latest_compatible() {
        Some(c) if !same_version(&c.version, &release.version) => {
            format!("Update available: {} (latest compatible: {})", release.version, c.version)
        }
        _ => format!("Update available: {} (may not work with your game version)", release.version),
    }
}

/// Per-mod summary for the installed list.
pub fn summarize(
    info: &ModDbInfo,
    installed_version: Option<&str>,
    target: Option<&str>,
    exact: bool,
    skipped: Option<&str>,
    excluded: bool,
) -> ModDbSummary {
    let candidate = update_candidate(info)
        .filter(|c| version::is_candidate_newer(Some(&c.version), installed_version));
    let skipped_hit = match (candidate, skipped) {
        (Some(c), Some(s)) if same_version(&c.version, s) => Some(c.version.clone()),
        _ => None,
    };
    let update = candidate.filter(|_| skipped_hit.is_none()).map(|c| UpdateOffer {
        version: c.version.clone(),
        compatible: c.compatible,
        message: offer_message(info, c),
    });
    let installed = installed_release(info, installed_version);
    let installed_tags = installed.map(|r| r.game_versions.clone()).unwrap_or_default();
    let installed_may_be_incompatible =
        target.is_some() && !installed_tags.is_empty() && !tags_support_target(&installed_tags, target, exact);

    ModDbSummary {
        mod_id: info.mod_id.clone(),
        asset_id: info.asset_id,
        page_url: info.page_url.clone(),
        logo_url: info.logo_url.clone(),
        tags: info.tags.clone(),
        downloads: info.downloads,
        side: info.side.clone(),
        latest_version: info.latest_release().map(|r| r.version.clone()),
        latest_compatible_version: info.latest_compatible().map(|r| r.version.clone()),
        installed_release_game_versions: installed_tags,
        installed_may_be_incompatible,
        update,
        skipped_version: skipped_hit,
        excluded_from_bulk_update: excluded,
        release_count: info.releases.len() as u32,
        fetched_ms: info.fetched_ms,
    }
}
