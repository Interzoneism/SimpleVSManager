//! Prints the version resource of PE files: `cargo run -p svsm-core --example pe_version -- <file>...`
fn main() {
    for arg in std::env::args().skip(1) {
        let path = std::path::Path::new(&arg);
        match svsm_core::game_version::read_pe_version(path) {
            Ok(v) => println!("{arg}: {v}"),
            Err(e) => println!("{arg}: {e}"),
        }
    }
}
