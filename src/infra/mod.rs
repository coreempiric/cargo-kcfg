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
//! Crate `build.rs` files enter at [`build_script::BuildScript`].

pub mod build_script;
pub mod codegen;
pub mod defconfig;
pub mod kconfig;
pub mod locator;
pub mod pipeline;
