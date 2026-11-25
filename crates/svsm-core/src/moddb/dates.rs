//! Mod DB timestamps look like `"2026-10-14 14:48:50"` with no zone; they are treated as UTC.

/// Parses `YYYY-MM-DD[ T]HH:MM[:SS]` (optionally with a trailing `Z`) into Unix milliseconds.
pub fn parse_utc_ms(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches('Z');
    let (date, time) = match t.split_once([' ', 'T']) {
        Some((d, tm)) => (d, Some(tm)),
        None => (t, None),
    };
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    if d.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hh, mut mm, mut ss) = (0i64, 0i64, 0f64);
    if let Some(time) = time {
        let time = time.split(['+']).next().unwrap_or(time);
        let mut p = time.split(':');
        hh = p.next()?.parse().ok()?;
        mm = p.next().map(|x| x.parse().ok()).unwrap_or(Some(0))?;
        ss = p.next().map(|x| x.parse().ok()).unwrap_or(Some(0.0))?;
    }
    let days = days_from_civil(y, m, day);
    Some(((days * 86_400 + hh * 3_600 + mm * 60) as f64 + ss) * 1000.0)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mod_db_dates() {
        assert_eq!(parse_utc_ms("1970-01-01 00:00:00"), Some(0.0));
        assert_eq!(parse_utc_ms("2026-10-14 14:48:50"), Some(1_791_989_330_000.0));
        assert_eq!(parse_utc_ms("2026-10-14T14:48:50Z"), Some(1_791_989_330_000.0));
        assert_eq!(parse_utc_ms("2000-02-29"), Some(951_782_400_000.0));
        assert_eq!(parse_utc_ms("nope"), None);
        assert_eq!(parse_utc_ms("2026-13-01"), None);
    }
}
