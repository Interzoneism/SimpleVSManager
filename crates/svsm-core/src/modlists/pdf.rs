//! PDF modlists, compatible with Simple VS Manager 2.3.0.
//!
//! The PDF is a readable mod list for people; the data for the app lives in the document info:
//! `Subject` = `SVSM:MODLIST:<base64 modlist JSON>` and `Keywords` = `SVSM:CONFIG:<base64 config
//! list JSON>` or `SVSM:CONFIG:NONE`. Only that metadata is read back (no text extraction).

use std::path::Path;

use base64::Engine;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

use super::format::{parse_config_list, parse_modlist, to_compact_json, ConfigList, Modlist};
use crate::error::{CoreError, CoreResult};

const MODLIST_PREFIX: &str = "SVSM:MODLIST:";
const CONFIG_PREFIX: &str = "SVSM:CONFIG:";
const CONFIG_NONE: &str = "NONE";

/// One line in the printed list.
#[derive(Debug, Clone)]
pub struct PdfModLine {
    pub title: String,
    pub version: Option<String>,
    /// Mod DB page, turned into a link.
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PdfExport<'a> {
    pub modlist: &'a Modlist,
    pub configs: Option<ConfigList>,
    pub lines: Vec<PdfModLine>,
}

/// Reads the modlist embedded in a PDF exported by Simple VS Manager.
pub fn read_pdf(path: &Path) -> CoreResult<Modlist> {
    let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
    read_pdf_bytes(&bytes).map_err(|message| CoreError::Parse { path: path.to_path_buf(), message })
}

pub fn read_pdf_bytes(bytes: &[u8]) -> Result<Modlist, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("The PDF could not be read: {e}"))?;
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        })
        .ok_or("This PDF wasn't made by Simple VS Manager (it has no modlist data).")?;
    let text = |key: &[u8]| info.get(key).ok().and_then(|o| lopdf::decode_text_string(o).ok());

    let subject = text(b"Subject").unwrap_or_default();
    let payload = subject
        .trim()
        .strip_prefix(MODLIST_PREFIX)
        .ok_or("This PDF wasn't made by Simple VS Manager (it has no modlist data).")?;
    let json = payload_to_json(payload).ok_or("The modlist data in this PDF is damaged.")?;
    let mut modlist = parse_modlist(&json)?;

    if let Some(keywords) = text(b"Keywords") {
        if let Some(payload) = keywords.trim().strip_prefix(CONFIG_PREFIX) {
            if payload.trim() != CONFIG_NONE {
                let json = payload_to_json(payload).ok_or("The config data in this PDF is damaged.")?;
                modlist.merge_config_list(parse_config_list(&json)?);
            }
        }
    }
    Ok(modlist)
}

/// 2.x accepted raw JSON or base64 JSON, with whitespace anywhere in the base64.
fn payload_to_json(payload: &str) -> Option<String> {
    let p = payload.replace('\0', "");
    let p = p.trim();
    if p.starts_with('{') || p.starts_with('[') {
        return Some(p.to_string());
    }
    let compact: String = p.chars().filter(|c| !c.is_whitespace()).collect();
    let raw = base64::engine::general_purpose::STANDARD.decode(compact).ok()?;
    String::from_utf8(raw).ok().filter(|s| !s.trim().is_empty())
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

const PAGE_W: f32 = 595.0; // A4 in points
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 40.0;

#[derive(Clone, Copy)]
enum Font {
    Regular,
    Bold,
    Italic,
}

impl Font {
    fn key(self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
            Font::Italic => "F3",
        }
    }
}

/// A link area (x1, y1, x2, y2) and its URL.
type Link = (f32, f32, f32, f32, String);

struct Layout {
    pages: Vec<(Vec<Operation>, Vec<Link>)>,
    y: f32,
}

impl Layout {
    fn new() -> Self {
        Self { pages: vec![(Vec::new(), Vec::new())], y: PAGE_H - MARGIN }
    }

    fn ensure(&mut self, height: f32) {
        if self.y - height < MARGIN {
            self.pages.push((Vec::new(), Vec::new()));
            self.y = PAGE_H - MARGIN;
        }
    }

    /// Writes one paragraph, wrapped to the page width. Returns nothing; advances the cursor.
    fn text(&mut self, text: &str, font: Font, size: f32, color: (f32, f32, f32), url: Option<&str>) {
        let max_chars = ((PAGE_W - 2.0 * MARGIN) / (size * 0.5)).max(10.0) as usize;
        for line in wrap(text, max_chars) {
            let lead = size * 1.3;
            self.ensure(lead);
            self.y -= size;
            let (ops, links) = self.pages.last_mut().expect("at least one page");
            ops.extend([
                Operation::new("BT", vec![]),
                Operation::new("rg", vec![color.0.into(), color.1.into(), color.2.into()]),
                Operation::new("Tf", vec![Object::Name(font.key().as_bytes().to_vec()), size.into()]),
                Operation::new("Td", vec![MARGIN.into(), self.y.into()]),
                Operation::new("Tj", vec![Object::String(win_ansi(&line), lopdf::StringFormat::Literal)]),
                Operation::new("ET", vec![]),
            ]);
            if let Some(url) = url {
                let width = (line.chars().count() as f32 * size * 0.5).min(PAGE_W - 2.0 * MARGIN);
                links.push((MARGIN, self.y - 2.0, MARGIN + width, self.y + size, url.to_string()));
            }
            self.y -= lead - size;
        }
    }

    fn gap(&mut self, h: f32) {
        self.y -= h;
    }
}

fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// The standard PDF fonts use WinAnsi (cp1252). Latin-1 maps 1:1; the rest becomes `?`.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => c as u8,
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '™' => 0x99,
            _ => b'?',
        })
        .collect()
}

/// Writes a modlist PDF readable by both 2.3.0 and this app.
pub fn write_pdf(path: &Path, export: &PdfExport) -> CoreResult<()> {
    let bytes = pdf_bytes(export);
    let tmp = path.with_extension("pdf.tmp");
    crate::fsutil::write_synced(&tmp, &bytes)?;
    crate::fsutil::rename_with_retry(&tmp, path)
}

pub fn pdf_bytes(export: &PdfExport) -> Vec<u8> {
    let list = export.modlist;
    let name = list.display_name().unwrap_or("Installed Mods").to_string();
    let uploader = list.uploader.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Unknown");
    let black = (0.0, 0.0, 0.0);
    let blue = (0.13, 0.45, 0.8);

    let mut l = Layout::new();
    l.text(&name, Font::Bold, 28.0, black, None);
    if let Some(v) = list.version.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        l.text(&format!("Version: {v}"), Font::Italic, 12.0, black, None);
    }
    l.gap(4.0);
    l.text("Generated with Simple VS Manager. Get it from the Vintage Story Mod DB to load this PDF", Font::Regular, 10.0, black, None);
    l.text("as a modlist: https://mods.vintagestory.at/simplevsmanager", Font::Regular, 10.0, blue, Some("https://mods.vintagestory.at/simplevsmanager"));
    l.gap(6.0);
    let game = list.game_version.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or("Unknown");
    l.text(&format!("Made for VS version {game}"), Font::Regular, 14.0, black, None);
    l.text(&format!("Modlist by {uploader}"), Font::Regular, 14.0, black, None);
    if let Some(d) = list.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        l.gap(4.0);
        l.text(d, Font::Italic, 10.0, black, None);
    }
    l.gap(8.0);
    l.text("Mods in this list:", Font::Bold, 12.0, black, None);
    for m in &export.lines {
        let line = match m.version.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => format!("{} {v}", m.title.trim()),
            None => m.title.trim().to_string(),
        };
        let color = if m.url.is_some() { blue } else { black };
        l.text(&line, Font::Regular, 10.0, color, m.url.as_deref());
    }

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = |doc: &mut Document, base: &str| {
        doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base, "Encoding" => "WinAnsiEncoding",
        })
    };
    let f1 = font(&mut doc, "Helvetica");
    let f2 = font(&mut doc, "Helvetica-Bold");
    let f3 = font(&mut doc, "Helvetica-Oblique");
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2, "F3" => f3 } });

    let mut kids = Vec::new();
    for (ops, links) in l.pages {
        let content = Content { operations: ops };
        let stream = Stream::new(dictionary! {}, content.encode().expect("content encodes")).with_compression(true);
        let content_id = doc.add_object(stream);
        let annots: Vec<Object> = links
            .into_iter()
            .map(|(x1, y1, x2, y2, url)| {
                doc.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Link",
                    "Rect" => vec![x1.into(), y1.into(), x2.into(), y2.into()],
                    "Border" => vec![0.into(), 0.into(), 0.into()],
                    "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal(url) },
                })
                .into()
            })
            .collect();
        let mut page = dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
        };
        if !annots.is_empty() {
            page.set("Annots", annots);
        }
        kids.push(Object::from(doc.add_object(page)));
    }
    let count = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);

    let config_value = match export.configs.as_ref().filter(|c| c.configurations.as_ref().is_some_and(|v| !v.is_empty())) {
        Some(c) => format!("{CONFIG_PREFIX}{}", b64(&to_compact_json(c))),
        None => format!("{CONFIG_PREFIX}{CONFIG_NONE}"),
    };
    let info = doc.add_object(dictionary! {
        "Title" => lopdf::text_string(&name),
        "Author" => lopdf::text_string(uploader),
        "Subject" => Object::string_literal(format!("{MODLIST_PREFIX}{}", b64(&to_compact_json(list)))),
        "Keywords" => Object::string_literal(config_value),
        "Creator" => Object::string_literal("Simple VS Manager"),
        "Producer" => Object::string_literal("Simple VS Manager"),
    });
    doc.trailer.set("Info", info);
    doc.compress();

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("writing to a Vec cannot fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modlists::format::{ModlistConfig, ModlistMod};

    #[test]
    fn reads_pdfs_exported_by_2_3_0() {
        let with = read_pdf_bytes(include_bytes!("../../tests/fixtures/modlists/svsm-2.3.0-with-configs.pdf")).unwrap();
        assert_eq!(with.display_name(), Some("Golden list ä"));
        assert_eq!(with.uploader.as_deref(), Some("Tester"));
        assert_eq!(with.game_version.as_deref(), Some("1.21.5"));
        assert_eq!(with.entries().len(), 2);
        assert_eq!(with.entries()[1].is_active, Some(false));
        let configs = with.config_files();
        assert_eq!(configs.len(), 2, "{configs:?}");
        assert!(configs.iter().any(|c| c.mod_id == "carryon" && c.relative_path.as_deref() == Some("CarryOn.json")));
        assert!(configs.iter().any(|c| c.mod_id == "betterruins" && c.content == "{\"a\":1}"));

        let without = read_pdf_bytes(include_bytes!("../../tests/fixtures/modlists/svsm-2.3.0-no-configs.pdf")).unwrap();
        assert_eq!(without.config_files().len(), 1, "only the inline one");
    }

    #[test]
    fn writes_pdfs_we_can_read_back() {
        let list = Modlist {
            name: Some("Ünïcode — list".into()),
            uploader: Some("Me".into()),
            description: Some("Long description ".repeat(30)),
            mods: Some(
                (0..120)
                    .map(|i| ModlistMod { mod_id: Some(format!("mod{i}")), version: Some("1.0.0".into()), is_active: Some(i % 2 == 0), ..Default::default() })
                    .collect(),
            ),
            include_mod_status: Some(true),
            ..Default::default()
        };
        let configs = ConfigList {
            configurations: Some(vec![ModlistConfig {
                mod_id: Some("mod1".into()),
                file_name: Some("m.json".into()),
                relative_path: None,
                content: Some("{}".into()),
            }]),
        };
        let lines = (0..120)
            .map(|i| PdfModLine { title: format!("Mod {i}"), version: Some("1.0.0".into()), url: (i % 3 == 0).then(|| "https://mods.vintagestory.at/x".into()) })
            .collect();
        let bytes = pdf_bytes(&PdfExport { modlist: &list, configs: Some(configs), lines });
        let doc = Document::load_mem(&bytes).unwrap();
        assert!(doc.get_pages().len() >= 3, "long lists span pages");
        let back = read_pdf_bytes(&bytes).unwrap();
        assert_eq!(back.display_name(), Some("Ünïcode — list"));
        assert_eq!(back.entries().len(), 120);
        assert_eq!(back.config_files().len(), 1);
    }

    #[test]
    fn rejects_other_pdfs() {
        assert!(read_pdf_bytes(b"not a pdf").is_err());
        let mut doc = Document::with_version("1.5");
        let info = doc.add_object(dictionary! { "Subject" => Object::string_literal("Holiday photos") });
        doc.trailer.set("Info", info);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert!(read_pdf_bytes(&bytes).unwrap_err().contains("wasn't made by Simple VS Manager"));
    }

    #[test]
    fn payload_forms() {
        assert_eq!(payload_to_json(" {\"a\":1} ").as_deref(), Some("{\"a\":1}"));
        assert_eq!(payload_to_json(&format!("{}\n{}", &b64("{}")[..2], &b64("{}")[2..])).as_deref(), Some("{}"));
        assert!(payload_to_json("!!").is_none());
        assert_eq!(win_ansi("aé•€☃"), vec![b'a', 0xe9, 0x95, 0x80, b'?']);
    }
}
