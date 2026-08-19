use std::fmt;

/// Classification of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    UnknownSymbol,
    TypeMismatch,
    OutOfRange,
    UnmetDependency,
    MissingType,
    LimitExceeded,
    ParseError,
    CyclicDependency,
    ChoiceConflict,
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownSymbol => "unknown-symbol",
            Self::TypeMismatch => "type-mismatch",
            Self::OutOfRange => "out-of-range",
            Self::UnmetDependency => "unmet-dependency",
            Self::MissingType => "missing-type",
            Self::LimitExceeded => "limit-exceeded",
            Self::ParseError => "parse-error",
            Self::CyclicDependency => "cyclic-dependency",
            Self::ChoiceConflict => "choice-conflict",
        }
    }
}

/// A single actionable validation message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub kind: IssueKind,
    pub symbol: Option<String>,
    pub message: String,
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

/// Collected validation findings. Empty `errors` means the configuration is usable.
///
/// `warnings` are non-fatal findings that do not make the configuration
/// incoherent. Unmet `depends on` after `select` is an error, not a warning.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, issue: ValidationIssue) {
        self.errors.push(issue);
    }

    pub fn warn(&mut self, issue: ValidationIssue) {
        self.warnings.push(issue);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_kind(&self, kind: IssueKind) -> bool {
        self.errors.iter().any(|e| e.kind == kind)
    }

    pub fn has_warning_kind(&self, kind: IssueKind) -> bool {
        self.warnings.iter().any(|w| w.kind == kind)
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for issue in &self.errors {
            if !first {
                writeln!(f)?;
            }
            write!(f, "{issue}")?;
            first = false;
        }
        for issue in &self.warnings {
            if !first {
                writeln!(f)?;
            }
            write!(f, "warning: {issue}")?;
            first = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{IssueKind, ValidationIssue, ValidationReport};

    #[test]
    fn display_joins_errors() {
        let mut report = ValidationReport::new();
        report.push(ValidationIssue {
            kind: IssueKind::UnknownSymbol,
            symbol: Some("FOO".into()),
            message: "unknown symbol `CONFIG_FOO`".into(),
        });
        assert!(report.to_string().contains("unknown-symbol"));
        assert!(report.has_kind(IssueKind::UnknownSymbol));
    }

    #[test]
    fn display_includes_warnings() {
        let mut report = ValidationReport::new();
        report.warn(ValidationIssue {
            kind: IssueKind::UnmetDependency,
            symbol: Some("HAS_DMA".into()),
            message:
                "`CONFIG_HAS_DMA` is selected by `CONFIG_DRIVER` but has unmet dependencies (BUS)"
                    .into(),
        });
        let text = report.to_string();
        assert!(text.contains("warning:"));
        assert!(text.contains("HAS_DMA"));
        assert!(report.has_warning_kind(IssueKind::UnmetDependency));
        assert!(!report.has_errors());
    }
}
