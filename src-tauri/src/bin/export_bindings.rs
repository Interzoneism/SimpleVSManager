//! Regenerates `ui/src/bindings.ts` without opening a window: `cargo run --bin export-bindings`.
fn main() {
    let path = svsm_app::bindings_path();
    svsm_app::specta_builder()
        .export(svsm_app::typescript_exporter(), &path)
        .expect("failed to export TypeScript bindings");
    println!("Wrote {}", path.display());
}
