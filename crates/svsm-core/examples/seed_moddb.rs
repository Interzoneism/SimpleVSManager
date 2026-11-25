//! Dev helper: fills a manager folder's Mod DB cache from saved API responses, so the UI can be
//! tried offline. Usage: seed_moddb <manager dir> <catalogue.json> [mod_<id>.json ...]
use svsm_core::moddb::{cache::ModDbCache, dates, parse};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let manager = std::path::Path::new(&args[0]);
    let cache = ModDbCache::new(manager);
    let now = dates::now_ms();
    let list = parse::list_from_response(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let entries: Vec<_> = list.iter().filter_map(parse::list_entry).collect();
    cache.store("catalogue", &entries, now).unwrap();
    println!("catalogue: {} mods", entries.len());
    for f in &args[2..] {
        let text = std::fs::read_to_string(f).unwrap();
        if f.ends_with("gameversions.json") {
            cache.store("gameversions", &parse::tags_from_response(&text, "gameversions").unwrap(), now).unwrap();
            continue;
        }
        if f.ends_with("tags.json") {
            cache.store("tags", &parse::tags_from_response(&text, "tags").unwrap(), now).unwrap();
            continue;
        }
        let raw = parse::mod_from_response(&text).unwrap().expect("a mod");
        // mod_<id>.json -> <id>
        let stem = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
        let id = stem.trim_start_matches("mod_").to_string();
        cache.store_mod(&id, Some(&raw), now).unwrap();
        println!("mod: {id}");
    }
}
