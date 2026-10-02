//! Infrastructure: file loading, parser adaptation, and code generation.
//!
//! Depends on the domain layer; the domain does not depend on this module.
//!
//! After the CLI (or a `build.rs`) has a project root:
//! [`locator::ProjectLocator`] → [`pipeline::Pipeline`]
//!   → [`kconfig::KconfigLoader`] + [`defconfig::DefconfigLoader`]
//!   → [`crate::domain::Evaluator`]
//!   → [`codegen::CodeGenerator`]
//!   → [`pipeline::ArtifactWriter`]
//!
//! Constants: [`crate::include_config`] after [`build_script::run_build_script`].
//! The same build script emits `cargo:rustc-cfg` for `#[cfg(CONFIG_*)]`.
//! Unit tests that need a good or bad defconfig enter at [`config_test::ConfigTest`].

pub mod build_script;
pub mod codegen;
pub mod config_test;
pub mod defconfig;
pub mod kconfig;
pub mod locator;
pub mod pipeline;
