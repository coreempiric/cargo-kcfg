use super::error::DomainError;
use super::expression::Expression;
use super::limits::Limits;
use std::fmt;

/// Declared type of a `config` symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    Bool,
    Tristate,
    Int,
    Hex,
    String,
}

impl SymbolType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Tristate => "tristate",
            Self::Int => "int",
            Self::Hex => "hex",
            Self::String => "string",
        }
    }

    /// Rust type used when the integer width is not yet known.
    pub fn rust_type(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Tristate => "Tristate",
            Self::Int | Self::Hex => "u32",
            Self::String => "&'static str",
        }
    }
}

impl fmt::Display for SymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `default` attribute: value plus optional `if` condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultValue {
    pub value: Expression,
    pub condition: Option<Expression>,
}

/// Bound of a `range` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeBound {
    Number(i64),
    Symbol(String),
}

impl RangeBound {
    pub fn from_number_or_symbol(raw: &str) -> Self {
        if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"))
            && let Ok(n) = i64::from_str_radix(hex, 16)
        {
            return Self::Number(n);
        }
        if let Ok(n) = raw.parse::<i64>() {
            return Self::Number(n);
        }
        Self::Symbol(raw.to_string())
    }
}

/// An `int`/`hex` `range` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRange {
    pub min: RangeBound,
    pub max: RangeBound,
    pub condition: Option<Expression>,
}

/// A reverse dependency: `select FOO [if EXPR]` or `imply FOO [if EXPR]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverseDep {
    pub symbol: String,
    pub condition: Option<Expression>,
}

impl ReverseDep {
    pub fn always(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            condition: None,
        }
    }
}

/// A configuration symbol (`config FOO`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: Option<SymbolType>,
    pub prompt: Option<String>,
    pub help: Option<String>,
    pub defaults: Vec<DefaultValue>,
    pub depends: Vec<Expression>,
    pub ranges: Vec<ValueRange>,
    /// `select` reverse dependencies. Enabling this symbol raises each target.
    pub selects: Vec<ReverseDep>,
    /// `imply` reverse dependencies. Weaker than `select`: user `n` and unmet
    /// target dependencies both win.
    pub implies: Vec<ReverseDep>,
}

impl Symbol {
    pub fn new(name: impl Into<String>, limits: Limits) -> Result<Self, DomainError> {
        let name = name.into();
        validate_symbol_name(&name, limits)?;
        Ok(Self {
            name,
            kind: None,
            prompt: None,
            help: None,
            defaults: Vec::new(),
            depends: Vec::new(),
            ranges: Vec::new(),
            selects: Vec::new(),
            implies: Vec::new(),
        })
    }

    pub fn with_type(mut self, kind: SymbolType) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn require_type(&self) -> Result<SymbolType, DomainError> {
        self.kind.ok_or_else(|| DomainError::MissingType {
            name: self.name.clone(),
        })
    }

    /// Merge a later definition of the same symbol (Kconfig allows this).
    pub fn merge(&mut self, other: Symbol) -> Result<(), DomainError> {
        if let (Some(a), Some(b)) = (self.kind, other.kind)
            && a != b
        {
            return Err(DomainError::ConflictingType {
                name: self.name.clone(),
                first: a.to_string(),
                second: b.to_string(),
            });
        }
        if self.kind.is_none() {
            self.kind = other.kind;
        }
        if self.prompt.is_none() {
            self.prompt = other.prompt;
        }
        if self.help.is_none() {
            self.help = other.help;
        }
        self.defaults.extend(other.defaults);
        self.depends.extend(other.depends);
        self.ranges.extend(other.ranges);
        self.selects.extend(other.selects);
        self.implies.extend(other.implies);
        Ok(())
    }
}

pub fn validate_symbol_name(name: &str, limits: Limits) -> Result<(), DomainError> {
    if name.is_empty() || name.len() > limits.max_symbol_name_len {
        return Err(DomainError::symbol_name_too_long(name, limits));
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(DomainError::InvalidSymbolName {
            name: name.to_string(),
        });
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(DomainError::InvalidSymbolName {
            name: name.to_string(),
        });
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(DomainError::InvalidSymbolName {
            name: name.to_string(),
        });
    }
    Ok(())
}

/// Strip a leading `CONFIG_` prefix used in defconfig / `.config` files.
pub fn strip_config_prefix(name: &str) -> &str {
    name.strip_prefix("CONFIG_").unwrap_or(name)
}

/// Emit the `CONFIG_` form used in generated artefacts.
pub fn config_ident(name: &str) -> String {
    if name.starts_with("CONFIG_") {
        name.to_string()
    } else {
        format!("CONFIG_{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::{Symbol, SymbolType, validate_symbol_name};
    use crate::domain::Limits;

    #[test]
    fn rejects_empty_and_illegal_names() {
        let limits = Limits::default();
        assert!(validate_symbol_name("", limits).is_err());
        assert!(validate_symbol_name("1FOO", limits).is_err());
        assert!(validate_symbol_name("FOO-BAR", limits).is_err());
        assert!(validate_symbol_name("FOO_BAR", limits).is_ok());
    }

    #[test]
    fn merge_appends_defaults_depends_and_reverse_deps() {
        let limits = Limits::default();
        let mut a = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut b = Symbol::new("FOO", limits).unwrap();
        b.defaults.push(crate::domain::DefaultValue {
            value: crate::domain::Expression::symbol("y"),
            condition: None,
        });
        b.selects.push(crate::domain::ReverseDep::always("HAS_FOO"));
        b.implies.push(crate::domain::ReverseDep::always("LOG"));
        a.merge(b).unwrap();
        assert_eq!(a.defaults.len(), 1);
        assert_eq!(a.selects.len(), 1);
        assert_eq!(a.implies.len(), 1);
        assert_eq!(a.kind, Some(SymbolType::Bool));
    }

    #[test]
    fn merge_rejects_conflicting_types() {
        let limits = Limits::default();
        let mut a = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let b = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        assert!(a.merge(b).is_err());
    }
}
