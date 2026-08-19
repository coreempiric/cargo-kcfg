use super::error::DomainError;
use super::limits::Limits;
use super::symbol::strip_config_prefix;
use std::collections::HashMap;

/// A single assignment from a `*_defconfig` (or `.config`) file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    /// Symbol name without a `CONFIG_` prefix.
    pub name: String,
    /// Raw right-hand side as written (`y`, `256`, `"board"`, `0x10`, …).
    pub raw: String,
    pub line: u32,
}

/// Ordered set of user assignments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssignmentSet {
    assignments: Vec<Assignment>,
    by_name: HashMap<String, usize>,
}

impl AssignmentSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    pub fn push(&mut self, assignment: Assignment, limits: Limits) -> Result<(), DomainError> {
        if assignment.raw.len() > limits.max_string_value_len {
            return Err(DomainError::string_value_too_long(&assignment.name, limits));
        }
        if let Some(&idx) = self.by_name.get(&assignment.name) {
            self.assignments[idx] = assignment;
            return Ok(());
        }
        self.by_name
            .insert(assignment.name.clone(), self.assignments.len());
        self.assignments.push(assignment);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Assignment> {
        let key = strip_config_prefix(name);
        self.by_name.get(key).map(|&i| &self.assignments[i])
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Assignment> {
        self.assignments.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{Assignment, AssignmentSet};
    use crate::domain::Limits;

    #[test]
    fn later_assignment_overrides_earlier() {
        let limits = Limits::default();
        let mut set = AssignmentSet::new();
        set.push(
            Assignment {
                name: "FOO".into(),
                raw: "y".into(),
                line: 1,
            },
            limits,
        )
        .unwrap();
        set.push(
            Assignment {
                name: "FOO".into(),
                raw: "n".into(),
                line: 2,
            },
            limits,
        )
        .unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(set.get("CONFIG_FOO").unwrap().raw, "n");
    }
}
