#![doc = include_str!("../README.md")]

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
