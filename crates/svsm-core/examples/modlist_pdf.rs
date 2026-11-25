//! Writes a modlist JSON file as a PDF (same format as the app's "Export PDF").
//! `cargo run -p svsm-core --example modlist_pdf -- <modlist.json> <out.pdf>`

use svsm_core::modlists::pdf::{write_pdf, PdfExport, PdfModLine};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, input, output] = args.as_slice() else {
        eprintln!("usage: modlist_pdf <modlist.json> <out.pdf>");
        std::process::exit(2);
    };
    let list = svsm_core::modlists::read_modlist_file(input.as_ref()).expect("read modlist");
    let lines = list
        .entries()
        .iter()
        .map(|m| PdfModLine { title: m.id().unwrap_or_default().to_string(), version: m.version().map(String::from), url: None })
        .collect();
    write_pdf(output.as_ref(), &PdfExport { modlist: &list, configs: None, lines }).expect("write pdf");
    println!("wrote {output}");
}
