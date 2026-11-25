//! Turns the Mod DB's HTML changelogs into plain text with bullets, like the 2.x app did.
//! A tiny tag scanner is enough: the input is short, trusted-ish markup from a WYSIWYG editor.

const BULLETS: [&str; 4] = ["\u{2022} ", "\u{25E6} ", "\u{25AA} ", "\u{25AB} "];

pub fn html_to_text(html: &str) -> Option<String> {
    let html = html.trim();
    if html.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(html.len());
    let mut depth: usize = 0;
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("<!--") {
            rest = stripped.find("-->").map(|i| &stripped[i + 3..]).unwrap_or("");
            continue;
        }
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else {
                append_text(&mut out, &decode_entities(rest));
                break;
            };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            let closing = tag.starts_with('/');
            let name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            match (name.as_str(), closing) {
                ("br", _) => ensure_newlines(&mut out, 1),
                ("p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "blockquote", false)
                    if depth == 0 && !out.is_empty() =>
                {
                    ensure_newlines(&mut out, 2)
                }
                ("p" | "div" | "section" | "article" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "blockquote", true) => {
                    ensure_newlines(&mut out, 2)
                }
                ("ul" | "ol", false) => {
                    ensure_newlines(&mut out, 1);
                    depth += 1;
                }
                ("ul" | "ol", true) => {
                    depth = depth.saturating_sub(1);
                    ensure_newlines(&mut out, 1);
                }
                ("li", false) => {
                    ensure_newlines(&mut out, 1);
                    let level = depth.max(1);
                    out.push_str(&" ".repeat((level - 1) * 2));
                    out.push_str(BULLETS[(level - 1).min(BULLETS.len() - 1)]);
                }
                ("script" | "style", false) => {
                    let close = format!("</{name}");
                    let lower = rest.to_ascii_lowercase();
                    rest = lower.find(&close).map(|i| &rest[i..]).unwrap_or("");
                }
                _ => {}
            }
            continue;
        }
        let next = rest.find('<').unwrap_or(rest.len());
        append_text(&mut out, &decode_entities(&rest[..next]));
        rest = &rest[next..];
    }
    normalize_lines(&out)
}

fn append_text(out: &mut String, text: &str) {
    // Source newlines are formatting only; collapse whitespace runs to single spaces.
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        pending_space = false;
        out.push(if c == '\u{a0}' { ' ' } else { c });
    }
    if pending_space && !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }
}

fn ensure_newlines(out: &mut String, required: usize) {
    while out.ends_with(' ') {
        out.pop();
    }
    let have = out.chars().rev().take_while(|&c| c == '\n').count();
    for _ in have..required {
        out.push('\n');
    }
}

fn normalize_lines(text: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
            continue;
        }
        let start = line.trim_start();
        if let Some(b) = BULLETS.iter().find(|b| start.starts_with(*b)) {
            let indent = &line[..line.len() - start.len()];
            lines.push(format!("{indent}{b}{}", start[b.len()..].trim()));
        } else {
            lines.push(start.to_string());
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace()).map(|e| e + 1);
        let decoded = match end {
            Some(e) if rest.as_bytes().get(e) == Some(&b';') => decode_entity(&rest[1..e]).map(|c| (c, e + 1)),
            _ => None,
        };
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "hellip" => '\u{2026}',
        "rsquo" => '\u{2019}',
        "lsquo" => '\u{2018}',
        "rdquo" => '\u{201d}',
        "ldquo" => '\u{201c}',
        "copy" => '\u{a9}',
        "rarr" => '\u{2192}',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_and_lists() {
        let html = "<p><strong>New</strong> stuff</p>\n<ul>\n<li>One &amp; two</li>\n<li>Three<ul><li>Nested</li></ul></li>\n</ul><p>Bye<br>now</p>";
        assert_eq!(
            html_to_text(html).unwrap(),
            "New stuff\n\n\u{2022} One & two\n\u{2022} Three\n  \u{25E6} Nested\n\nBye\nnow"
        );
    }

    #[test]
    fn empty_and_plain() {
        assert_eq!(html_to_text("  "), None);
        assert_eq!(html_to_text("<p></p>"), None);
        assert_eq!(html_to_text("Fixed a crash"), Some("Fixed a crash".into()));
        assert_eq!(decode_entities("a &lt;b&gt; &#39;c&#x27; & d &bogus;"), "a <b> 'c' & d &bogus;");
    }
}
