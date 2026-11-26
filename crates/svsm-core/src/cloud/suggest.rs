//! "Suggested for you" in the mod browser: mods that the community uses together with yours.
//!
//! Two public signals, both read in one request each and cached with an ETag:
//!
//! * **Modlists** (`registrySummaries`): every public modlist is a set of mods people run together.
//! * **Reports** (`compatVotes`): every person who said a mod works is running it, so the mods one
//!   person reported on form another (smaller, weaker) set. Mostly bad reports push a mod down.
//!
//! Ranking is item-to-item co-occurrence: a candidate scores for each seed mod it shares a set with,
//! normalised by how common both are (cosine similarity), so a mod that is in every list doesn't
//! win just by being everywhere. Large sets count less than small, focused ones.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::RegistryEntry;
use super::votes::VoteOption;
use super::{http_client, CloudConfig, CloudError, CloudResult};

/// How much one person's reports count next to one published modlist.
const VOTER_WEIGHT: f64 = 0.5;

/// What the reports say, reduced to what ranking needs. Cached on disk with its ETag.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteSignals {
    pub etag: Option<String>,
    /// Per person: lower-case ids of the mods they reported as working (in any way).
    pub voters: Vec<Vec<String>>,
    /// Per lower-case mod id: (working reports, not working/crashing reports).
    pub health: HashMap<String, (u32, u32)>,
}

/// Turns the whole `compatVotes` tree into [`VoteSignals`]. Mod keys are URL-safe base64 of the id.
pub fn parse_votes(tree: &Value) -> VoteSignals {
    let mut by_voter: HashMap<&str, HashSet<String>> = HashMap::new();
    let mut health: HashMap<String, (u32, u32)> = HashMap::new();
    let Some(mods) = tree.as_object() else { return VoteSignals::default() };
    for (mod_key, versions) in mods {
        let Some(mod_id) = decode_key(mod_key) else { continue };
        let Some(versions) = versions.as_object() else { continue };
        // One person counts once per mod, whatever versions they reported on (latest wins is
        // unknowable here, so a person with both kinds counts as both).
        let mut good: HashSet<&str> = HashSet::new();
        let mut bad: HashSet<&str> = HashSet::new();
        for version in versions.values() {
            let Some(users) = version.get("users").and_then(Value::as_object) else { continue };
            for (user, rec) in users {
                let Some(option) = rec.get("option").and_then(Value::as_str).and_then(VoteOption::parse) else { continue };
                if option.needs_comment() {
                    bad.insert(user.as_str());
                } else {
                    good.insert(user.as_str());
                }
            }
        }
        if good.is_empty() && bad.is_empty() {
            continue;
        }
        for u in &good {
            by_voter.entry(u).or_default().insert(mod_id.clone());
        }
        health.insert(mod_id, (good.len() as u32, bad.len() as u32));
    }
    let mut voters: Vec<Vec<String>> = by_voter
        .into_values()
        .filter(|m| m.len() >= 2)
        .map(|m| {
            let mut v: Vec<String> = m.into_iter().collect();
            v.sort();
            v
        })
        .collect();
    voters.sort();
    VoteSignals { etag: None, voters, health }
}

fn decode_key(key: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(key.trim_end_matches('=')).ok()?;
    let id = String::from_utf8(bytes).ok()?.trim().to_lowercase();
    (!id.is_empty()).then_some(id)
}

/// Reads `compatVotes` (public), reusing the cached copy when the ETag still matches.
pub struct VoteSignalSource {
    config: CloudConfig,
    cache_file: PathBuf,
}

impl VoteSignalSource {
    pub fn new(config: CloudConfig, cache_dir: &Path) -> Self {
        Self { config, cache_file: cache_dir.join("svsm3-vote-signals.json") }
    }

    pub fn cached(&self) -> Option<VoteSignals> {
        serde_json::from_str(&std::fs::read_to_string(&self.cache_file).ok()?).ok()
    }

    pub async fn fetch(&self) -> CloudResult<VoteSignals> {
        let cached = self.cached();
        let url = format!("{}/compatVotes.json", self.config.db_url.trim_end_matches('/'));
        let mut req = http_client()?.get(url).header("X-Firebase-ETag", "true");
        if let Some(e) = cached.as_ref().and_then(|c| c.etag.as_deref()) {
            req = req.header("If-None-Match", e);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(c) = cached {
                return Ok(c);
            }
        }
        let etag = resp.headers().get("ETag").and_then(|v| v.to_str().ok()).map(String::from);
        let text = resp.text().await.unwrap_or_default();
        let tree: Value = match status.as_u16() {
            200 => serde_json::from_str(&text).map_err(|e| CloudError::BadResponse(e.to_string()))?,
            404 => Value::Null,
            code => return Err(CloudError::BadResponse(format!("Reading reports: HTTP {code}"))),
        };
        let mut s = parse_votes(&tree);
        s.etag = etag;
        if let Some(dir) = self.cache_file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.cache_file, serde_json::to_string(&s).unwrap_or_default());
        Ok(s)
    }
}

/// Everything ranking needs, built once per download of the two sources.
#[derive(Debug, Clone, Default)]
pub struct Signals {
    /// Each set of mods used together (lower-case ids) and how much it counts.
    sets: Vec<(Vec<String>, f64)>,
    /// Per mod: indices into `sets`.
    index: HashMap<String, Vec<usize>>,
    /// Per mod: summed weight of the sets it's in.
    weight: HashMap<String, f64>,
    health: HashMap<String, (u32, u32)>,
    pub modlists: u32,
    pub voters: u32,
}

impl Signals {
    pub fn new(registry: &[RegistryEntry], votes: &VoteSignals) -> Self {
        let mut s = Signals { health: votes.health.clone(), ..Default::default() };
        let mut seen_lists: HashSet<Vec<String>> = HashSet::new();
        for e in registry {
            let mut ids: Vec<String> =
                e.mods.iter().map(|m| m.mod_id.trim().to_lowercase()).filter(|m| !m.is_empty()).collect();
            ids.sort();
            ids.dedup();
            // The same list published twice (or copied) shouldn't count twice.
            if ids.len() < 2 || !seen_lists.insert(ids.clone()) {
                continue;
            }
            s.modlists += 1;
            s.push(ids, 1.0);
        }
        for v in &votes.voters {
            s.voters += 1;
            s.push(v.clone(), VOTER_WEIGHT);
        }
        s
    }

    fn push(&mut self, ids: Vec<String>, base: f64) {
        // A 200-mod list says less about any one pair than a focused 10-mod one.
        let w = base / (2.0 + ids.len() as f64).log2();
        let i = self.sets.len();
        for id in &ids {
            self.index.entry(id.clone()).or_default().push(i);
            *self.weight.entry(id.clone()).or_default() += w;
        }
        self.sets.push((ids, w));
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Scales a score down for mods people mostly report as broken (1.0 without reports).
    fn health_factor(&self, id: &str) -> f64 {
        match self.health.get(id) {
            Some(&(good, bad)) => (good as f64 + 1.0) / (good as f64 + bad as f64 + 1.0),
            None => 1.0,
        }
    }

    /// Mods to suggest for `seeds` (any case), best first; seeds themselves are left out. With no
    /// seed that appears in any set, falls back to what the community uses most.
    pub fn rank(&self, seeds: &[String]) -> Vec<Suggestion> {
        let seeds: HashSet<String> = seeds.iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
        let mut co: HashMap<&str, (f64, u32)> = HashMap::new();
        let mut matched = 0usize;
        for seed in &seeds {
            let Some(sets) = self.index.get(seed) else { continue };
            matched += 1;
            let seed_w = self.weight[seed];
            // Per candidate: weight shared with this seed.
            let mut shared: HashMap<&str, (f64, u32)> = HashMap::new();
            for &i in sets {
                let (ids, w) = &self.sets[i];
                for id in ids {
                    if !seeds.contains(id) {
                        let e = shared.entry(id.as_str()).or_default();
                        e.0 += w;
                        e.1 += 1;
                    }
                }
            }
            for (id, (w, n)) in shared {
                let cosine = w / (seed_w * self.weight[id]).sqrt();
                let e = co.entry(id).or_default();
                e.0 += cosine;
                e.1 = e.1.max(n);
            }
        }
        let mut out: Vec<Suggestion> = if matched == 0 {
            self.weight
                .iter()
                .filter(|(id, _)| !seeds.contains(*id))
                .map(|(id, w)| Suggestion { mod_id: id.clone(), score: *w, together: 0, sets: self.index[id].len() as u32 })
                .collect()
        } else {
            co.into_iter()
                // One shared set is noise unless it's all there is.
                .filter(|(_, (_, n))| *n >= 2 || self.sets.len() < 20)
                .map(|(id, (score, n))| Suggestion { mod_id: id.to_string(), score, together: n, sets: self.index[id].len() as u32 })
                .collect()
        };
        for s in &mut out {
            s.score *= self.health_factor(&s.mod_id);
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.mod_id.cmp(&b.mod_id)));
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// Lower case.
    pub mod_id: String,
    pub score: f64,
    /// Most modlists/reporters that had it with any one of the seed mods (0 = popularity fallback).
    pub together: u32,
    /// Modlists/reporters that have it at all.
    pub sets: u32,
}

#[cfg(test)]
mod tests {
    use super::super::store::RegistryMod;
    use super::super::votes::key;
    use super::*;
    use serde_json::json;

    fn list(id: &str, mods: &[&str]) -> RegistryEntry {
        RegistryEntry {
            registry_id: id.into(),
            name: None,
            description: None,
            icon: None,
            icon_color: None,
            version: None,
            uploader: None,
            game_version: None,
            mods: mods.iter().map(|m| RegistryMod { mod_id: m.to_string(), version: None }).collect(),
            revision: 1.0,
            created_ms: None,
            updated_ms: None,
            complete: false,
        }
    }

    fn lists() -> Vec<RegistryEntry> {
        let mut v = vec![
            list("1", &["CarryOn", "expandedfoods", "aculinaryartillery"]),
            list("2", &["carryon", "expandedfoods", "aculinaryartillery", "primitivesurvival"]),
            list("3", &["carryon", "expandedfoods", "aculinaryartillery", "betterruins"]),
            list("4", &["primitivesurvival", "butchering", "carryon"]),
            list("5", &["primitivesurvival", "butchering"]),
            list("6", &["primitivesurvival", "butchering"]),
        ];
        // Filler so the "one shared set is noise" rule applies.
        for i in 0..20 {
            v.push(list(&format!("f{i}"), &[&format!("x{i}"), &format!("y{i}")]));
        }
        v
    }

    #[test]
    fn ranks_by_co_occurrence_and_leaves_out_seeds() {
        let s = Signals::new(&lists(), &VoteSignals::default());
        let r = s.rank(&["ExpandedFoods".into()]);
        assert_eq!(r[0].mod_id, "aculinaryartillery");
        assert_eq!(r[0].together, 3);
        assert!(r.iter().all(|x| x.mod_id != "expandedfoods"));
        // primitivesurvival shares only one list with expandedfoods: dropped as noise.
        assert!(r.iter().all(|x| x.mod_id != "primitivesurvival"));

        let r = s.rank(&["butchering".into()]);
        assert_eq!(r[0].mod_id, "primitivesurvival");
    }

    #[test]
    fn duplicate_lists_count_once_and_no_seed_falls_back_to_popular() {
        let mut l = lists();
        l.push(list("dup", &["carryon", "expandedfoods", "aculinaryartillery"]));
        let s = Signals::new(&l, &VoteSignals::default());
        assert_eq!(s.modlists, 25); // 5 and 6 are the same list too
        let r = s.rank(&["notinanylist".into()]);
        assert_eq!(r[0].mod_id, "carryon");
        assert_eq!(r[0].together, 0);
    }

    #[test]
    fn reports_add_sets_and_bad_reports_push_down() {
        let tree = json!({
            key("carryon"): {"MS4w": {"users": {
                "a": {"option": "fullyFunctional"}, "b": {"option": "noIssuesSoFar"}, "c": {"option": "crashesOrFreezesGame"}
            }}},
            key("butchering"): {"MS4w": {"users": {"a": {"option": "fullyFunctional"}, "b": {"option": "someIssuesButWorks"}}}},
            key("aculinaryartillery"): {"MS4w": {"users": {
                "c": {"option": "notFunctional"}, "d": {"option": "notFunctional"}, "e": {"option": "crashesOrFreezesGame"}
            }}},
        });
        let v = parse_votes(&tree);
        assert_eq!(v.health["carryon"], (2, 1));
        assert_eq!(v.health["aculinaryartillery"], (0, 3));
        // a and b both report carryon + butchering; c only has bad reports.
        assert_eq!(v.voters, vec![vec!["butchering".to_string(), "carryon".to_string()]; 2]);

        let s = Signals::new(&lists(), &v);
        assert_eq!(s.voters, 2);
        let r = s.rank(&["expandedfoods".into()]);
        let culinary = r.iter().find(|x| x.mod_id == "aculinaryartillery").unwrap();
        let carry = r.iter().find(|x| x.mod_id == "carryon").unwrap();
        // Same lists as carryon, but reported broken: now below it.
        assert!(carry.score > culinary.score);
    }
}
