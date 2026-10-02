//! Core Kconfig domain: symbols, expressions, evaluation, and validation.
//!
//! This layer has no Cargo, filesystem, or parser-crate dependencies.
//! The pipeline enters here at [`Evaluator`].

mod choice;
mod defconfig;
mod dependency;
mod error;
mod evaluation;
mod expression;
mod limits;
mod symbol;
mod symbol_table;
mod tristate;
mod validation;
mod value;

pub use choice::ChoiceGroup;
pub use defconfig::{Assignment, AssignmentSet};
pub use dependency::{DependencyGraph, ReverseEdge, ReverseKind};
pub use error::DomainError;
pub use evaluation::{EvaluatedConfig, EvaluationContext, Evaluator, evaluate};
pub use expression::{CompareOp, Expression};
pub use limits::Limits;
pub use symbol::{
    DefaultValue, RangeBound, ReverseDep, Symbol, SymbolType, ValueRange, config_ident,
    strip_config_prefix, validate_symbol_name,
};
pub use symbol_table::SymbolTable;
pub use tristate::Tristate;
pub use validation::{IssueKind, ValidationIssue, ValidationReport};
pub use value::Value;
