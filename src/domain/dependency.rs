use super::expression::Expression;
use super::symbol_table::SymbolTable;
use std::collections::{HashMap, HashSet};

/// Kind of reverse dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReverseKind {
    Select,
    Imply,
}

/// `select` / `imply` edge from one symbol to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverseEdge {
    pub from: String,
    pub to: String,
    pub kind: ReverseKind,
    pub condition: Option<Expression>,
}

/// Direct (`depends on`) and reverse (`select` / `imply`) dependency graph.
///
/// Built from a [`SymbolTable`]; the domain evaluator uses reverse edges when
/// applying automatic enabling, and can report cycles in direct dependencies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    direct: HashMap<String, Vec<String>>,
    reverse: Vec<ReverseEdge>,
}

impl DependencyGraph {
    pub fn from_table(table: &SymbolTable) -> Self {
        let mut direct = HashMap::new();
        let mut reverse = Vec::new();
        for symbol in table.iter() {
            let mut refs = Vec::new();
            for dep in &symbol.depends {
                for name in dep.referenced_symbols() {
                    if !refs.iter().any(|existing| existing == name) {
                        refs.push(name.to_string());
                    }
                }
            }
            direct.insert(symbol.name.clone(), refs);
            for sel in &symbol.selects {
                reverse.push(ReverseEdge {
                    from: symbol.name.clone(),
                    to: sel.symbol.clone(),
                    kind: ReverseKind::Select,
                    condition: sel.condition.clone(),
                });
            }
            for imp in &symbol.implies {
                reverse.push(ReverseEdge {
                    from: symbol.name.clone(),
                    to: imp.symbol.clone(),
                    kind: ReverseKind::Imply,
                    condition: imp.condition.clone(),
                });
            }
        }
        Self { direct, reverse }
    }

    pub fn direct_refs(&self, name: &str) -> &[String] {
        self.direct.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn reverse_edges(&self) -> &[ReverseEdge] {
        &self.reverse
    }

    /// Depth-first search for a cycle in the direct-dependency graph.
    ///
    /// Returns the cycle as a list of symbol names that starts and ends on
    /// the same node (`A, B, A`).
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut nodes: Vec<&String> = self.direct.keys().collect();
        nodes.sort();
        for node in nodes {
            if let Some(cycle) = self.dfs(node, &mut visiting, &mut visited, &mut stack) {
                return Some(cycle);
            }
        }
        None
    }

    fn dfs(
        &self,
        node: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if visited.contains(node) {
            return None;
        }
        if visiting.contains(node) {
            let start = stack.iter().position(|n| n == node).unwrap_or(0);
            let mut cycle = stack[start..].to_vec();
            cycle.push(node.to_string());
            return Some(cycle);
        }
        visiting.insert(node.to_string());
        stack.push(node.to_string());
        if let Some(neighbours) = self.direct.get(node) {
            for next in neighbours {
                if let Some(cycle) = self.dfs(next, visiting, visited, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        visiting.remove(node);
        visited.insert(node.to_string());
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyGraph, ReverseKind};
    use crate::domain::{Expression, Limits, ReverseDep, Symbol, SymbolTable, SymbolType};

    fn table(symbols: Vec<Symbol>) -> SymbolTable {
        let limits = Limits::default();
        let mut table = SymbolTable::new();
        for symbol in symbols {
            table.insert(symbol, limits).unwrap();
        }
        table
    }

    #[test]
    fn records_direct_and_reverse_edges() {
        let limits = Limits::default();
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.depends.push(Expression::symbol("BAR"));
        foo.selects.push(ReverseDep::always("HAS_FOO"));
        let bar = Symbol::new("BAR", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let has = Symbol::new("HAS_FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let graph = DependencyGraph::from_table(&table(vec![foo, bar, has]));
        assert_eq!(graph.direct_refs("FOO"), ["BAR"]);
        assert!(
            graph
                .reverse_edges()
                .iter()
                .any(|e| e.from == "FOO" && e.to == "HAS_FOO" && e.kind == ReverseKind::Select)
        );
        assert!(graph.find_cycle().is_none());
    }

    #[test]
    fn finds_direct_dependency_cycle() {
        let limits = Limits::default();
        let mut a = Symbol::new("A", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        a.depends.push(Expression::symbol("B"));
        let mut b = Symbol::new("B", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        b.depends.push(Expression::symbol("A"));
        let graph = DependencyGraph::from_table(&table(vec![a, b]));
        let cycle = graph.find_cycle().expect("cycle");
        assert!(cycle.contains(&"A".to_string()));
        assert!(cycle.contains(&"B".to_string()));
        assert_eq!(cycle.first(), cycle.last());
    }
}
