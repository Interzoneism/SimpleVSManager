//! "Suggested for you" in the mod browser. The community data (public modlists and reports) is
//! downloaded at most every [`MAX_AGE`] (ETag-cached on disk), kept in memory, and ranking runs
//! locally, so paging or switching pages doesn't touch the network.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use svsm_core::cloud::suggest::{Signals, VoteSignalSource};
use svsm_core::cloud::CloudStore;
use svsm_core::moddb::{ModDbListEntry, SuggestBasis, SuggestionHint};
use svsm_core::sync::RwLockExt as _;
use tokio::sync::Mutex;

use crate::cloud_commands::{cache_dir, cached_registry};
use crate::state::AppState;

const MAX_AGE: Duration = Duration::from_secs(30 * 60);
/// After a failed download, wait this long before trying again (the cached copy is used).
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

struct Cached {
    dir: PathBuf,
    at: Instant,
    signals: Arc<Signals>,
    error: Option<String>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::const_new(None);

/// The community data, downloading it again when it's old (or `refresh`). Never fails: falls back
/// to the copies on disk, then to nothing, and says why.
async fn signals(state: &AppState, refresh: bool) -> (Arc<Signals>, Option<String>) {
    let dir = cache_dir(state);
    let mut slot = CACHE.lock().await;
    if let Some(c) = slot.as_ref().filter(|c| c.dir == dir) {
        let max = if c.error.is_some() { RETRY_AFTER } else { MAX_AGE };
        if !refresh && c.at.elapsed() < max {
            return (c.signals.clone(), c.error.clone());
        }
    }
    let votes = VoteSignalSource::new(state.cloud_config.clone(), &dir);
    let (registry, votes, error) = match state.cloud_auth() {
        Ok(auth) => {
            let store = CloudStore::new(&auth, state.cloud_config.clone(), &dir);
            let (reg, reg_err) = match &store {
                Ok(s) => match s.registry().await {
                    Ok(r) => (r, None),
                    Err(e) => (s.cached_registry(), Some(e.to_string())),
                },
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let (v, v_err) = match votes.fetch().await {
                Ok(v) => (v, None),
                Err(e) => (votes.cached().unwrap_or_default(), Some(e.to_string())),
            };
            (reg, v, reg_err.or(v_err))
        }
        // Offline mode: what was downloaded before.
        Err(e) => (cached_registry(state), votes.cached().unwrap_or_default(), Some(e.to_string())),
    };
    let signals = Arc::new(Signals::new(&registry, &votes));
    *slot = Some(Cached { dir, at: Instant::now(), signals: signals.clone(), error: error.clone() });
    (signals, error)
}

/// Report counts (best to worst) for one mod version, from the same cached community data.
pub(crate) async fn version_counts(state: &AppState) -> impl Fn(&str, &str) -> Option<[u32; 5]> {
    let (signals, _) = signals(state, false).await;
    move |mod_id: &str, version: &str| signals.version_counts(mod_id, version)
}

/// Suggestion scores for catalogue entries, and what they were based on. `seed_mod_ids` empty =
/// the installed mods.
pub(crate) async fn scorer(
    state: &AppState,
    seed_mod_ids: &[String],
    refresh: bool,
) -> (impl Fn(&ModDbListEntry) -> Option<SuggestionHint>, SuggestBasis) {
    let from_builder = !seed_mod_ids.is_empty();
    let seeds: Vec<String> = if from_builder {
        seed_mod_ids.to_vec()
    } else {
        state.mods.read_safe().mods.iter().map(|m| m.mod_id.clone()).collect()
    };
    let (signals, error) = signals(state, refresh).await;
    let ranked: HashMap<String, SuggestionHint> = signals
        .rank(&seeds)
        .into_iter()
        .map(|s| (s.mod_id, SuggestionHint { score: s.score, together: s.together }))
        .collect();
    let seed_set: HashSet<String> = seeds.iter().map(|s| s.trim().to_lowercase()).collect();
    let basis = SuggestBasis {
        from_builder,
        seed_count: seed_set.len() as u32,
        modlists: signals.modlists,
        reporters: signals.voters,
        error: if signals.is_empty() { error.or_else(|| Some("No community modlists yet.".into())) } else { error },
    };
    let score = move |e: &ModDbListEntry| -> Option<SuggestionHint> {
        let ids = || std::iter::once(&e.mod_id).chain(e.mod_ids.iter()).map(|i| i.to_lowercase());
        // Already one of the seeds (under any of its ids).
        if ids().any(|i| seed_set.contains(&i)) {
            return None;
        }
        ids().filter_map(|i| ranked.get(&i).copied()).max_by(|a, b| a.score.total_cmp(&b.score))
    };
    (score, basis)
}
