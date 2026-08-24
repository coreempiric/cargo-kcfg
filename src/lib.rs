//! cargo-kconfig: Linux/Zephyr-style Kconfig for Rust crates.
//!
//! # Navigation
//!
//! Binary: [`cli::Cli::run`] → [`cli::Cli::execute`] → [`cli::Commands::execute`]
//! → [`cli::CheckCommand`] / [`cli::BuildCommand`] / [`cli::TestCommand`]
//! → [`infra::locator::ProjectLocator`] → [`infra::pipeline::Pipeline`].
//!
//! Constants: `cargo_kconfig_macros::include_config!` (no consumer `build.rs`).
//! Optional `build.rs`: [`BuildScript`] when this crate needs `#[cfg(CONFIG_*)]`.
//! Unit tests: [`ConfigTest`].
//!
//! The domain layer (`domain`) is independent of the filesystem and of Cargo.
//! Infrastructure (`infra`) loads Kconfig/`*_defconfig` files, evaluates them,
//! and generates `.config`, `config.rs`, and `rustc-cfg` flags.

pub mod cli;
pub mod domain;
pub mod error;
pub mod infra;
pub mod telemetry;

pub use domain::{
    Assignment, AssignmentSet, ChoiceGroup, DependencyGraph, EvaluatedConfig, Evaluator,
    Expression, Limits, Symbol, SymbolTable, SymbolType, Tristate, ValidationReport, Value,
    evaluate,
};
pub use error::Error;
pub use infra::build_script::{BuildScript, run_build_script};
pub use infra::codegen::{CodeGenerator, Generated};
pub use infra::config_test::ConfigTest;
pub use infra::locator::{
    ProjectLocator, find_unique_defconfig, resolve_defconfig, resolve_kconfig,
};
pub use infra::pipeline::{
    ArtifactWriter, GenerateRequest, GenerateResult, Pipeline, run, write_outputs,
};
pub use telemetry::telemetry::{Telemetry, TelemetryError};
