//! Lenient JSON getters. The Mod DB mixes strings and numbers for the same field
//! (`statuscode` is `"200"`, ids are numbers), so everything is read through these.

use serde_json::Value;

/// A non-blank string, or a number rendered as a string.
pub fn string(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) => Some(s.trim()).filter(|s| !s.is_empty()).map(String::from),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// An integer from a number or a numeric string; floats are truncated.
pub fn int(v: &Value, key: &str) -> Option<i64> {
    match v.get(key)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A list of non-blank strings (non-string items are skipped).
pub fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The API's own status code, which is a string inside a 200 response.
pub fn status_code(v: &Value) -> Option<i64> {
    int(v, "statuscode")
}
