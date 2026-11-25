//! Times a mod scan: `cargo run --release -p svsm-core --example scan -- <VintagestoryData>`
fn main() {
    let data = std::env::args().nth(1).expect("usage: scan <data dir>");
    let settings = svsm_core::clientsettings::ClientSettings::load(std::path::Path::new(&data)).unwrap();
    let start = std::time::Instant::now();
    let r = svsm_core::mods::discover_mods(&settings, None);
    let problems = r.mods.iter().filter(|m| m.error.is_some() || m.load_error.is_some()).count();
    println!("{} mods, {} icons, {} with problems in {:?}", r.mods.len(), r.icons.len(), problems, start.elapsed());
}
