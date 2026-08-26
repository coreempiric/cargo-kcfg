//! Top-level error type for the library, CLI, and optional `build.rs` helper.

use crate::domain::{DomainError, ValidationReport};
use std::path::PathBuf;
use thiserror::Error;

/// Failure from loading, evaluating, or writing a Kconfig configuration.
///
/// Library code never panics on user input; it returns this type (or
/// [`TelemetryError`](crate::TelemetryError) from telemetry init). The
/// `Display` text names the problem and the change that fixes it.
///
/// # Variants
///
/// - [`Error::Io`] — filesystem read/write of Kconfig, defconfig, or output.
/// - [`Error::Parse`] — `nom-kconfig` could not parse a definition file.
/// - [`Error::Validation`] — the assignment set is incoherent; inspect the
///   inner [`ValidationReport`].
/// - [`Error::Domain`] — [`Limits`](crate::Limits) or symbol-table rules.
/// - [`Error::Usage`] — missing/ambiguous files, missing Cargo env vars, or
///   `rustc` failed while type-checking generated constants.
#[derive(Debug, Error)]
pub enum Error {
    /// A required file could not be read or written.
    #[error("failed to read `{path}`: {source}. Check that the file exists and is readable")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A domain / resource-limit failure.
    #[error("{0}")]
    Domain(#[from] DomainError),

    /// The Kconfig definition file is not valid syntax.
    #[error("kconfig parse error: {0}")]
    Parse(String),

    /// Evaluation produced an incoherent configuration.
    #[error("{0}")]
    Validation(ValidationReport),

    /// The invocation is missing files, env vars, or tools.
    #[error("{0}")]
    Usage(String),
}

impl Error {
    /// Wrap a filesystem error with the path that failed.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
