use super::limits::Limits;
use thiserror::Error;

/// Recoverable domain failures. Library code returns these instead of panicking.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("symbol name `{name}` exceeds the maximum length of {max} characters")]
    SymbolNameTooLong { name: String, max: usize },

    #[error("symbol name `{name}` is not a valid Kconfig identifier")]
    InvalidSymbolName { name: String },

    #[error("string value for `{symbol}` exceeds the maximum length of {max} characters")]
    StringValueTooLong { symbol: String, max: usize },

    #[error("symbol table would exceed the maximum of {max} symbols")]
    TooManySymbols { max: usize },

    #[error("source directive nesting exceeds the maximum depth of {max}")]
    SourceDepthExceeded { max: usize },

    #[error("expression nesting exceeds the maximum depth of {max}")]
    ExpressionDepthExceeded { max: usize },

    #[error("input file `{path}` is {size} bytes, which exceeds the limit of {max} bytes")]
    FileTooLarge { path: String, size: u64, max: u64 },

    #[error("cyclic dependency involving symbol `{name}`")]
    CyclicDependency { name: String },

    #[error("dependency resolution did not converge within {max} iterations")]
    ResolutionDidNotConverge { max: usize },

    #[error("symbol `{name}` has no type")]
    MissingType { name: String },

    #[error("symbol `{name}` is defined with conflicting types {first} and {second}")]
    ConflictingType {
        name: String,
        first: String,
        second: String,
    },

    #[error("{0}")]
    Evaluation(String),
}

impl DomainError {
    pub fn symbol_name_too_long(name: impl Into<String>, limits: Limits) -> Self {
        Self::SymbolNameTooLong {
            name: name.into(),
            max: limits.max_symbol_name_len,
        }
    }

    pub fn string_value_too_long(symbol: impl Into<String>, limits: Limits) -> Self {
        Self::StringValueTooLong {
            symbol: symbol.into(),
            max: limits.max_string_value_len,
        }
    }
}
