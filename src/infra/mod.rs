//! Infrastructure: file loading, parser adaptation, and code generation.
//!
//! Depends on the domain layer; the domain does not depend on this module.

pub mod codegen;
pub mod defconfig;
pub mod discover;
pub mod kconfig;
pub mod pipeline;
