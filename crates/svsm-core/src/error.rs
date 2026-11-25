use std::path::PathBuf;

/// Errors produced by the core library.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Could not parse {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("Not a Windows PE file or it has no version resource: {0}")]
    NoVersionInfo(PathBuf),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
