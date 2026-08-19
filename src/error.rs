use crate::domain::{DomainError, ValidationReport};
use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type used by the library and CLI.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read `{path}`: {source}. Check that the file exists and is readable")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Domain(#[from] DomainError),

    #[error("kconfig parse error: {0}")]
    Parse(String),

    #[error("{0}")]
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
