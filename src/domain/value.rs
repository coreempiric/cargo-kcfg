use super::error::DomainError;
use super::symbol::SymbolType;
use super::tristate::Tristate;
use std::fmt;

/// A concrete configuration value with an explicit type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Tristate(Tristate),
    /// Signed 64-bit so evaluation never widens or truncates implicitly.
    /// Generated Rust constants use `u32` when the value is in range.
    Int(i64),
    Hex(u64),
    String(String),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Tristate(_) => "tristate",
            Self::Int(_) => "int",
            Self::Hex(_) => "hex",
            Self::String(_) => "string",
        }
    }

    pub fn to_tristate(&self) -> Tristate {
        match self {
            Self::Bool(v) => Tristate::from(*v),
            Self::Tristate(v) => *v,
            Self::Int(v) => Tristate::from(*v != 0),
            Self::Hex(v) => Tristate::from(*v != 0),
            Self::String(v) => Tristate::from(!v.is_empty()),
        }
    }

    pub fn as_int(&self) -> Result<i64, DomainError> {
        match self {
            Self::Int(v) => Ok(*v),
            Self::Hex(v) => i64::try_from(*v).map_err(|_| {
                DomainError::Evaluation(format!("hex value {v:#x} does not fit in i64"))
            }),
            Self::Bool(v) => Ok(i64::from(*v)),
            Self::Tristate(v) => Ok(*v as i64),
            Self::String(s) => parse_number(s).ok_or_else(|| {
                DomainError::Evaluation(format!("cannot interpret string `{s}` as an integer"))
            }),
        }
    }

    pub fn as_u32(&self) -> Result<u32, DomainError> {
        let n = self.as_int()?;
        u32::try_from(n)
            .map_err(|_| DomainError::Evaluation(format!("integer {n} is outside the u32 range")))
    }

    pub fn as_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Bool(v) => if *v { "y" } else { "n" }.to_string(),
            Self::Tristate(v) => v.as_str().to_string(),
            Self::Int(v) => v.to_string(),
            Self::Hex(v) => format!("{v:#x}"),
        }
    }

    pub fn default_for(kind: SymbolType) -> Self {
        match kind {
            SymbolType::Bool => Self::Bool(false),
            SymbolType::Tristate => Self::Tristate(Tristate::No),
            SymbolType::Int => Self::Int(0),
            SymbolType::Hex => Self::Hex(0),
            SymbolType::String => Self::String(String::new()),
        }
    }

    /// Coerce `self` into `kind` without implicit narrowing of integers.
    pub fn coerce_to(&self, kind: SymbolType) -> Result<Self, DomainError> {
        match kind {
            SymbolType::Bool => Ok(Self::Bool(self.to_tristate().is_enabled())),
            SymbolType::Tristate => Ok(Self::Tristate(self.to_tristate())),
            SymbolType::Int => Ok(Self::Int(self.as_int()?)),
            SymbolType::Hex => {
                let n = self.as_int()?;
                let u = u64::try_from(n).map_err(|_| {
                    DomainError::Evaluation(format!("integer {n} cannot be used as hex"))
                })?;
                Ok(Self::Hex(u))
            }
            SymbolType::String => Ok(Self::String(self.as_string())),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(true) => f.write_str("y"),
            Self::Bool(false) => f.write_str("n"),
            Self::Tristate(t) => write!(f, "{t}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Hex(v) => write!(f, "{v:#x}"),
            Self::String(s) => write!(f, "\"{}\"", escape_kconfig_string(s)),
        }
    }
}

pub(crate) fn escape_kconfig_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn parse_number(raw: &str) -> Option<i64> {
    let t = raw.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, parse_number};
    use crate::domain::{SymbolType, Tristate};

    #[test]
    fn tristate_from_bool_and_int() {
        assert_eq!(Value::Bool(true).to_tristate(), Tristate::Yes);
        assert_eq!(Value::Int(0).to_tristate(), Tristate::No);
        assert_eq!(Value::Int(7).to_tristate(), Tristate::Yes);
        assert_eq!(Value::String(String::new()).to_tristate(), Tristate::No);
    }

    #[test]
    fn coerce_int_to_u32_rejects_negative() {
        let err = Value::Int(-1).as_u32().unwrap_err();
        assert!(err.to_string().contains("u32"));
    }

    #[test]
    fn parse_decimal_and_hex() {
        assert_eq!(parse_number("128"), Some(128));
        assert_eq!(parse_number("0x10"), Some(16));
        assert_eq!(parse_number("0XFF"), Some(255));
    }

    #[test]
    fn default_for_each_type() {
        assert_eq!(Value::default_for(SymbolType::Bool), Value::Bool(false));
        assert_eq!(
            Value::default_for(SymbolType::Tristate),
            Value::Tristate(Tristate::No)
        );
        assert_eq!(Value::default_for(SymbolType::Int), Value::Int(0));
    }
}
