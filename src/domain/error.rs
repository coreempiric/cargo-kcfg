use super::limits::Limits;
use thiserror::Error;

/// Recoverable domain failures. Library code returns these instead of panicking.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error(
        "symbol name `{name}` exceeds the maximum of {max} characters. Shorten the name in Kconfig"
    )]
    SymbolNameTooLong { name: String, max: usize },

    #[error(
        "symbol name `{name}` is not a valid Kconfig identifier. Use letters, digits, and `_`, starting with a letter or `_`"
    )]
    InvalidSymbolName { name: String },

    #[error(
        "string value for `{symbol}` exceeds the maximum of {max} characters. Shorten it in the defconfig"
    )]
    StringValueTooLong { symbol: String, max: usize },

    #[error(
        "symbol table would exceed the maximum of {max} symbols. Split the Kconfig tree or raise the limit"
    )]
    TooManySymbols { max: usize },

    #[error(
        "source directive nesting exceeds the maximum depth of {max}. Flatten `source` includes or raise the limit"
    )]
    SourceDepthExceeded { max: usize },

    #[error(
        "expression nesting exceeds the maximum depth of {max}. Simplify the expression or raise the limit"
    )]
    ExpressionDepthExceeded { max: usize },

    #[error(
        "input file `{path}` is {size} bytes, which exceeds the limit of {max} bytes. Split the file or raise the limit"
    )]
    FileTooLarge { path: String, size: u64, max: u64 },

    #[error(
        "cyclic dependency involving symbol `{name}`. Remove one `depends on` along the cycle in Kconfig"
    )]
    CyclicDependency { name: String },

    #[error(
        "dependency resolution did not converge within {max} iterations. Check for a `select`/`imply` loop, or raise the iteration limit"
    )]
    ResolutionDidNotConverge { max: usize },

    #[error(
        "symbol `{name}` has no type. Add `bool`, `tristate`, `int`, `hex`, or `string` to `config {name}` in Kconfig"
    )]
    MissingType { name: String },

    #[error(
        "symbol `{name}` is defined as both {first} and {second}. Use the same type in every `config {name}` entry"
    )]
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
