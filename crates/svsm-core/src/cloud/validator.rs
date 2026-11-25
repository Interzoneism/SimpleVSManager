//! Blocked words for public modlist text (name, description, uploader). Port of
//! `OnlineModlistContentValidator`: whole-word, case-insensitive, phrases match any whitespace.

use std::sync::OnceLock;

const WORDS_XML: &str = include_str!("swearWords.xml");

fn terms() -> &'static Vec<Vec<String>> {
    static TERMS: OnceLock<Vec<Vec<String>>> = OnceLock::new();
    TERMS.get_or_init(|| {
        let mut out: Vec<Vec<String>> = Vec::new();
        for part in WORDS_XML.split("<word>").skip(1) {
            let Some(word) = part.split("</word>").next() else { continue };
            let parts: Vec<String> = word.split_whitespace().map(str::to_lowercase).collect();
            if !parts.is_empty() && !out.contains(&parts) {
                out.push(parts);
            }
        }
        out
    })
}

/// Splits text into lower-case words (letters and digits); everything else separates words.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// The first blocked word or phrase found in any of the texts.
pub fn find_blocked<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
    for text in texts {
        let w = words(text);
        for term in terms() {
            if w.windows(term.len()).any(|win| win.iter().zip(term).all(|(a, b)| a == b)) {
                return Some(term.join(" "));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_words_only() {
        assert!(terms().len() > 10);
        assert_eq!(find_blocked(["My ASS list"]).as_deref(), Some("ass"));
        assert_eq!(find_blocked(["Classic assets", "glass"]), None, "inside other words is fine");
        assert_eq!(find_blocked(["nice", "fine"]), None);
    }
}
