#![cfg_attr(not(feature = "full"), no_std)]
#![doc = include_str!("../README.md")]

/// Include the `CONFIG_*` constants written by `run_build_script`.
///
/// The calling crate's `build.rs` must call `cargo_kcfg::run_build_script`,
/// which writes `config.rs` into `OUT_DIR`. Invoke the macro inside the module
/// that should own the constants:
///
/// ```rust,ignore
/// pub mod config {
///     cargo_kcfg::include_config!();
/// }
/// ```
#[macro_export]
macro_rules! include_config {
    () => {
        include!(concat!(env!("OUT_DIR"), "/config.rs"));
    };
}

#[cfg(feature = "full")]
pub mod cli;
#[cfg(feature = "full")]
pub mod domain;
#[cfg(feature = "full")]
pub mod error;
#[cfg(feature = "full")]
pub mod infra;
#[cfg(feature = "full")]
pub mod telemetry;

#[cfg(feature = "full")]
pub use domain::{
    Assignment, AssignmentSet, ChoiceGroup, DependencyGraph, EvaluatedConfig, Evaluator,
    Expression, Limits, Symbol, SymbolTable, SymbolType, Tristate, ValidationReport, Value,
    evaluate,
};
#[cfg(feature = "full")]
pub use error::Error;
#[cfg(feature = "full")]
pub use infra::build_script::{BuildScript, run_build_script};
#[cfg(feature = "full")]
pub use infra::codegen::{CodeGenerator, Generated};
#[cfg(feature = "full")]
pub use infra::config_test::ConfigTest;
#[cfg(feature = "full")]
pub use infra::locator::{
    ProjectLocator, find_unique_defconfig, resolve_defconfig, resolve_kconfig,
};
#[cfg(feature = "full")]
pub use infra::pipeline::{
    ArtifactWriter, GenerateRequest, GenerateResult, Pipeline, run, write_outputs,
};
#[cfg(feature = "full")]
pub use telemetry::telemetry::{Telemetry, TelemetryError};
