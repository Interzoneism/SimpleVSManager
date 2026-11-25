//! Version string helpers, ported from `VersionStringUtility.cs` in the WPF app.
//!
//! Vintage Story and its mods use loose versions ("1.21.5", "v1.2.0-rc.3", "1.19"), so these
//! are deliberately not semver. Behaviour must match the C# code; the tests pin it down.

/// Extracts up to four numeric parts separated by non-digits: `"v1.21.5-rc.2"` -> `"1.21.5.2"`.
/// Stops at the first whitespace after a digit. Returns `None` if there are no digits.
pub fn normalize(version: &str) -> Option<String> {
    const MAX_PARTS: usize = 4;
    if version.trim().is_empty() {
        return None;
    }

    let mut parts: Vec<String> = Vec::with_capacity(MAX_PARTS);
    let mut current = String::new();
    let mut has_digits = false;

    for c in version.chars() {
        if c.is_ascii_digit() {
            current.push(c);
            has_digits = true;
            continue;
        }
        if !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            if parts.len() >= MAX_PARTS {
                break;
            }
        }
        if c.is_whitespace() && has_digits {
            break;
        }
    }
    if !current.is_empty() && parts.len() < MAX_PARTS {
        parts.push(current);
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

/// Parses a normalized version into integer parts. `None` if any part overflows an `i32`
/// (the C# code used `int.TryParse`).
fn parse_parts(normalized: &str) -> Option<Vec<i32>> {
    normalized.split('.').map(|p| p.parse::<i32>().ok()).collect()
}

fn part(parts: &[i32], i: usize) -> i32 {
    parts.get(i).copied().unwrap_or(0)
}

/// True if `candidate` is newer than `current`.
pub fn is_candidate_newer(candidate: Option<&str>, current: Option<&str>) -> bool {
    let Some(candidate) = candidate.filter(|c| !c.trim().is_empty()) else {
        return false;
    };
    let norm_candidate = normalize(candidate);
    let norm_current = current.and_then(normalize);

    let Some(norm_candidate) = norm_candidate else {
        return match current {
            Some(cur) if !cur.trim().is_empty() => !candidate.trim().eq_ignore_ascii_case(cur.trim()),
            _ => false,
        };
    };
    let Some(norm_current) = norm_current else {
        return true;
    };

    match (parse_parts(&norm_candidate), parse_parts(&norm_current)) {
        (Some(a), Some(b)) => {
            for i in 0..a.len().max(b.len()) {
                match part(&a, i).cmp(&part(&b, i)) {
                    std::cmp::Ordering::Greater => return true,
                    std::cmp::Ordering::Less => return false,
                    std::cmp::Ordering::Equal => {}
                }
            }
            false
        }
        _ => !norm_candidate.eq_ignore_ascii_case(&norm_current),
    }
}

/// True if `candidate` equals `target` or is a numeric prefix of it ("1.21" matches "1.21.5").
pub fn matches_version_or_prefix(candidate: &str, target: &str) -> bool {
    let (Some(c), Some(t)) = (normalize(candidate), normalize(target)) else {
        return false;
    };
    if c.eq_ignore_ascii_case(&t) {
        return true;
    }
    let (Some(cp), Some(tp)) = (parse_parts(&c), parse_parts(&t)) else {
        return false;
    };
    if cp.is_empty() || cp.len() > tp.len() {
        return false;
    }
    cp.iter().zip(tp.iter()).all(|(a, b)| a == b)
}

/// True if `provided` is at least `requested`. Empty, `*` or digit-less requests match anything.
pub fn satisfies_minimum_version(requested: Option<&str>, provided: Option<&str>) -> bool {
    let requested = match requested {
        None => return true,
        Some(r) if r.trim().is_empty() || r.trim() == "*" => return true,
        Some(r) => r,
    };
    let Some(norm_requested) = normalize(requested) else {
        return true;
    };
    let Some(norm_provided) = provided.and_then(normalize) else {
        return false;
    };
    let Some(rp) = parse_parts(&norm_requested) else {
        return true;
    };
    let Some(pp) = parse_parts(&norm_provided) else {
        return false;
    };
    for i in 0..rp.len().max(pp.len()) {
        match part(&pp, i).cmp(&part(&rp, i)) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    true
}

/// True if both versions have at least three parts and the first three are equal.
pub fn matches_first_three_digits(a: &str, b: &str) -> bool {
    let (Some(a), Some(b)) = (normalize(a), normalize(b)) else {
        return false;
    };
    let (Some(ap), Some(bp)) = (parse_parts(&a), parse_parts(&b)) else {
        return false;
    };
    ap.len() >= 3 && bp.len() >= 3 && ap[..3] == bp[..3]
}

/// Whether a mod's version tag supports `target`. Exact mode allows equal or prefix matches;
/// relaxed mode also accepts the same major.minor ("1.21.0" supports "1.21.5").
pub fn supports_version(candidate_tag: &str, target: &str, require_exact_match: bool) -> bool {
    let tag = candidate_tag.trim();
    let target = target.trim();
    if tag.is_empty() || target.is_empty() {
        return false;
    }
    if tag.eq_ignore_ascii_case(target) || matches_version_or_prefix(tag, target) {
        return true;
    }
    if require_exact_match {
        return false;
    }
    let (Some(t), Some(g)) = (normalize(tag), normalize(target)) else {
        return false;
    };
    let (Some(tp), Some(gp)) = (parse_parts(&t), parse_parts(&g)) else {
        return false;
    };
    tp.len() >= 2 && gp.len() >= 2 && tp[0] == gp[0] && tp[1] == gp[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cases() {
        assert_eq!(normalize("1.21.5"), Some("1.21.5".into()));
        assert_eq!(normalize("v1.2.0-rc.3"), Some("1.2.0.3".into()));
        assert_eq!(normalize("1.2.3.4.5"), Some("1.2.3.4".into()));
        assert_eq!(normalize("1.20.0 (stable)"), Some("1.20.0".into()));
        assert_eq!(normalize("  version 1.2"), Some("1.2".into()));
        assert_eq!(normalize("abc"), None);
        assert_eq!(normalize("   "), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn newer() {
        assert!(is_candidate_newer(Some("1.2.1"), Some("1.2.0")));
        assert!(is_candidate_newer(Some("1.10"), Some("1.9.9")));
        assert!(!is_candidate_newer(Some("1.2"), Some("1.2.0")));
        assert!(!is_candidate_newer(Some("1.2.0"), Some("1.2.1")));
        assert!(is_candidate_newer(Some("1.0"), None));
        assert!(is_candidate_newer(Some("1.0"), Some("unknown")));
        assert!(!is_candidate_newer(None, Some("1.0")));
        assert!(is_candidate_newer(Some("beta"), Some("alpha")));
        assert!(!is_candidate_newer(Some("Beta"), Some("beta")));
        assert!(!is_candidate_newer(Some("beta"), None));
    }

    #[test]
    fn prefix() {
        assert!(matches_version_or_prefix("1.21", "1.21.5"));
        assert!(matches_version_or_prefix("1.21.5", "1.21.5"));
        assert!(!matches_version_or_prefix("1.21.0", "1.21.5"));
        assert!(!matches_version_or_prefix("1.21.5.1", "1.21.5"));
    }

    #[test]
    fn minimum() {
        assert!(satisfies_minimum_version(Some("*"), None));
        assert!(satisfies_minimum_version(None, Some("1.0")));
        assert!(satisfies_minimum_version(Some("1.2.0"), Some("1.2")));
        assert!(satisfies_minimum_version(Some("1.2.0"), Some("1.3")));
        assert!(!satisfies_minimum_version(Some("1.2.1"), Some("1.2")));
        assert!(!satisfies_minimum_version(Some("1.0"), None));
        assert!(satisfies_minimum_version(Some("any"), None));
    }

    #[test]
    fn first_three() {
        assert!(matches_first_three_digits("1.21.5.3", "1.21.5"));
        assert!(!matches_first_three_digits("1.21", "1.21"));
        assert!(!matches_first_three_digits("1.21.4", "1.21.5"));
    }

    #[test]
    fn supports() {
        assert!(supports_version("1.21.5", "1.21.5", true));
        assert!(supports_version("1.21", "1.21.5", true));
        assert!(!supports_version("1.21.0", "1.21.5", true));
        assert!(supports_version("1.21.0", "1.21.5", false));
        assert!(!supports_version("1.20.0", "1.21.5", false));
        assert!(supports_version("v1.21.5", "1.21.5", true));
    }
}
