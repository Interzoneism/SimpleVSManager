use serde::Serialize;

/// Installed mods or clientsettings.json changed on disk; the UI should refetch the mod list.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct ModsChanged;

/// Progress of one mod during `update_mods` / `install_mod`. `key` is the installed mod key,
/// or `install:<mod id>` for fresh installs.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub key: String,
    pub stage: crate::moddb_commands::UpdateStage,
    pub percent: Option<f64>,
    pub message: Option<String>,
}

/// Progress of `apply_modlist`: step `step` of `total`, with a short label.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ModlistProgress {
    pub step: u32,
    pub total: u32,
    pub label: String,
}

/// Progress of a data folder backup or restore.
#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct BackupProgress {
    pub percent: f64,
    pub label: String,
}
