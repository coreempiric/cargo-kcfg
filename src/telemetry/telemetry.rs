//! Global telemetry: tracing subscriber and structured log macros.
//!
//! # How to use
//!
//! Construct once in `main` with a message prefix and keep the value alive
//! for the process. Every [`crate::telemetry_info`] / [`crate::telemetry_error`] call
//! then prepends that prefix to the log message:
//!
//! ```
//! use cargo_kcfg::{telemetry_error, telemetry_info, Telemetry, TelemetryError};
//!
//! # fn main() -> Result<(), TelemetryError> {
//! let _telemetry = Telemetry::new("cargo-kcfg")?;
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
use std::sync::OnceLock;
use tracing_subscriber::{EnvFilter, fmt as tracing_fmt, prelude::*};

static PREFIX: OnceLock<String> = OnceLock::new();
static PREFIX_DISPLAY: OnceLock<String> = OnceLock::new();

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
    ///
    /// `prefix` is prepended to every subsequent [`crate::telemetry_info`] and
    /// [`crate::telemetry_error`] message (`"{prefix}: {message}"`). An empty
    /// prefix leaves messages unchanged.
    ///
    /// # Errors
    ///
    /// [`TelemetryError::InitFailed`] if a tracing subscriber is already
    /// installed in this process (including a second call to [`Self::new`]).
    /// The first prefix is kept.
    pub fn new(prefix: impl Into<String>) -> Result<Self, TelemetryError> {
        let prefix = prefix.into();
        let display = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}: ")
        };
        let _ = PREFIX.set(prefix);
        let _ = PREFIX_DISPLAY.set(display);
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

    /// Prefix installed by [`Telemetry::new`], or an empty string before init.
    pub fn prefix() -> &'static str {
        PREFIX.get().map(String::as_str).unwrap_or("")
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        tracing::info!(concat!("{}", "Telemetry shutting down"), prefix_display());
    }
}

/// `"{prefix}: "` or empty. Used by the log macros.
#[doc(hidden)]
pub fn prefix_display() -> &'static str {
    PREFIX_DISPLAY.get().map(String::as_str).unwrap_or("")
}

/// Log an INFO event, prefixed with the string passed to [`Telemetry::new`].
#[macro_export]
macro_rules! telemetry_info {
    (@emit [$($fields:tt)*] $fmt:literal $($rest:tt)*) => {
        tracing::info!(
            $($fields)*
            concat!("{}", $fmt),
            $crate::telemetry::telemetry::prefix_display()
            $($rest)*
        )
    };
    (@accum [$($fields:tt)*] $key:ident = % $value:expr, $($rest:tt)+) => {
        $crate::telemetry_info!(@accum [$($fields)* $key = % $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $key:ident = ? $value:expr, $($rest:tt)+) => {
        $crate::telemetry_info!(@accum [$($fields)* $key = ? $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $key:ident = $value:expr, $($rest:tt)+) => {
        $crate::telemetry_info!(@accum [$($fields)* $key = $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $($rest:tt)+) => {
        $crate::telemetry_info!(@emit [$($fields)*] $($rest)+)
    };
    ($($arg:tt)+) => {
        $crate::telemetry_info!(@accum [] $($arg)+)
    };
}

/// Log an ERROR event, prefixed with the string passed to [`Telemetry::new`].
#[macro_export]
macro_rules! telemetry_error {
    (@emit [$($fields:tt)*] $fmt:literal $($rest:tt)*) => {
        tracing::error!(
            $($fields)*
            concat!("{}", $fmt),
            $crate::telemetry::telemetry::prefix_display()
            $($rest)*
        )
    };
    (@accum [$($fields:tt)*] $key:ident = % $value:expr, $($rest:tt)+) => {
        $crate::telemetry_error!(@accum [$($fields)* $key = % $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $key:ident = ? $value:expr, $($rest:tt)+) => {
        $crate::telemetry_error!(@accum [$($fields)* $key = ? $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $key:ident = $value:expr, $($rest:tt)+) => {
        $crate::telemetry_error!(@accum [$($fields)* $key = $value,] $($rest)+)
    };
    (@accum [$($fields:tt)*] $($rest:tt)+) => {
        $crate::telemetry_error!(@emit [$($fields)*] $($rest)+)
    };
    ($($arg:tt)+) => {
        $crate::telemetry_error!(@accum [] $($arg)+)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_init_succeeds_once() {
        let first = Telemetry::new("cargo-kcfg");
        telemetry_info!("Test info message.");
        telemetry_error!("Test error message.");
        telemetry_info!(code = 7u32, "field plus message");

        assert!(first.is_ok(), "telemetry init should succeed");
        assert_eq!(Telemetry::prefix(), "cargo-kcfg");
        assert_eq!(prefix_display(), "cargo-kcfg: ");

        let second = Telemetry::new("other");
        assert_eq!(second.unwrap_err(), TelemetryError::InitFailed);
        assert_eq!(
            Telemetry::prefix(),
            "cargo-kcfg",
            "the first prefix must stick"
        );
    }

    #[test]
    fn empty_prefix_adds_no_separator() {
        assert_eq!(format!(concat!("{}", "hello"), ""), "hello");
        assert_eq!(
            format!(concat!("{}", "hello"), "cargo-kcfg: "),
            "cargo-kcfg: hello"
        );
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
