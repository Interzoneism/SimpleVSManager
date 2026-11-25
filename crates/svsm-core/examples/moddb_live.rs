//! Dev check against the real Mod DB. Read-only for the data folder; all caches go to a temp
//! folder. Usage: moddb_live <VintagestoryData> [game version]
use std::time::Instant;

use svsm_core::clientsettings::ClientSettings;
use svsm_core::moddb::{parse, updates, ModDb};
use svsm_core::mods::discover_mods;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = std::path::PathBuf::from(&args[0]);
    let target = args.get(1).cloned();
    let tmp = std::env::temp_dir().join(format!("svsm-moddb-live-{}", std::process::id()));
    let db = ModDb::new(&tmp, false).expect("client");

    let settings = ClientSettings::load(&data).unwrap();
    let mods = discover_mods(&settings, None).mods;
    let ids: Vec<String> = mods.iter().map(|m| m.mod_id.clone()).filter(|i| !i.is_empty()).collect();
    let t = Instant::now();
    let l = db.lookup(&ids, true).await;
    println!("looked up {} ids in {:?}; fetched {}, error {:?}", ids.len(), t.elapsed(), l.fetched, l.error);
    let (mut known, mut unknown, mut offers) = (0, 0, 0);
    for m in &mods {
        let Some(c) = l.mods.get(&m.mod_id.to_ascii_lowercase()) else { continue };
        let Some(raw) = &c.raw else {
            unknown += 1;
            continue;
        };
        known += 1;
        let info = parse::build_info(&m.mod_id, raw, target.as_deref(), false, c.fetched_ms);
        let s = updates::summarize(&info, m.version.as_deref(), target.as_deref(), false, None, false);
        if let Some(u) = &s.update {
            offers += 1;
            println!("  update: {} {} -> {} ({})", m.mod_id, m.version.as_deref().unwrap_or("?"), u.version, u.message);
        }
    }
    println!("on Mod DB: {known}, not on Mod DB: {unknown}, updates offered: {offers}");

    let t = Instant::now();
    let (cat, _) = db.catalogue(true).await.expect("catalogue");
    println!("catalogue: {} mods in {:?}", cat.len(), t.elapsed());
    println!("tags: {}, game versions: {}", db.tags().await.unwrap().len(), db.game_versions().await.unwrap().len());

    // Download one small release into the temp cache and validate it.
    let (c, err) = db.get_mod("carryon", false).await;
    let info = parse::build_info("carryon", c.unwrap().raw.as_ref().unwrap(), target.as_deref(), false, 0.0);
    let rel = info.latest_compatible().or(info.latest_release()).unwrap().clone();
    let t = Instant::now();
    let (path, temp) = db.fetch_release("carryon", &rel, true, |_| {}, || false).await.expect("download");
    println!(
        "downloaded carryon {} ({} bytes) in {:?} -> cached={} err={err:?}",
        rel.version,
        std::fs::metadata(&path).unwrap().len(),
        t.elapsed(),
        !temp
    );
    let _ = std::fs::remove_dir_all(&tmp);
    println!("OK");
}
