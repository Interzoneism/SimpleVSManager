//! Mod browser search. The Mod DB returns the whole catalogue in one response, so filtering,
//! sorting and paging happen locally over the cached list (fast, and works offline).

use serde::{Deserialize, Serialize};

use super::model::ModDbListEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BrowseSort {
    /// Best text match first (falls back to downloads with no text).
    #[default]
    Relevance,
    Downloads,
    Follows,
    Trending,
    Comments,
    RecentlyUpdated,
    /// Newest mods first (by Mod DB id, which grows over time).
    RecentlyAdded,
    Name,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct BrowseQuery {
    pub text: String,
    /// Category tag names; a mod must have all of them.
    pub tags: Vec<String>,
    /// "client", "server" or "both".
    pub side: Option<String>,
    pub sort: BrowseSort,
    /// Reverse the sort (default: biggest/newest first, names A to Z).
    pub reverse: bool,
    pub only_favourites: bool,
    pub hide_installed: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BrowseEntry {
    #[serde(flatten)]
    pub entry: ModDbListEntry,
    pub installed: bool,
    pub favourite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    pub total: u32,
    pub items: Vec<BrowseEntry>,
    /// When the catalogue was downloaded.
    pub fetched_ms: f64,
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace().map(|t| t.to_lowercase()).collect()
}

/// Higher is better; `None` = doesn't match every token.
fn score(e: &ModDbListEntry, toks: &[String]) -> Option<f64> {
    if toks.is_empty() {
        return Some(0.0);
    }
    let name = e.name.to_lowercase();
    let author = e.author.as_deref().unwrap_or("").to_lowercase();
    let summary = e.summary.as_deref().unwrap_or("").to_lowercase();
    let ids: Vec<String> = e.mod_ids.iter().map(|s| s.to_lowercase()).collect();
    let tags: Vec<String> = e.tags.iter().map(|s| s.to_lowercase()).collect();
    let mut total = 0.0;
    for t in toks {
        let s = if name == *t || ids.iter().any(|i| i == t) {
            100.0
        } else if name.starts_with(t.as_str()) || name.split_whitespace().any(|w| w.starts_with(t.as_str())) {
            60.0
        } else if name.contains(t.as_str()) || ids.iter().any(|i| i.contains(t.as_str())) {
            40.0
        } else if author.contains(t.as_str()) {
            25.0
        } else if tags.iter().any(|g| g.contains(t.as_str())) {
            15.0
        } else if summary.contains(t.as_str()) {
            10.0
        } else {
            return None;
        };
        total += s;
    }
    // Popularity breaks ties without drowning out the text match.
    Some(total + (e.downloads as f64 + 1.0).log10())
}

/// Filters, sorts and pages `entries`. `is_installed`/`is_favourite` take the entry's ids.
pub fn browse(
    entries: &[ModDbListEntry],
    q: &BrowseQuery,
    is_installed: impl Fn(&ModDbListEntry) -> bool,
    is_favourite: impl Fn(&ModDbListEntry) -> bool,
    fetched_ms: f64,
) -> BrowsePage {
    let toks = tokens(&q.text);
    let mut hits: Vec<(f64, BrowseEntry)> = entries
        .iter()
        .filter(|e| q.tags.iter().all(|t| e.tags.iter().any(|g| g.eq_ignore_ascii_case(t))))
        .filter(|e| match q.side.as_deref() {
            Some(s) if !s.is_empty() => e.side.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(s)),
            _ => true,
        })
        .filter_map(|e| {
            let sc = score(e, &toks)?;
            let installed = is_installed(e);
            let favourite = is_favourite(e);
            if (q.only_favourites && !favourite) || (q.hide_installed && installed) {
                return None;
            }
            Some((sc, BrowseEntry { entry: e.clone(), installed, favourite }))
        })
        .collect();

    let key = |e: &ModDbListEntry| -> f64 {
        match q.sort {
            BrowseSort::Relevance | BrowseSort::Downloads => e.downloads as f64,
            BrowseSort::Follows => e.follows as f64,
            BrowseSort::Trending => e.trending_points as f64,
            BrowseSort::Comments => e.comments as f64,
            BrowseSort::RecentlyUpdated => e.last_released_ms.unwrap_or(0.0),
            BrowseSort::RecentlyAdded => e.numeric_id as f64,
            BrowseSort::Name => 0.0,
        }
    };
    hits.sort_by(|(sa, a), (sb, b)| {
        let ord = match q.sort {
            BrowseSort::Name => b.entry.name.to_lowercase().cmp(&a.entry.name.to_lowercase()),
            BrowseSort::Relevance if !toks.is_empty() => sa.total_cmp(sb),
            _ => key(&a.entry).total_cmp(&key(&b.entry)),
        };
        // `ord` is ascending; the default shows the biggest first (names A to Z).
        if q.reverse { ord } else { ord.reverse() }
    });

    let total = hits.len() as u32;
    let limit = if q.limit == 0 { 60 } else { q.limit.min(500) };
    let items = hits.into_iter().skip(q.offset as usize).take(limit as usize).map(|(_, e)| e).collect();
    BrowsePage { total, items, fetched_ms }
}

/// Every category tag used in the list, most common first.
pub fn tag_counts(entries: &[ModDbListEntry]) -> Vec<(String, u32)> {
    let mut map: std::collections::HashMap<String, (String, u32)> = std::collections::HashMap::new();
    for e in entries {
        for t in &e.tags {
            map.entry(t.to_lowercase()).or_insert_with(|| (t.clone(), 0)).1 += 1;
        }
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}
