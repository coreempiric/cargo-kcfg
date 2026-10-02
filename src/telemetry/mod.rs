//! Process-wide telemetry and structured logging macros.
//!
//! Install with [`telemetry::Telemetry::new`] and a message prefix. Emit
//! [`crate::telemetry_info`] and [`crate::telemetry_error`] only.

#[allow(clippy::module_inception)]
pub mod telemetry;
