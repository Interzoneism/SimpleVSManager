//! Core logic for Simple VS Manager.
//!
//! This crate must not depend on Tauri or any UI toolkit. Everything OS-specific lives in
//! [`platform`], and everything that touches the user's machine takes a [`HostEnv`] so tests
//! can point it at a temporary directory.

pub mod backups;
pub mod clientsettings;
pub mod cloud;
pub mod environment;
pub mod error;
mod fsutil;
pub mod game_version;
pub mod host;
pub mod legacy_config;
pub mod moddb;
pub mod modlists;
pub mod mods;
pub mod paths;
pub mod launch;
pub mod platform;
pub mod sessions;
pub mod settings;
pub mod version;

pub use environment::{detect_environment, detect_environment_with, DetectedEnvironment, DetectedPath, PathSource};
pub use settings::Settings;
pub use error::{CoreError, CoreResult};
pub use host::HostEnv;
