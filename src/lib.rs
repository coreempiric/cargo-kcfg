//! cargo-kconfig: Linux/Zephyr-style Kconfig for Rust crates.
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
    Assignment, AssignmentSet, ChoiceGroup, DependencyGraph, EvaluatedConfig, Expression, Limits,
    Symbol, SymbolTable, SymbolType, Tristate, ValidationReport, Value, evaluate,
};
pub use error::Error;
pub use infra::codegen::Generated;
pub use infra::discover::{find_unique_defconfig, resolve_defconfig, resolve_kconfig};
pub use infra::pipeline::{GenerateRequest, GenerateResult, run, run_build_script, write_outputs};
pub use telemetry::telemetry::{Telemetry, TelemetryError};
