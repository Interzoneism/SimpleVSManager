use super::*;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/moddb").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn carryon() -> serde_json::Value {
    parse::mod_from_response(&fixture("mod_carryon.json")).unwrap().unwrap()
}

#[test]
fn missing_mod_is_none() {
    assert!(parse::mod_from_response(&fixture("mod_missing.json")).unwrap().is_none());
}

#[test]
fn parses_real_mod() {
    let info = parse::build_info("carryon", &carryon(), Some("1.21.0"), false, 1.0);
    assert_eq!(info.name.as_deref(), Some("Carry On"));
    assert_eq!(info.asset_id, Some(4405));
    assert_eq!(info.page_url.as_deref(), Some("https://mods.vintagestory.at/carryon"));
    assert!(info.logo_url.as_deref().unwrap().starts_with("https://moddbcdn.vintagestory.at/"));
    assert_eq!(info.tags, vec!["Storage", "QoL"]);
    assert_eq!(info.screenshots.len(), 1);
    assert!(info.screenshots[0].url.starts_with("https://moddbcdn.vintagestory.at/CarryOnLogo2_"));
    assert!(info.screenshots[0].thumbnail_url.as_deref().is_some_and(|t| t.ends_with("_55_60.png")));
    assert_eq!(info.releases.len(), 89);
    let latest = info.latest_release().unwrap();
    assert_eq!(latest.version, "2.0.0-rc.1");
    assert_eq!(latest.file_name.as_deref(), Some("CarryOn-1.22.0_v2.0.0-rc.1.zip"));
    assert!(!latest.compatible, "1.22-only release is not compatible with 1.21.0");
    let compat = info.latest_compatible().unwrap();
    assert!(compat.game_versions.iter().any(|v| v.starts_with("1.21")));
    let cl = latest.changelog.as_deref().unwrap();
    assert!(cl.starts_with("CarryOn 2.0.0-rc.1 is the first release candidate"), "{cl}");
    assert!(cl.contains("\n\u{2022} Carry bags from ground storage."), "{cl}");
    assert!(!cl.contains('<'));
    assert!(latest.created_ms.unwrap() > 1.7e12);
}

#[test]
fn target_latest_takes_newest() {
    let info = parse::build_info("carryon", &carryon(), None, false, 1.0);
    assert_eq!(info.latest_compatible().unwrap().version, "2.0.0-rc.1");
}

#[test]
fn update_offer_rules() {
    let info = parse::build_info("carryon", &carryon(), Some("1.21.0"), false, 1.0);
    let compat = info.latest_compatible().unwrap().version.clone();

    // An old installed version gets offered the newest compatible release, not the 1.22 one.
    let s = updates::summarize(&info, Some("1.0.0"), Some("1.21.0"), false, None, false);
    let offer = s.update.unwrap();
    assert_eq!(offer.version, compat);
    assert!(offer.compatible);
    assert_eq!(s.latest_version.as_deref(), Some("2.0.0-rc.1"));

    // Already on it: nothing to offer.
    let s = updates::summarize(&info, Some(&compat), Some("1.21.0"), false, None, false);
    assert!(s.update.is_none());
    assert!(!s.installed_may_be_incompatible);

    // Skipped.
    let s = updates::summarize(&info, Some("1.0.0"), Some("1.21.0"), false, Some(&compat), true);
    assert!(s.update.is_none());
    assert_eq!(s.skipped_version.as_deref(), Some(compat.as_str()));
    assert!(s.excluded_from_bulk_update);

    // Installed 1.22-only build on a 1.21 game.
    let s = updates::summarize(&info, Some("2.0.0-rc.1"), Some("1.21.0"), false, None, false);
    assert!(s.installed_may_be_incompatible);
    assert!(s.update.is_none());
}

#[test]
fn manager_mod_page() {
    let raw = parse::mod_from_response(&fixture("mod_5545.json")).unwrap().unwrap();
    let info = parse::build_info(MANAGER_MOD_ID, &raw, None, false, 1.0);
    assert_eq!(info.latest_release().unwrap().version, "2.3.0");
    assert!(parse::description_text(&raw).is_some());
}

#[test]
fn list_and_browse() {
    let raw = parse::list_from_response(&fixture("mods_search_carry.json")).unwrap();
    let entries: Vec<_> = raw.iter().filter_map(parse::list_entry).collect();
    assert_eq!(entries.len(), raw.len());
    let first = &entries[0];
    assert_eq!(first.mod_id, "carryon");
    assert_eq!(first.name, "Carry On");
    assert!(first.logo_url.is_some());

    let q = BrowseQuery { text: "carry on".into(), ..Default::default() };
    let page = browse(&entries, &q, |e| e.mod_id == "carryon", |_| false, 1.0);
    assert_eq!(page.items[0].entry.mod_id, "carryon");
    assert!(page.items[0].installed);

    let q = BrowseQuery { hide_installed: true, sort: BrowseSort::Name, limit: 5, ..Default::default() };
    let page = browse(&entries, &q, |e| e.mod_id == "carryon", |_| false, 1.0);
    assert_eq!(page.total as usize, entries.len() - 1);
    assert_eq!(page.items.len(), 5);
    let names: Vec<_> = page.items.iter().map(|i| i.entry.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);

    let q = BrowseQuery { tags: vec!["qol".into()], ..Default::default() };
    let page = browse(&entries, &q, |_| false, |_| false, 1.0);
    assert!(page.items.iter().all(|i| i.entry.tags.iter().any(|t| t == "QoL")));
    assert!(!browse::tag_counts(&entries).is_empty());
}

#[test]
fn tags_and_game_versions() {
    let gv = parse::tags_from_response(&fixture("gameversions.json"), "gameversions").unwrap();
    assert!(gv.iter().any(|g| g.name == "1.21.0"));
    assert!(gv[0].id.starts_with('-'));
    let tags = parse::tags_from_response(&fixture("tags.json"), "tags").unwrap();
    assert!(tags.iter().any(|t| t.name == "QoL"));
}

#[tokio::test]
async fn offline_uses_cache_only() {
    let t = tempfile::tempdir().unwrap();
    // Unroutable base: any network attempt would fail, proving none happens.
    let db = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), true);
    db.cache().store_mod("carryon", Some(&carryon()), dates::now_ms()).unwrap();
    let l = db.lookup(&["CarryOn".into(), "unknown".into()], true).await;
    assert!(l.error.is_none());
    assert_eq!(l.fetched, 0);
    assert!(l.mods.contains_key("carryon"));
    assert!(!l.mods.contains_key("unknown"));
    assert!(matches!(db.catalogue(false).await, Err(NetError::Offline)));
}

#[tokio::test]
async fn network_errors_fall_back_to_cache() {
    let t = tempfile::tempdir().unwrap();
    let db = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), false);
    db.cache().store_mod("carryon", Some(&carryon()), 1.0).unwrap(); // very stale
    let l = db.lookup(&["carryon".into()], false).await;
    assert!(l.error.is_some());
    assert!(l.mods["carryon"].raw.is_some());
}

#[tokio::test]
async fn releases_come_from_version_cache_offline() {
    let t = tempfile::tempdir().unwrap();
    let db = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), true);
    let info = parse::build_info("carryon", &carryon(), None, false, 1.0);
    let rel = info.releases[3].clone();
    assert!(matches!(db.fetch_release("carryon", &rel, true, |_| {}, || false).await, Err(NetError::Offline)));

    let slot = db.cached_mods().path_for("carryon", &rel.version, rel.file_name.as_deref());
    std::fs::create_dir_all(slot.parent().unwrap()).unwrap();
    {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::fs::File::create(&slot).unwrap());
        z.start_file("modinfo.json", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"{}").unwrap();
        z.finish().unwrap();
    }
    let (p, temp) = db.fetch_release("carryon", &rel, true, |_| {}, || false).await.unwrap();
    assert_eq!(p, slot);
    assert!(!temp);
}

#[tokio::test]
async fn thumbnails_come_from_cached_mod_data_offline() {
    let t = tempfile::tempdir().unwrap();
    let db = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), true);
    db.cache()
        .store_mod("carryon", Some(&serde_json::json!({"logo": "a.png", "logofiledb": "https://cdn/db.png"})), 1.0)
        .unwrap();
    db.cache().store_mod("nologo", Some(&serde_json::json!({"name": "x"})), 1.0).unwrap();
    let ids = vec!["CarryOn".to_string(), "nologo".to_string(), "unknown".to_string()];
    let got = db.thumbnails(&ids).await;
    assert_eq!(got.get("carryon"), Some(&Some("https://cdn/db.png".to_string())));
    assert_eq!(got.get("nologo"), Some(&None));
    assert!(!got.contains_key("unknown"), "not looked up yet: caller keeps the catalogue logo");

    // Remembered on disk: a fresh instance knows it even after the mod data is gone.
    std::fs::remove_dir_all(db.cache().dir().join("mods")).unwrap();
    let db2 = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), true);
    let got = db2.thumbnails(&["carryon".to_string()]).await;
    assert_eq!(got.get("carryon"), Some(&Some("https://cdn/db.png".to_string())));
}

#[tokio::test]
async fn catalogue_is_kept_in_memory() {
    let t = tempfile::tempdir().unwrap();
    let db = ModDb::with_client(ModDbClient::with_base("http://127.0.0.1:9").unwrap(), t.path(), true);
    let entry = parse::list_from_response(&fixture("mods_top_downloads.json")).unwrap().iter().find_map(parse::list_entry).unwrap();
    db.cache().store("catalogue", &vec![entry], dates::now_ms()).unwrap();
    let (first, _) = db.catalogue_shared(false).await.unwrap();
    // The disk copy is gone, the memory copy is still served.
    db.cache().clear().unwrap();
    let (second, _) = db.catalogue_shared(false).await.unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    db.forget_catalogue();
    assert!(db.catalogue_shared(false).await.is_err());
}
