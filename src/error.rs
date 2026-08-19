use crate::domain::{DomainError, ValidationReport};
use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type used by the library and CLI.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Domain(#[from] DomainError),

    #[error("kconfig parse error:\n{0}")]
    Parse(String),

    #[error("validation failed:\n{0}")]
    Validation(ValidationReport),

    #[error("{0}")]
    Usage(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
