use super::error::DomainError;
use super::limits::Limits;
use super::value::Value;
use std::fmt;

/// Comparison operators preserved from the Kconfig expression AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl CompareOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }
}

/// Domain expression AST. Independent of the parser crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Symbol(String),
    Constant(Value),
    Not(Box<Expression>),
    And(Vec<Expression>),
    Or(Vec<Expression>),
    Compare {
        left: Box<Expression>,
        op: CompareOp,
        right: Box<Expression>,
    },
}

impl Expression {
    pub fn symbol(name: impl Into<String>) -> Self {
        Self::Symbol(name.into())
    }

    pub fn and(parts: Vec<Self>) -> Self {
        match parts.len() {
            0 => Self::Constant(Value::Bool(true)),
            1 => match parts.into_iter().next() {
                Some(part) => part,
                None => Self::Constant(Value::Bool(true)),
            },
            _ => Self::And(parts),
        }
    }

    pub fn or(parts: Vec<Self>) -> Self {
        match parts.len() {
            0 => Self::Constant(Value::Bool(false)),
            1 => match parts.into_iter().next() {
                Some(part) => part,
                None => Self::Constant(Value::Bool(false)),
            },
            _ => Self::Or(parts),
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Self::Symbol(_) | Self::Constant(_) => 1,
            Self::Not(inner) => 1 + inner.depth(),
            Self::And(parts) | Self::Or(parts) => {
                1 + parts.iter().map(Self::depth).max().unwrap_or(0)
            }
            Self::Compare { left, right, .. } => 1 + left.depth().max(right.depth()),
        }
    }

    pub fn check_depth(&self, limits: Limits) -> Result<(), DomainError> {
        if self.depth() > limits.max_expression_depth {
            Err(DomainError::ExpressionDepthExceeded {
                max: limits.max_expression_depth,
            })
        } else {
            Ok(())
        }
    }

    /// Render the expression with `CONFIG_` names so messages match defconfig syntax.
    pub fn config_display(&self) -> String {
        match self {
            Self::Symbol(name) => super::symbol::config_ident(name),
            Self::Constant(v) => v.to_string(),
            Self::Not(inner) => format!("!{}", inner.config_display()),
            Self::And(parts) => {
                let joined = parts
                    .iter()
                    .map(Self::config_display)
                    .collect::<Vec<_>>()
                    .join(" && ");
                format!("({joined})")
            }
            Self::Or(parts) => {
                let joined = parts
                    .iter()
                    .map(Self::config_display)
                    .collect::<Vec<_>>()
                    .join(" || ");
                format!("({joined})")
            }
            Self::Compare { left, op, right } => {
                format!(
                    "({} {} {})",
                    left.config_display(),
                    op.as_str(),
                    right.config_display()
                )
            }
        }
    }

    /// Symbol names referenced by this expression, excluding `y` / `m` / `n`.
    pub fn referenced_symbols(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_symbols(&mut out);
        out
    }

    fn collect_symbols<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Self::Symbol(name) => {
                if !is_tristate_literal(name) {
                    out.push(name);
                }
            }
            Self::Constant(_) => {}
            Self::Not(inner) => inner.collect_symbols(out),
            Self::And(parts) | Self::Or(parts) => {
                for part in parts {
                    part.collect_symbols(out);
                }
            }
            Self::Compare { left, right, .. } => {
                left.collect_symbols(out);
                right.collect_symbols(out);
            }
        }
    }
}

fn is_tristate_literal(name: &str) -> bool {
    matches!(name, "y" | "Y" | "m" | "M" | "n" | "N")
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(name) => f.write_str(name),
            Self::Constant(v) => write!(f, "{v}"),
            Self::Not(inner) => write!(f, "!({inner})"),
            Self::And(parts) => {
                let joined = parts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" && ");
                write!(f, "({joined})")
            }
            Self::Or(parts) => {
                let joined = parts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" || ");
                write!(f, "({joined})")
            }
            Self::Compare { left, op, right } => {
                write!(f, "({left} {} {right})", op.as_str())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CompareOp, Expression};
    use crate::domain::Value;

    #[test]
    fn depth_counts_nesting() {
        let expr = Expression::And(vec![
            Expression::symbol("A"),
            Expression::Not(Box::new(Expression::symbol("B"))),
        ]);
        assert_eq!(expr.depth(), 3);
    }

    #[test]
    fn compare_display() {
        let expr = Expression::Compare {
            left: Box::new(Expression::symbol("SIZE")),
            op: CompareOp::Greater,
            right: Box::new(Expression::Constant(Value::Int(0))),
        };
        assert_eq!(expr.to_string(), "(SIZE > 0)");
    }

    #[test]
    fn referenced_symbols_skips_tristate_literals() {
        let expr = Expression::And(vec![
            Expression::symbol("A"),
            Expression::Not(Box::new(Expression::symbol("y"))),
        ]);
        assert_eq!(expr.referenced_symbols(), ["A"]);
    }

    #[test]
    fn config_display_uses_config_prefix() {
        let expr = Expression::And(vec![
            Expression::symbol("BUS"),
            Expression::Not(Box::new(Expression::symbol("POLL"))),
        ]);
        assert_eq!(expr.config_display(), "(CONFIG_BUS && !CONFIG_POLL)");
    }
}
