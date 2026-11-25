//! `modinfo.json` parsing, mirroring the WPF app (which mirrors the game): property names are
//! case-insensitive, comments and trailing commas are allowed.

use serde_json::Value;

use super::model::Dependency;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModInfo {
    pub mod_id: String,
    pub name: String,
    pub manifest_name: Option<String>,
    pub version: Option<String>,
    pub network_version: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub authors: Vec<String>,
    pub contributors: Vec<String>,
    pub dependencies: Vec<Dependency>,
    pub icon_path: Option<String>,
    pub side: Option<String>,
    pub required_on_client: Option<bool>,
    pub required_on_server: Option<bool>,
}

/// Parses modinfo text. `fallback_name` is the folder or zip name, used when the manifest has
/// no name or id.
pub fn parse_modinfo(text: &str, fallback_name: &str) -> Result<ModInfo, String> {
    let text = text.trim_start_matches('\u{feff}');
    let root: Value = serde_json::from_str(text)
        .or_else(|_| json5::from_str(text))
        .map_err(|e| format!("Invalid modinfo.json: {e}"))?;
    if !root.is_object() {
        return Err("Invalid modinfo.json: expected an object".into());
    }

    let manifest_name = get_str(&root, "name").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let fallback_name = fallback_name.trim();
    let name_candidate = manifest_name.clone().unwrap_or_else(|| fallback_name.to_string());

    let mut mod_id = get_str(&root, "modid").unwrap_or_default().to_string();
    if mod_id.trim().is_empty() {
        mod_id = to_mod_id(&name_candidate);
    }

    let version = get_str(&root, "version")
        .map(String::from)
        .or_else(|| version_from_map(&root))
        .filter(|v| !v.trim().is_empty());

    let name = if name_candidate.trim().is_empty() { mod_id.clone() } else { name_candidate };

    Ok(ModInfo {
        mod_id,
        name,
        manifest_name,
        version,
        network_version: get_str(&root, "networkversion").map(String::from),
        description: get_str(&root, "description").map(String::from),
        website: get_str(&root, "website").map(String::from),
        authors: get_str_list(&root, "authors"),
        contributors: get_str_list(&root, "contributors"),
        dependencies: get_dependencies(&root),
        icon_path: get_str(&root, "iconpath").map(String::from),
        side: get_str(&root, "side").map(String::from),
        required_on_client: get(&root, "requiredonclient").and_then(Value::as_bool),
        required_on_server: get(&root, "requiredonserver").and_then(Value::as_bool),
    })
}

/// Derives a mod id from a name the way the game does: lowercase letters and digits only,
/// prefixed with `m` if it would start with a digit.
pub fn to_mod_id(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_alphabetic() {
            out.extend(ch.to_lowercase());
        } else if ch.is_numeric() {
            if out.is_empty() {
                out.push('m');
            }
            out.push(ch);
        }
    }
    if out.is_empty() {
        "mod".into()
    } else {
        out
    }
}

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

fn get_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    get(v, key).and_then(Value::as_str)
}

fn get_str_list(v: &Value, key: &str) -> Vec<String> {
    get(v, key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

fn get_dependencies(v: &Value) -> Vec<Dependency> {
    get(v, "dependencies")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(id, ver)| Dependency {
                    mod_id: id.clone(),
                    version: ver.as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Old manifests can carry a `versionmap` of game version -> mod version. Prefer the 1.21 entry
/// (as the WPF app does), otherwise the last one.
fn version_from_map(v: &Value) -> Option<String> {
    let map = get(v, "versionmap")?.as_object()?;
    let mut preferred = None;
    let mut fallback = None;
    for (game, ver) in map {
        if let Some(ver) = ver.as_str() {
            fallback = Some(ver.to_string());
            if game.contains("1.21") {
                preferred = Some(ver.to_string());
            }
        }
    }
    preferred.or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_manifest() {
        let text = r#"{
            "type": "code",
            "modid": "carryon",
            "name": "Carry On",
            "Version": "1.8.0",
            "authors": ["copygirl", "Nikro"],
            "description": "Pick up chests",
            "dependencies": { "game": "1.21.0", "betterruins": "*" },
            "side": "Universal",
            "requiredOnClient": true,
        }"#;
        let info = parse_modinfo(text, "carryon_1.8.0").unwrap();
        assert_eq!(info.mod_id, "carryon");
        assert_eq!(info.name, "Carry On");
        assert_eq!(info.version.as_deref(), Some("1.8.0"));
        assert_eq!(info.authors, ["copygirl", "Nikro"]);
        assert_eq!(info.dependencies.len(), 2);
        assert!(info.dependencies[0].is_game_or_core());
        assert_eq!(info.required_on_client, Some(true));
        assert_eq!(info.required_on_server, None);
    }

    #[test]
    fn comments_and_missing_id() {
        let text = "// comment\n{ /* x */ \"name\": \"Better Ruins 2\" }";
        let info = parse_modinfo(text, "zipname").unwrap();
        assert_eq!(info.mod_id, "betterruins2");
        assert_eq!(info.version, None);
    }

    #[test]
    fn falls_back_to_file_name() {
        let info = parse_modinfo("{}", "4kTextures").unwrap();
        assert_eq!(info.mod_id, "m4ktextures");
        assert_eq!(info.name, "4kTextures");
    }

    #[test]
    fn version_map() {
        let info = parse_modinfo(r#"{"modid":"x","versionMap":{"1.20.0":"1.0","1.21.0":"2.0","1.22":"3.0"}}"#, "x").unwrap();
        assert_eq!(info.version.as_deref(), Some("2.0"));
    }

    #[test]
    fn invalid() {
        assert!(parse_modinfo("[1,2]", "x").is_err());
        assert!(parse_modinfo("{ nope", "x").is_err());
    }

    #[test]
    fn mod_id_rules() {
        assert_eq!(to_mod_id("Hello World-2"), "helloworld2");
        assert_eq!(to_mod_id("---"), "mod");
        assert_eq!(to_mod_id("123"), "m123");
    }
}
