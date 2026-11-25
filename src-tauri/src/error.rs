use serde::Serialize;

/// Error returned to the frontend. Kept as a plain message for now; add a `kind` field when the
/// UI needs to branch on error types.
#[derive(Debug, thiserror::Error, Serialize, specta::Type)]
#[error("{message}")]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl From<svsm_core::CoreError> for AppError {
    fn from(e: svsm_core::CoreError) -> Self {
        Self::msg(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
