use super::expression::Expression;
use super::symbol::{DefaultValue, SymbolType};

/// A `choice` / `endchoice` group. Members are bool or tristate symbols that
/// are mutually related: a bool choice selects exactly one member (unless
/// `optional`); a tristate choice may select several as `m`, but at most one `y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceGroup {
    /// Stable identifier (`choice#N`, or a prompt used only for messages).
    pub id: String,
    pub prompt: Option<String>,
    pub kind: Option<SymbolType>,
    pub optional: bool,
    pub defaults: Vec<DefaultValue>,
    pub depends: Vec<Expression>,
    pub members: Vec<String>,
}

impl ChoiceGroup {
    pub fn new(id: impl Into<String>, members: Vec<String>) -> Self {
        Self {
            id: id.into(),
            prompt: None,
            kind: None,
            optional: false,
            defaults: Vec::new(),
            depends: Vec::new(),
            members,
        }
    }

    pub fn display_name(&self) -> &str {
        self.prompt
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.id)
    }

    pub fn with_type(mut self, kind: SymbolType) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ChoiceGroup;
    use crate::domain::SymbolType;

    #[test]
    fn display_name_prefers_prompt() {
        let mut choice = ChoiceGroup::new("choice#0", vec!["A".into(), "B".into()]);
        assert_eq!(choice.display_name(), "choice#0");
        choice.prompt = Some("Console".into());
        assert_eq!(choice.display_name(), "Console");
        let typed = ChoiceGroup::new("c", vec!["A".into()]).with_type(SymbolType::Bool);
        assert_eq!(typed.kind, Some(SymbolType::Bool));
    }
}
