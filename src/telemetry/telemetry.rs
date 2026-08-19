//! Global telemetry: tracing subscriber and structured log macros.
//!
//! # How to use
//!
//! Construct once in `main` and keep the value alive for the process:
//!
//! ```
//! use cargo_kconfig::{telemetry_error, telemetry_info, Telemetry, TelemetryError};
//!
//! # fn main() -> Result<(), TelemetryError> {
//! let _telemetry = Telemetry::new()?;
//! telemetry_info!("telemetry ready");
//! telemetry_error!(code = 1u32, "failure");
//! # Ok(())
//! # }
//! ```
//!
//! Console output uses ANSI colors (ERROR red, INFO green). There is no log
//! file. Level defaults to INFO; override with `RUST_LOG`. Second init fails
//! closed.

use std::fmt;
use tracing_subscriber::{EnvFilter, fmt as tracing_fmt, prelude::*};

/// Telemetry initialisation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryError {
    /// Subscriber install failed.
    InitFailed = 1,
}

impl TelemetryError {
    /// Stable numeric code for diagnostics.
    pub fn code(&self) -> u32 {
        *self as u32
    }
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TelemetryError::InitFailed => write!(f, "telemetry init failed"),
        }
    }
}

/// Process-wide tracing subscriber owner (keep alive for the process lifetime).
#[derive(Debug)]
pub struct Telemetry;

impl Telemetry {
    /// Installs process-wide tracing on stderr (once per process).
    pub fn new() -> Result<Self, TelemetryError> {
        tracing_subscriber::registry()
            .with(
                tracing_fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(true)
                    .with_target(false)
                    .without_time(),
            )
            .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
            .try_init()
            .map_err(|_| TelemetryError::InitFailed)?;

        Ok(Self)
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        tracing::info!("Telemetry shutting down");
    }
}

/// Logs an INFO event through the global tracing subscriber.
#[macro_export]
macro_rules! telemetry_info {
    ($($arg:tt)+) => { tracing::info!($($arg)+); };
}

/// Logs an ERROR event through the global tracing subscriber.
#[macro_export]
macro_rules! telemetry_error {
    ($($arg:tt)+) => { tracing::error!($($arg)+); };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_init_succeeds_once() {
        let first = Telemetry::new();
        telemetry_info!("Test info message.");
        telemetry_error!("Test error message.");

        assert!(first.is_ok(), "telemetry init should succeed");

        let second = Telemetry::new();
        assert_eq!(second.unwrap_err(), TelemetryError::InitFailed);
    }

    #[test]
    fn telemetry_error_has_numeric_code() {
        assert_eq!(TelemetryError::InitFailed.code(), 1);
    }

    #[test]
    fn telemetry_error_display() {
        let err = TelemetryError::InitFailed;
        assert_eq!(err.to_string(), "telemetry init failed");
    }
}
