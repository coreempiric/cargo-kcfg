use super::choice::ChoiceGroup;
use super::error::DomainError;
use super::limits::Limits;
use super::symbol::Symbol;
use std::collections::HashMap;

/// Ordered collection of configuration symbols and choice groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolTable {
    symbols: HashMap<String, Symbol>,
    order: Vec<String>,
    choices: Vec<ChoiceGroup>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn insert(&mut self, symbol: Symbol, limits: Limits) -> Result<(), DomainError> {
        if let Some(existing) = self.symbols.get_mut(&symbol.name) {
            existing.merge(symbol)?;
            return Ok(());
        }
        if self.order.len() >= limits.max_symbols {
            return Err(DomainError::TooManySymbols {
                max: limits.max_symbols,
            });
        }
        self.order.push(symbol.name.clone());
        self.symbols.insert(symbol.name.clone(), symbol);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.order.iter().filter_map(|name| self.symbols.get(name))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    pub fn push_choice(&mut self, mut choice: ChoiceGroup) {
        if choice.id.is_empty() {
            choice.id = format!("choice#{}", self.choices.len());
        }
        self.choices.push(choice);
    }

    pub fn choices(&self) -> &[ChoiceGroup] {
        &self.choices
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolTable;
    use crate::domain::{Limits, Symbol, SymbolType};

    #[test]
    fn insert_preserves_definition_order() {
        let limits = Limits::default();
        let mut table = SymbolTable::new();
        table
            .insert(
                Symbol::new("B", limits)
                    .unwrap()
                    .with_type(SymbolType::Bool),
                limits,
            )
            .unwrap();
        table
            .insert(
                Symbol::new("A", limits)
                    .unwrap()
                    .with_type(SymbolType::Bool),
                limits,
            )
            .unwrap();
        let names: Vec<_> = table.names().collect();
        assert_eq!(names, ["B", "A"]);
    }

    #[test]
    fn insert_merges_duplicate_names() {
        let limits = Limits::default();
        let mut table = SymbolTable::new();
        table
            .insert(
                Symbol::new("FOO", limits)
                    .unwrap()
                    .with_type(SymbolType::Bool),
                limits,
            )
            .unwrap();
        let mut second = Symbol::new("FOO", limits).unwrap();
        second.help = Some("help".into());
        table.insert(second, limits).unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table.get("FOO").unwrap().help.as_deref(), Some("help"));
    }

    #[test]
    fn rejects_when_symbol_limit_exceeded() {
        let limits = Limits {
            max_symbols: 1,
            ..Limits::default()
        };
        let mut table = SymbolTable::new();
        table
            .insert(Symbol::new("A", limits).unwrap(), limits)
            .unwrap();
        let err = table.insert(Symbol::new("B", limits).unwrap(), limits);
        assert!(matches!(
            err,
            Err(crate::domain::DomainError::TooManySymbols { .. })
        ));
    }
}
