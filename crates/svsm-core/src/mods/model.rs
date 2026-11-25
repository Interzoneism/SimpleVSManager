use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ModSourceKind {
    Folder,
    Zip,
    /// Loose `.cs` file: can't be managed without a modinfo.json.
    SourceCode,
    /// Loose `.dll` file: can't be managed without a modinfo.json.
    Assembly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub mod_id: String,
    /// Minimum version, `*` or empty for any.
    pub version: String,
}

impl Dependency {
    /// `game`, `creative` and `survival` are provided by the game itself.
    pub fn is_game_or_core(&self) -> bool {
        ["game", "creative", "survival"].iter().any(|c| c.eq_ignore_ascii_case(&self.mod_id))
    }
}

/// One installed mod as shown in the list. Port of `ModEntry.cs`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    /// Stable key for the UI and for commands: the full source path.
    pub key: String,
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
    pub side: Option<String>,
    pub required_on_client: Option<bool>,
    pub required_on_server: Option<bool>,
    pub source_path: String,
    pub source_kind: ModSourceKind,
    /// File name (zip / cs / dll) or folder name.
    pub file_name: String,
    /// Size in bytes (zip and loose files; folders are not summed).
    pub size_bytes: Option<f64>,
    /// Last modified, milliseconds since the Unix epoch.
    pub modified_ms: Option<f64>,
    /// Key for `svsm://localhost/icon/<key>`, when the mod ships an icon.
    pub icon_key: Option<String>,
    /// Problem reading the mod itself (bad zip, missing modinfo.json, ...).
    pub error: Option<String>,
    /// Problem the game will hit when loading (duplicate, missing/broken dependency).
    pub load_error: Option<String>,
    pub missing_dependencies: Vec<Dependency>,
    pub dependency_has_errors: bool,
    /// Other installed copies of the same mod id (source paths).
    pub duplicates: Vec<String>,
    /// From clientsettings.json `disabledMods`.
    pub is_active: bool,
}

impl InstalledMod {
    pub fn has_errors(&self) -> bool {
        self.error.as_deref().is_some_and(|e| !e.trim().is_empty())
    }

    pub fn has_load_error(&self) -> bool {
        self.load_error.as_deref().is_some_and(|e| !e.trim().is_empty())
    }
}
