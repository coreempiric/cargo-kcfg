//! Evaluation and validation of a symbol table against user assignments.
//!
//! Pipeline step: [`Evaluator::evaluate`]. [`EvaluationContext`] walks
//! expressions; [`EvaluatedConfig`] is the successful result.

use super::choice::ChoiceGroup;
use super::defconfig::AssignmentSet;
use super::dependency::{DependencyGraph, ReverseKind};
use super::error::DomainError;
use super::expression::{CompareOp, Expression};
use super::limits::Limits;
use super::symbol::{RangeBound, ReverseDep, Symbol, SymbolType};
use super::symbol_table::SymbolTable;
use super::tristate::Tristate;
use super::validation::{IssueKind, ValidationIssue, ValidationReport};
use super::value::{Value, parse_number};
use std::collections::{HashMap, HashSet};

/// Fully evaluated configuration: every symbol has a concrete value.
///
/// Produced by [`Evaluator::evaluate`] when there are no validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedConfig {
    pub table: SymbolTable,
    pub values: HashMap<String, Value>,
    /// Non-fatal findings, such as `select` bypassing the target's `depends on`.
    pub warnings: Vec<ValidationIssue>,
}

impl EvaluatedConfig {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub fn has_warning_kind(&self, kind: IssueKind) -> bool {
        self.warnings.iter().any(|w| w.kind == kind)
    }
}

/// Walks expressions against a symbol table and user assignments.
pub struct EvaluationContext<'a> {
    table: &'a SymbolTable,
    assignments: &'a AssignmentSet,
    limits: Limits,
    values: HashMap<String, Value>,
    visiting: HashSet<String>,
    report: ValidationReport,
    graph: DependencyGraph,
}

/// Evaluates user assignments against a symbol table.
///
/// Direct values (user assignment, then the first visible default, then the
/// type zero) are resolved first. `select` then raises bool/tristate targets
/// as a reverse lower bound; `imply` does the same unless the user set the
/// target or the target's own dependencies are unmet.
///
/// This type is filesystem-free. File loading lives in [`crate::Pipeline`].
pub struct Evaluator {
    limits: Limits,
}

impl Evaluator {
    /// Create an evaluator that applies `limits` during resolution.
    pub fn new(limits: Limits) -> Self {
        Self { limits }
    }

    /// Evaluate `assignments` against `table`.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationReport`] when the configuration is incoherent:
    /// unknown symbols, type mismatches, out-of-range integers, unmet
    /// `depends on` (user assignment or `select`), illegal `select`/`imply`
    /// targets, `choice` violations, or a dependency cycle.
    pub fn evaluate(
        &self,
        table: SymbolTable,
        assignments: &AssignmentSet,
    ) -> Result<EvaluatedConfig, ValidationReport> {
        let mut ctx = EvaluationContext {
            table: &table,
            assignments,
            limits: self.limits,
            values: HashMap::new(),
            visiting: HashSet::new(),
            report: ValidationReport::new(),
            graph: DependencyGraph::from_table(&table),
        };

        if let Some(cycle) = ctx.graph.find_cycle() {
            ctx.report.push(ValidationIssue {
                kind: IssueKind::CyclicDependency,
                symbol: cycle.first().cloned(),
                message: format_cycle(&cycle),
            });
        }

        for assignment in assignments.iter() {
            if !table.contains(&assignment.name) {
                ctx.report.push(ValidationIssue {
                kind: IssueKind::UnknownSymbol,
                symbol: Some(assignment.name.clone()),
                message: format!(
                    "`{}` is assigned on line {} but is not defined in Kconfig. Remove it from the defconfig, or add `config {}` to Kconfig",
                    crate::domain::config_ident(&assignment.name),
                    assignment.line,
                    assignment.name
                ),
            });
            }
        }

        for symbol in table.iter() {
            if let Err(err) = ctx.resolve(&symbol.name) {
                let (kind, symbol_name) = match &err {
                    DomainError::CyclicDependency { name } => {
                        (IssueKind::CyclicDependency, Some(name.clone()))
                    }
                    _ => (IssueKind::ParseError, Some(symbol.name.clone())),
                };
                if kind == IssueKind::CyclicDependency
                    && ctx.report.has_kind(IssueKind::CyclicDependency)
                {
                    continue;
                }
                ctx.report.push(ValidationIssue {
                    kind,
                    symbol: symbol_name,
                    message: err.to_string(),
                });
            }
        }

        ctx.validate_reverse_dep_targets();

        if let Err(err) = ctx.apply_reverse_dependencies() {
            let kind = match &err {
                DomainError::ResolutionDidNotConverge { .. } => IssueKind::LimitExceeded,
                DomainError::CyclicDependency { .. } => IssueKind::CyclicDependency,
                _ => IssueKind::ParseError,
            };
            ctx.report.push(ValidationIssue {
                kind,
                symbol: None,
                message: err.to_string(),
            });
        }

        if let Err(err) = ctx.enforce_choice_constraints() {
            ctx.report.push(ValidationIssue {
                kind: IssueKind::ParseError,
                symbol: None,
                message: err.to_string(),
            });
        }

        if let Err(err) = ctx.classify_unmet_dependencies() {
            ctx.report.push(ValidationIssue {
                kind: IssueKind::ParseError,
                symbol: None,
                message: err.to_string(),
            });
        }

        let EvaluationContext { values, report, .. } = ctx;
        if report.has_errors() {
            return Err(report);
        }

        Ok(EvaluatedConfig {
            table,
            values,
            warnings: report.warnings,
        })
    }
}

/// Evaluate assignments against the symbol table.
///
/// Convenience wrapper around [`Evaluator::evaluate`].
///
/// # Errors
///
/// Same as [`Evaluator::evaluate`].
pub fn evaluate(
    table: SymbolTable,
    assignments: &AssignmentSet,
    limits: Limits,
) -> Result<EvaluatedConfig, ValidationReport> {
    Evaluator::new(limits).evaluate(table, assignments)
}

impl EvaluationContext<'_> {
    fn resolve(&mut self, name: &str) -> Result<Value, DomainError> {
        if let Some(value) = self.values.get(name) {
            return Ok(value.clone());
        }
        if !self.visiting.insert(name.to_string()) {
            return Err(DomainError::CyclicDependency {
                name: name.to_string(),
            });
        }

        let symbol = self
            .table
            .get(name)
            .ok_or_else(|| DomainError::Evaluation(format!("undefined symbol `{name}`")))?
            .clone();
        let kind = match symbol.require_type() {
            Ok(k) => k,
            Err(err) => {
                self.visiting.remove(name);
                self.report.push(ValidationIssue {
                    kind: IssueKind::MissingType,
                    symbol: Some(name.to_string()),
                    message: err.to_string(),
                });
                let fallback = Value::default_for(SymbolType::Bool);
                self.values.insert(name.to_string(), fallback.clone());
                return Ok(fallback);
            }
        };

        let dep_bound = self.dependency_bound(&symbol)?;
        let deps_ok = dep_bound.is_enabled();
        let mut value = if let Some(assignment) = self.assignments.get(name).cloned() {
            match parse_assignment(&symbol, &assignment.raw, self.limits) {
                Ok(parsed) => parsed,
                Err(issue) => {
                    self.report.push(issue);
                    Value::default_for(kind)
                }
            }
        } else if !deps_ok {
            Value::default_for(kind)
        } else {
            self.apply_defaults(&symbol, kind)?
        };

        if deps_ok && kind == SymbolType::Tristate {
            value = Value::Tristate(value.to_tristate().and(dep_bound));
        }

        if deps_ok && let Err(issue) = self.check_range(&symbol, &value) {
            self.report.push(issue);
        }

        self.visiting.remove(name);
        self.values.insert(name.to_string(), value.clone());
        Ok(value)
    }

    fn dependencies_met(&mut self, symbol: &Symbol) -> Result<bool, DomainError> {
        Ok(self.dependency_bound(symbol)?.is_enabled())
    }

    fn dependency_bound(&mut self, symbol: &Symbol) -> Result<Tristate, DomainError> {
        if symbol.depends.is_empty() {
            return Ok(Tristate::Yes);
        }
        let combined = Expression::and(symbol.depends.clone());
        combined.check_depth(self.limits)?;
        Ok(self.eval_expr(&combined)?.to_tristate())
    }

    fn apply_defaults(&mut self, symbol: &Symbol, kind: SymbolType) -> Result<Value, DomainError> {
        for default in &symbol.defaults {
            let cond_ok = match &default.condition {
                None => true,
                Some(cond) => self.eval_expr(cond)?.to_tristate().is_enabled(),
            };
            if cond_ok {
                let raw = self.eval_expr(&default.value)?;
                return raw.coerce_to(kind);
            }
        }
        Ok(Value::default_for(kind))
    }

    fn check_range(&mut self, symbol: &Symbol, value: &Value) -> Result<(), ValidationIssue> {
        let Some(kind @ (SymbolType::Int | SymbolType::Hex)) = symbol.kind else {
            return Ok(());
        };
        let n = value.as_int().map_err(|err| ValidationIssue {
            kind: IssueKind::TypeMismatch,
            symbol: Some(symbol.name.clone()),
            message: err.to_string(),
        })?;
        for range in &symbol.ranges {
            let cond_ok = match &range.condition {
                None => true,
                Some(cond) => self
                    .eval_expr(cond)
                    .map(|v| v.to_tristate().is_enabled())
                    .unwrap_or(false),
            };
            if !cond_ok {
                continue;
            }
            let min = self
                .bound_value(&range.min)
                .map_err(|message| ValidationIssue {
                    kind: IssueKind::OutOfRange,
                    symbol: Some(symbol.name.clone()),
                    message,
                })?;
            let max = self
                .bound_value(&range.max)
                .map_err(|message| ValidationIssue {
                    kind: IssueKind::OutOfRange,
                    symbol: Some(symbol.name.clone()),
                    message,
                })?;
            if n < min || n > max {
                return Err(ValidationIssue {
                    kind: IssueKind::OutOfRange,
                    symbol: Some(symbol.name.clone()),
                    message: format!(
                        "`{}={n}` is outside the allowed {kind} range {min}..={max}. Set it to a value in that range in the defconfig",
                        crate::domain::config_ident(&symbol.name)
                    ),
                });
            }
        }
        Ok(())
    }

    fn bound_value(&mut self, bound: &RangeBound) -> Result<i64, String> {
        match bound {
            RangeBound::Number(n) => Ok(*n),
            RangeBound::Symbol(name) => {
                if let Some(t) = tristate_const(name) {
                    return Ok(t as i64);
                }
                self.resolve(name)
                    .and_then(|v| v.as_int())
                    .map_err(|err| err.to_string())
            }
        }
    }

    pub fn eval_expr(&mut self, expr: &Expression) -> Result<Value, DomainError> {
        expr.check_depth(self.limits)?;
        self.eval_expr_inner(expr, 0)
    }

    fn eval_expr_inner(&mut self, expr: &Expression, depth: usize) -> Result<Value, DomainError> {
        if depth > self.limits.max_expression_depth {
            return Err(DomainError::ExpressionDepthExceeded {
                max: self.limits.max_expression_depth,
            });
        }
        match expr {
            Expression::Constant(v) => Ok(v.clone()),
            Expression::Symbol(name) => self.eval_symbol_ref(name),
            Expression::Not(inner) => {
                let v = self.eval_expr_inner(inner, depth + 1)?;
                Ok(Value::Tristate(!v.to_tristate()))
            }
            Expression::And(parts) => {
                let mut acc = Tristate::Yes;
                for part in parts {
                    acc = acc.and(self.eval_expr_inner(part, depth + 1)?.to_tristate());
                    if acc == Tristate::No {
                        break;
                    }
                }
                Ok(Value::Tristate(acc))
            }
            Expression::Or(parts) => {
                let mut acc = Tristate::No;
                for part in parts {
                    acc = acc.or(self.eval_expr_inner(part, depth + 1)?.to_tristate());
                    if acc == Tristate::Yes {
                        break;
                    }
                }
                Ok(Value::Tristate(acc))
            }
            Expression::Compare { left, op, right } => {
                let l = self.eval_expr_inner(left, depth + 1)?;
                let r = self.eval_expr_inner(right, depth + 1)?;
                Ok(Value::Bool(compare_values(&l, *op, &r)?))
            }
        }
    }

    fn eval_symbol_ref(&mut self, name: &str) -> Result<Value, DomainError> {
        if let Some(t) = tristate_const(name)
            && !self.table.contains(name)
        {
            return Ok(Value::Tristate(t));
        }
        if self.table.contains(name) {
            return self.resolve(name);
        }
        // Unresolved references evaluate to `n`, matching Kconfig.
        Ok(Value::Tristate(Tristate::No))
    }

    fn validate_reverse_dep_targets(&mut self) {
        let edges: Vec<(String, &'static str, ReverseDep)> = self
            .table
            .iter()
            .flat_map(|symbol| {
                let from = symbol.name.clone();
                let selects = symbol
                    .selects
                    .iter()
                    .cloned()
                    .map(move |dep| (from.clone(), "select", dep));
                let from = symbol.name.clone();
                let implies = symbol
                    .implies
                    .iter()
                    .cloned()
                    .map(move |dep| (from.clone(), "imply", dep));
                selects.chain(implies)
            })
            .collect();
        for (from, verb, dep) in edges {
            match self.table.get(&dep.symbol) {
                None => self.report.push(ValidationIssue {
                    kind: IssueKind::UnknownSymbol,
                    symbol: Some(dep.symbol.clone()),
                    message: format!(
                        "`{}` has `{verb} {}`, but `{}` is not defined. Add `config {}` to Kconfig, or remove that `{verb}`",
                        crate::domain::config_ident(&from),
                        dep.symbol,
                        crate::domain::config_ident(&dep.symbol),
                        dep.symbol
                    ),
                }),
                Some(target) => match target.kind {
                    Some(SymbolType::Bool) | Some(SymbolType::Tristate) => {}
                    Some(kind) => self.report.push(ValidationIssue {
                        kind: IssueKind::TypeMismatch,
                        symbol: Some(dep.symbol.clone()),
                        message: format!(
                            "`{}` has `{verb} {}`, but `{}` is {kind}; {verb} can only target bool or tristate. Point it at a bool or tristate, or remove it",
                            crate::domain::config_ident(&from),
                            dep.symbol,
                            crate::domain::config_ident(&dep.symbol)
                        ),
                    }),
                    None => {}
                },
            }
        }
    }

    fn apply_reverse_dependencies(&mut self) -> Result<(), DomainError> {
        let cap = self.limits.max_resolution_iterations;
        let mut iterations = 0_usize;
        loop {
            let mut changed = false;
            changed |= self.raise_from_graph()?;
            changed |= self.refresh_newly_visible_defaults()?;
            changed |= self.apply_choice_defaults()?;
            if !changed {
                return Ok(());
            }
            iterations = iterations.saturating_add(1);
            if iterations >= cap {
                return Err(DomainError::ResolutionDidNotConverge { max: cap });
            }
        }
    }

    fn raise_from_graph(&mut self) -> Result<bool, DomainError> {
        let edges = self.graph.reverse_edges().to_vec();
        let mut changed = false;
        for edge in edges {
            let bound = match self.values.get(&edge.from) {
                Some(value) => value.to_tristate(),
                None => continue,
            };
            if bound == Tristate::No {
                continue;
            }
            if !self.condition_ok(&edge.condition)? {
                continue;
            }
            changed |= match edge.kind {
                ReverseKind::Select => self.raise_target(&edge.to, bound)?,
                ReverseKind::Imply => self.raise_imply_target(&edge.to, bound)?,
            };
        }
        Ok(changed)
    }

    fn raise_imply_target(&mut self, target: &str, bound: Tristate) -> Result<bool, DomainError> {
        if self.assignments.get(target).is_some() {
            return Ok(false);
        }
        let Some(symbol) = self.table.get(target).cloned() else {
            return Ok(false);
        };
        if !self.dependencies_met(&symbol)? {
            return Ok(false);
        }
        self.raise_target(target, bound)
    }

    fn raise_target(&mut self, target: &str, bound: Tristate) -> Result<bool, DomainError> {
        let Some(symbol) = self.table.get(target) else {
            return Ok(false);
        };
        let kind = match symbol.kind {
            Some(SymbolType::Bool) => SymbolType::Bool,
            Some(SymbolType::Tristate) => SymbolType::Tristate,
            Some(_) | None => return Ok(false),
        };
        let current = self
            .values
            .get(target)
            .cloned()
            .unwrap_or_else(|| Value::default_for(kind));
        let raised = match kind {
            SymbolType::Bool => {
                if bound.is_enabled() {
                    Value::Bool(true)
                } else {
                    current.clone()
                }
            }
            SymbolType::Tristate => Value::Tristate(current.to_tristate().or(bound)),
            SymbolType::Int | SymbolType::Hex | SymbolType::String => current.clone(),
        };
        if raised == current {
            return Ok(false);
        }
        self.values.insert(target.to_string(), raised);
        Ok(true)
    }

    fn refresh_newly_visible_defaults(&mut self) -> Result<bool, DomainError> {
        let symbols: Vec<Symbol> = self.table.iter().cloned().collect();
        let mut changed = false;
        for symbol in symbols {
            if self.assignments.get(&symbol.name).is_some() {
                continue;
            }
            let Ok(kind) = symbol.require_type() else {
                continue;
            };
            if !self.dependencies_met(&symbol)? {
                continue;
            }
            let current = self
                .values
                .get(&symbol.name)
                .cloned()
                .unwrap_or_else(|| Value::default_for(kind));
            if current != Value::default_for(kind) {
                continue;
            }
            let desired = self.apply_defaults(&symbol, kind)?;
            if desired == current {
                continue;
            }
            if let Err(issue) = self.check_range(&symbol, &desired) {
                self.report.push(issue);
            }
            self.values.insert(symbol.name.clone(), desired);
            changed = true;
        }
        Ok(changed)
    }

    fn apply_choice_defaults(&mut self) -> Result<bool, DomainError> {
        let choices: Vec<ChoiceGroup> = self.table.choices().to_vec();
        let mut changed = false;
        for choice in choices {
            changed |= self.enable_choice_default(&choice)?;
        }
        Ok(changed)
    }

    fn enable_choice_default(&mut self, choice: &ChoiceGroup) -> Result<bool, DomainError> {
        if !self.choice_visible(choice)? {
            return Ok(false);
        }
        if self
            .choice_members(choice)
            .iter()
            .any(|(_, value)| value.is_enabled())
        {
            return Ok(false);
        }
        if choice.optional {
            return Ok(false);
        }
        if let Some(name) = self.choice_default_member(choice)? {
            return self.raise_target(&name, Tristate::Yes);
        }
        Ok(false)
    }

    fn choice_default_member(
        &mut self,
        choice: &ChoiceGroup,
    ) -> Result<Option<String>, DomainError> {
        for default in &choice.defaults {
            if !self.condition_ok(&default.condition)? {
                continue;
            }
            let name = match &default.value {
                Expression::Symbol(name) => name.clone(),
                other => self.eval_expr(other)?.as_string(),
            };
            if !choice.members.iter().any(|member| member == &name) {
                self.report.push(ValidationIssue {
                    kind: IssueKind::UnknownSymbol,
                    symbol: Some(name.clone()),
                    message: format!(
                        "choice `{}` defaults to `{}`, which is not a member. Change `default` to one of the choice members",
                        choice.display_name(),
                        crate::domain::config_ident(&name)
                    ),
                });
                return Ok(None);
            }
            if self.member_is_visible(&name)? {
                return Ok(Some(name));
            }
        }
        for name in &choice.members {
            if self.member_is_visible(name)? {
                return Ok(Some(name.clone()));
            }
        }
        Ok(None)
    }

    fn member_is_visible(&mut self, name: &str) -> Result<bool, DomainError> {
        let Some(symbol) = self.table.get(name).cloned() else {
            return Ok(false);
        };
        self.dependencies_met(&symbol)
    }

    fn enforce_choice_constraints(&mut self) -> Result<(), DomainError> {
        let choices: Vec<ChoiceGroup> = self.table.choices().to_vec();
        for choice in choices {
            self.check_choice(&choice)?;
        }
        Ok(())
    }

    fn check_choice(&mut self, choice: &ChoiceGroup) -> Result<(), DomainError> {
        if !self.choice_visible(choice)? {
            return Ok(());
        }
        if let Err(issue) = self.check_choice_member_types(choice) {
            self.report.push(issue);
            return Ok(());
        }
        let members = self.choice_members(choice);
        let yes: Vec<&str> = members
            .iter()
            .filter_map(|(name, value)| {
                if *value == Tristate::Yes {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect();
        let module: Vec<&str> = members
            .iter()
            .filter_map(|(name, value)| {
                if *value == Tristate::Module {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect();
        let kind = self.inferred_choice_kind(choice);
        if yes.len() > 1 {
            self.report.push(choice_conflict(
                choice,
                format!(
                    "choice `{}` has more than one member enabled ({}). Keep exactly one at `y` and set the others to `n` in the defconfig",
                    choice.display_name(),
                    join_idents(&yes)
                ),
            ));
            return Ok(());
        }
        if kind == Some(SymbolType::Tristate) && yes.len() == 1 && !module.is_empty() {
            self.report.push(choice_conflict(
                choice,
                format!(
                    "choice `{}` has `{}=y`, so {} cannot be `m`. Set those members to `n`, or set `{}=m`",
                    choice.display_name(),
                    crate::domain::config_ident(yes[0]),
                    join_idents(&module),
                    crate::domain::config_ident(yes[0])
                ),
            ));
            return Ok(());
        }
        let any_enabled = !yes.is_empty() || !module.is_empty();
        if !any_enabled && !choice.optional {
            self.report.push(choice_conflict(
                choice,
                format!(
                    "choice `{}` has no member enabled. Set one member to `y` in the defconfig, or mark the choice `optional`",
                    choice.display_name()
                ),
            ));
        }
        Ok(())
    }

    fn check_choice_member_types(&self, choice: &ChoiceGroup) -> Result<(), ValidationIssue> {
        let Some(kind) = self.inferred_choice_kind(choice) else {
            return Ok(());
        };
        if !matches!(kind, SymbolType::Bool | SymbolType::Tristate) {
            return Err(choice_conflict(
                choice,
                format!(
                    "choice `{}` has type {kind}; choices must be bool or tristate. Change the choice type in Kconfig",
                    choice.display_name()
                ),
            ));
        }
        for name in &choice.members {
            let Some(symbol) = self.table.get(name) else {
                continue;
            };
            let Some(member_kind) = symbol.kind else {
                continue;
            };
            let incompatible = match (kind, member_kind) {
                (SymbolType::Bool, SymbolType::Bool)
                | (SymbolType::Tristate, SymbolType::Bool)
                | (SymbolType::Tristate, SymbolType::Tristate) => false,
                (SymbolType::Bool, SymbolType::Tristate)
                | (_, SymbolType::Int | SymbolType::Hex | SymbolType::String)
                | (SymbolType::Int | SymbolType::Hex | SymbolType::String, _) => true,
            };
            if incompatible {
                return Err(ValidationIssue {
                    kind: IssueKind::TypeMismatch,
                    symbol: Some(name.clone()),
                    message: format!(
                        "choice `{}` is {kind} but member `{}` is {member_kind}. Give that member type {kind}, or change the choice type in Kconfig",
                        choice.display_name(),
                        crate::domain::config_ident(name)
                    ),
                });
            }
        }
        Ok(())
    }

    fn inferred_choice_kind(&self, choice: &ChoiceGroup) -> Option<SymbolType> {
        if let Some(kind) = choice.kind {
            return Some(kind);
        }
        choice
            .members
            .iter()
            .find_map(|name| self.table.get(name).and_then(|symbol| symbol.kind))
    }

    fn choice_visible(&mut self, choice: &ChoiceGroup) -> Result<bool, DomainError> {
        if choice.depends.is_empty() {
            return Ok(true);
        }
        let combined = Expression::and(choice.depends.clone());
        combined.check_depth(self.limits)?;
        Ok(self.eval_expr(&combined)?.to_tristate().is_enabled())
    }

    fn choice_members(&self, choice: &ChoiceGroup) -> Vec<(String, Tristate)> {
        choice
            .members
            .iter()
            .map(|name| {
                let value = self
                    .values
                    .get(name)
                    .map(Value::to_tristate)
                    .unwrap_or(Tristate::No);
                (name.clone(), value)
            })
            .collect()
    }

    fn classify_unmet_dependencies(&mut self) -> Result<(), DomainError> {
        let symbols: Vec<Symbol> = self.table.iter().cloned().collect();
        for symbol in symbols {
            let Some(value) = self.values.get(&symbol.name).cloned() else {
                continue;
            };
            if is_disabled(&value) {
                continue;
            }
            if self.dependencies_met(&symbol)? {
                continue;
            }
            let selectors = self.active_selectors(&symbol.name)?;
            let ident = crate::domain::config_ident(&symbol.name);
            let deps = depends_display(&symbol);
            let hint = unmet_fix_hint(&symbol, &ident);
            if !selectors.is_empty() {
                let listed = selectors
                    .iter()
                    .map(|name| format!("`{}`", crate::domain::config_ident(name)))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.report.push(ValidationIssue {
                    kind: IssueKind::UnmetDependency,
                    symbol: Some(symbol.name.clone()),
                    message: format!(
                        "`{ident}` was turned on by `select` from {listed}, but `{deps}` is not satisfied. {hint}, or remove the `select`"
                    ),
                });
            } else if let Some(assignment) = self.assignments.get(&symbol.name) {
                self.report.push(ValidationIssue {
                    kind: IssueKind::UnmetDependency,
                    symbol: Some(symbol.name.clone()),
                    message: format!(
                        "`{ident}={}` is not allowed because `{deps}` is not satisfied. {hint}",
                        assignment.raw
                    ),
                });
            } else {
                self.report.push(ValidationIssue {
                    kind: IssueKind::UnmetDependency,
                    symbol: Some(symbol.name.clone()),
                    message: format!("`{ident}` is enabled but `{deps}` is not satisfied. {hint}"),
                });
            }
        }
        Ok(())
    }

    fn active_selectors(&mut self, target: &str) -> Result<Vec<String>, DomainError> {
        let selectors: Vec<(String, Vec<ReverseDep>)> = self
            .table
            .iter()
            .map(|symbol| (symbol.name.clone(), symbol.selects.clone()))
            .collect();
        let mut names = Vec::new();
        for (name, selects) in selectors {
            let bound = match self.values.get(&name) {
                Some(value) => value.to_tristate(),
                None => continue,
            };
            if bound == Tristate::No {
                continue;
            }
            for dep in selects {
                if dep.symbol == target && self.condition_ok(&dep.condition)? {
                    names.push(name.clone());
                    break;
                }
            }
        }
        Ok(names)
    }

    fn condition_ok(&mut self, condition: &Option<Expression>) -> Result<bool, DomainError> {
        match condition {
            None => Ok(true),
            Some(expr) => Ok(self.eval_expr(expr)?.to_tristate().is_enabled()),
        }
    }
}

fn depends_display(symbol: &Symbol) -> String {
    if symbol.depends.is_empty() {
        return "none".into();
    }
    Expression::and(symbol.depends.clone()).config_display()
}

fn unmet_fix_hint(symbol: &Symbol, enabled_ident: &str) -> String {
    match Expression::and(symbol.depends.clone()) {
        Expression::Symbol(dep) => format!(
            "Set `{}=y` in the defconfig, or set `{enabled_ident}=n`",
            crate::domain::config_ident(&dep)
        ),
        Expression::Not(inner) => match inner.as_ref() {
            Expression::Symbol(dep) => format!(
                "Set `{}=n` in the defconfig, or set `{enabled_ident}=n`",
                crate::domain::config_ident(dep)
            ),
            other => format!(
                "Adjust the defconfig so `!{}` is satisfied, or set `{enabled_ident}=n`",
                other.config_display()
            ),
        },
        other => format!(
            "Adjust the defconfig so `{}` is satisfied, or set `{enabled_ident}=n`",
            other.config_display()
        ),
    }
}

fn format_cycle(cycle: &[String]) -> String {
    let path = cycle
        .iter()
        .map(|name| crate::domain::config_ident(name))
        .collect::<Vec<_>>()
        .join(" -> ");
    format!("cyclic dependency: {path}. Remove one `depends on` along that cycle in Kconfig")
}

fn choice_conflict(choice: &ChoiceGroup, message: String) -> ValidationIssue {
    ValidationIssue {
        kind: IssueKind::ChoiceConflict,
        symbol: choice.members.first().cloned(),
        message,
    }
}

fn join_idents(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("`{}`", crate::domain::config_ident(name)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_disabled(value: &Value) -> bool {
    !value.to_tristate().is_enabled()
}

fn tristate_const(name: &str) -> Option<Tristate> {
    match name {
        "y" | "Y" => Some(Tristate::Yes),
        "m" | "M" => Some(Tristate::Module),
        "n" | "N" => Some(Tristate::No),
        _ => None,
    }
}

fn expected_literal(kind: SymbolType) -> &'static str {
    match kind {
        SymbolType::Bool => "`y` or `n`",
        SymbolType::Tristate => "`y`, `m`, or `n`",
        SymbolType::Int => "a decimal integer (for example `32`)",
        SymbolType::Hex => "a hexadecimal value (for example `0x10`)",
        SymbolType::String => "a quoted string (for example `\"board\"`)",
    }
}

pub(crate) fn parse_assignment(
    symbol: &Symbol,
    raw: &str,
    limits: Limits,
) -> Result<Value, ValidationIssue> {
    let kind = symbol.require_type().map_err(|err| ValidationIssue {
        kind: IssueKind::MissingType,
        symbol: Some(symbol.name.clone()),
        message: err.to_string(),
    })?;
    let trimmed = raw.trim();
    if trimmed.len() > limits.max_string_value_len {
        return Err(ValidationIssue {
            kind: IssueKind::ParseError,
            symbol: Some(symbol.name.clone()),
            message: DomainError::string_value_too_long(&symbol.name, limits).to_string(),
        });
    }
    let parsed = match kind {
        SymbolType::Bool => parse_bool(trimmed).map(Value::Bool),
        SymbolType::Tristate => Tristate::parse(trimmed).map(Value::Tristate),
        SymbolType::Int => parse_number(trimmed).map(Value::Int),
        SymbolType::Hex => parse_hex(trimmed).map(Value::Hex),
        SymbolType::String => Some(Value::String(unquote(trimmed))),
    };
    parsed.ok_or_else(|| ValidationIssue {
        kind: IssueKind::TypeMismatch,
        symbol: Some(symbol.name.clone()),
        message: format!(
            "`{}={trimmed}` is not a valid {kind} value. Use {}",
            crate::domain::config_ident(&symbol.name),
            expected_literal(kind)
        ),
    })
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw {
        "y" | "Y" | "yes" | "true" | "1" => Some(true),
        "n" | "N" | "no" | "false" | "0" => Some(false),
        "m" | "M" => None,
        _ => None,
    }
}

fn parse_hex(raw: &str) -> Option<u64> {
    let t = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    u64::from_str_radix(t, 16).ok()
}

fn unquote(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        t.to_string()
    }
}

fn compare_values(left: &Value, op: CompareOp, right: &Value) -> Result<bool, DomainError> {
    if let (Ok(l), Ok(r)) = (left.as_int(), right.as_int()) {
        return Ok(cmp_ord(l, op, r));
    }
    let l = left.as_string();
    let r = right.as_string();
    Ok(match op {
        CompareOp::Equal => l == r,
        CompareOp::NotEqual => l != r,
        CompareOp::Less => l < r,
        CompareOp::LessOrEqual => l <= r,
        CompareOp::Greater => l > r,
        CompareOp::GreaterOrEqual => l >= r,
    })
}

fn cmp_ord<T: Ord>(left: T, op: CompareOp, right: T) -> bool {
    match op {
        CompareOp::Equal => left == right,
        CompareOp::NotEqual => left != right,
        CompareOp::Less => left < right,
        CompareOp::LessOrEqual => left <= right,
        CompareOp::Greater => left > right,
        CompareOp::GreaterOrEqual => left >= right,
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate, parse_assignment};
    use crate::domain::{
        Assignment, AssignmentSet, ChoiceGroup, DefaultValue, Expression, Limits, RangeBound,
        ReverseDep, Symbol, SymbolTable, SymbolType, Tristate, Value, ValueRange,
    };

    fn table_with(symbols: Vec<Symbol>) -> SymbolTable {
        let limits = Limits::default();
        let mut table = SymbolTable::new();
        for s in symbols {
            table.insert(s, limits).unwrap();
        }
        table
    }

    #[test]
    fn applies_first_visible_default() {
        let limits = Limits::default();
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Bool(true)),
            condition: None,
        });
        let table = table_with(vec![foo]);
        let result = evaluate(table, &AssignmentSet::new(), limits).unwrap();
        assert_eq!(result.get("FOO"), Some(&Value::Bool(true)));
    }

    #[test]
    fn user_assignment_overrides_default() {
        let limits = Limits::default();
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Bool(true)),
            condition: None,
        });
        let table = table_with(vec![foo]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "FOO".into(),
                    raw: "n".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        let result = evaluate(table, &assignments, limits).unwrap();
        assert_eq!(result.get("FOO"), Some(&Value::Bool(false)));
    }

    #[test]
    fn and_or_not_evaluation() {
        let limits = Limits::default();
        let a = Symbol::new("A", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let b = Symbol::new("B", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut c = Symbol::new("C", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        c.defaults.push(DefaultValue {
            value: Expression::And(vec![
                Expression::symbol("A"),
                Expression::Not(Box::new(Expression::symbol("B"))),
            ]),
            condition: None,
        });
        let table = table_with(vec![a, b, c]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "A".into(),
                    raw: "y".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        assignments
            .push(
                Assignment {
                    name: "B".into(),
                    raw: "n".into(),
                    line: 2,
                },
                limits,
            )
            .unwrap();
        let result = evaluate(table, &assignments, limits).unwrap();
        assert_eq!(result.get("C"), Some(&Value::Bool(true)));
    }

    #[test]
    fn unmet_dependency_rejects_user_value() {
        let limits = Limits::default();
        let dep = Symbol::new("DEP", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.depends.push(Expression::symbol("DEP"));
        let table = table_with(vec![dep, foo]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "DEP".into(),
                    raw: "n".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        assignments
            .push(
                Assignment {
                    name: "FOO".into(),
                    raw: "y".into(),
                    line: 2,
                },
                limits,
            )
            .unwrap();
        let err = evaluate(table, &assignments, limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnmetDependency));
    }

    #[test]
    fn unmet_dependency_skips_range_on_forced_off_int() {
        let limits = Limits::default();
        let dep = Symbol::new("DEP", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut size = Symbol::new("SIZE", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        size.depends.push(Expression::symbol("DEP"));
        size.ranges.push(ValueRange {
            min: RangeBound::Number(8),
            max: RangeBound::Number(64),
            condition: None,
        });
        let table = table_with(vec![dep, size]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "DEP".into(),
                    raw: "n".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        let result = evaluate(table, &assignments, limits).unwrap();
        assert_eq!(result.get("SIZE"), Some(&Value::Int(0)));
    }

    #[test]
    fn range_violation_is_reported() {
        let limits = Limits::default();
        let mut size = Symbol::new("SIZE", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        size.ranges.push(ValueRange {
            min: RangeBound::Number(1),
            max: RangeBound::Number(8),
            condition: None,
        });
        let table = table_with(vec![size]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "SIZE".into(),
                    raw: "99".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        let err = evaluate(table, &assignments, limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::OutOfRange));
    }

    #[test]
    fn unknown_symbol_is_reported() {
        let limits = Limits::default();
        let table = table_with(vec![
            Symbol::new("FOO", limits)
                .unwrap()
                .with_type(SymbolType::Bool),
        ]);
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: "NOPE".into(),
                    raw: "y".into(),
                    line: 1,
                },
                limits,
            )
            .unwrap();
        let err = evaluate(table, &assignments, limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnknownSymbol));
    }

    #[test]
    fn type_mismatch_bool_rejects_string() {
        let limits = Limits::default();
        let foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let err = parse_assignment(&foo, "hello", limits).unwrap_err();
        assert_eq!(err.kind, crate::domain::IssueKind::TypeMismatch);
        assert!(err.message.contains("Use `y` or `n`"), "{}", err.message);
    }

    #[test]
    fn comparison_of_integers() {
        let limits = Limits::default();
        let mut size = Symbol::new("SIZE", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        size.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Int(4)),
            condition: None,
        });
        let mut flag = Symbol::new("FLAG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        flag.defaults.push(DefaultValue {
            value: Expression::Compare {
                left: Box::new(Expression::symbol("SIZE")),
                op: crate::domain::CompareOp::Greater,
                right: Box::new(Expression::Constant(Value::Int(2))),
            },
            condition: None,
        });
        let table = table_with(vec![size, flag]);
        let result = evaluate(table, &AssignmentSet::new(), limits).unwrap();
        assert_eq!(result.get("FLAG"), Some(&Value::Bool(true)));
    }

    #[test]
    fn tristate_and_or() {
        assert_eq!(Tristate::Yes.and(Tristate::Module), Tristate::Module);
        assert_eq!(Tristate::No.or(Tristate::Module), Tristate::Module);
    }

    fn assign(name: &str, raw: &str, line: u32, limits: Limits) -> AssignmentSet {
        let mut assignments = AssignmentSet::new();
        assignments
            .push(
                Assignment {
                    name: name.into(),
                    raw: raw.into(),
                    line,
                },
                limits,
            )
            .unwrap();
        assignments
    }

    fn assign_many(pairs: &[(&str, &str)], limits: Limits) -> AssignmentSet {
        let mut assignments = AssignmentSet::new();
        for (i, (name, raw)) in pairs.iter().enumerate() {
            assignments
                .push(
                    Assignment {
                        name: (*name).into(),
                        raw: (*raw).into(),
                        line: u32::try_from(i + 1).expect("test assignment count fits u32"),
                    },
                    limits,
                )
                .unwrap();
        }
        assignments
    }

    #[test]
    fn select_raises_hidden_helper() {
        let limits = Limits::default();
        let has_uart = Symbol::new("HAS_UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut uart = Symbol::new("UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        uart.selects.push(ReverseDep::always("HAS_UART"));
        let table = table_with(vec![has_uart, uart]);
        let result = evaluate(table, &assign("UART", "y", 1, limits), limits).unwrap();
        assert_eq!(result.get("UART"), Some(&Value::Bool(true)));
        assert_eq!(result.get("HAS_UART"), Some(&Value::Bool(true)));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn select_overrides_user_n() {
        let limits = Limits::default();
        let mut has_uart = Symbol::new("HAS_UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        has_uart.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Bool(false)),
            condition: None,
        });
        let mut uart = Symbol::new("UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        uart.selects.push(ReverseDep::always("HAS_UART"));
        let table = table_with(vec![has_uart, uart]);
        let result = evaluate(
            table,
            &assign_many(&[("UART", "y"), ("HAS_UART", "n")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(result.get("HAS_UART"), Some(&Value::Bool(true)));
    }

    #[test]
    fn select_raises_when_target_dependencies_are_met() {
        let limits = Limits::default();
        let bus = Symbol::new("BUS", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut has_dma = Symbol::new("HAS_DMA", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        has_dma.depends.push(Expression::symbol("BUS"));
        let mut driver = Symbol::new("DRIVER", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        driver.selects.push(ReverseDep::always("HAS_DMA"));
        let table = table_with(vec![bus, has_dma, driver]);
        let result = evaluate(
            table,
            &assign_many(&[("DRIVER", "y"), ("BUS", "y")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(result.get("HAS_DMA"), Some(&Value::Bool(true)));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn select_unmet_target_deps_is_an_error() {
        let limits = Limits::default();
        let bus = Symbol::new("BUS", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut has_dma = Symbol::new("HAS_DMA", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        has_dma.depends.push(Expression::symbol("BUS"));
        let mut driver = Symbol::new("DRIVER", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        driver.selects.push(ReverseDep::always("HAS_DMA"));
        let table = table_with(vec![bus, has_dma, driver]);
        let err = evaluate(
            table,
            &assign_many(&[("DRIVER", "y"), ("BUS", "n")], limits),
            limits,
        )
        .unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnmetDependency));
        let text = err.to_string();
        assert!(text.contains("`select` from `CONFIG_DRIVER`"), "{text}");
        assert!(text.contains("CONFIG_BUS"), "{text}");
        assert!(text.contains("Set `CONFIG_BUS=y`"), "{text}");
        assert!(!err.has_warning_kind(crate::domain::IssueKind::UnmetDependency));
    }

    #[test]
    fn user_enabled_unmet_without_select_is_still_an_error() {
        let limits = Limits::default();
        let dep = Symbol::new("DEP", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.depends.push(Expression::symbol("DEP"));
        let table = table_with(vec![dep, foo]);
        let err = evaluate(
            table,
            &assign_many(&[("DEP", "n"), ("FOO", "y")], limits),
            limits,
        )
        .unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnmetDependency));
        let text = err.to_string();
        assert!(text.contains("`CONFIG_FOO=y`"), "{text}");
        assert!(text.contains("CONFIG_DEP"), "{text}");
        assert!(text.contains("Set `CONFIG_DEP=y`"), "{text}");
        assert!(!err.has_warning_kind(crate::domain::IssueKind::UnmetDependency));
    }

    #[test]
    fn select_does_not_excuse_unmet_dependencies() {
        let limits = Limits::default();
        let dep = Symbol::new("DEP", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.depends.push(Expression::symbol("DEP"));
        let mut bar = Symbol::new("BAR", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        bar.selects.push(ReverseDep::always("FOO"));
        let table = table_with(vec![dep, foo, bar]);
        let err = evaluate(
            table,
            &assign_many(&[("DEP", "n"), ("FOO", "y"), ("BAR", "y")], limits),
            limits,
        )
        .unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnmetDependency));
        assert!(!err.has_warning_kind(crate::domain::IssueKind::UnmetDependency));
    }

    #[test]
    fn select_if_is_respected() {
        let limits = Limits::default();
        let has_uart = Symbol::new("HAS_UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let pinmux = Symbol::new("PINMUX", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut uart = Symbol::new("UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        uart.selects.push(ReverseDep {
            symbol: "HAS_UART".into(),
            condition: Some(Expression::symbol("PINMUX")),
        });
        let table = table_with(vec![has_uart, pinmux, uart]);
        let off = evaluate(
            table.clone(),
            &assign_many(&[("UART", "y"), ("PINMUX", "n")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(off.get("HAS_UART"), Some(&Value::Bool(false)));
        let on = evaluate(
            table,
            &assign_many(&[("UART", "y"), ("PINMUX", "y")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(on.get("HAS_UART"), Some(&Value::Bool(true)));
    }

    #[test]
    fn select_cascades_through_another_select() {
        let limits = Limits::default();
        let serial = Symbol::new("HAS_SERIAL", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut uart = Symbol::new("HAS_UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        uart.selects.push(ReverseDep::always("HAS_SERIAL"));
        let mut driver = Symbol::new("UART_FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        driver.selects.push(ReverseDep::always("HAS_UART"));
        let table = table_with(vec![serial, uart, driver]);
        let result = evaluate(table, &assign("UART_FOO", "y", 1, limits), limits).unwrap();
        assert_eq!(result.get("HAS_UART"), Some(&Value::Bool(true)));
        assert_eq!(result.get("HAS_SERIAL"), Some(&Value::Bool(true)));
    }

    #[test]
    fn tristate_select_takes_max_and_promotes_bool() {
        let limits = Limits::default();
        let helper = Symbol::new("HELPER", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        let flag = Symbol::new("FLAG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut low = Symbol::new("LOW", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        let mut high = Symbol::new("HIGH", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        low.selects.push(ReverseDep::always("HELPER"));
        low.selects.push(ReverseDep::always("FLAG"));
        high.selects.push(ReverseDep::always("HELPER"));
        let table = table_with(vec![helper, flag, low, high]);
        let result = evaluate(
            table,
            &assign_many(&[("LOW", "m"), ("HIGH", "y")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(result.get("HELPER"), Some(&Value::Tristate(Tristate::Yes)));
        assert_eq!(result.get("FLAG"), Some(&Value::Bool(true)));
    }

    #[test]
    fn select_unknown_symbol_is_an_error() {
        let limits = Limits::default();
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.selects.push(ReverseDep::always("MISSING"));
        let table = table_with(vec![foo]);
        let err = evaluate(table, &assign("FOO", "y", 1, limits), limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::UnknownSymbol));
        assert!(err.to_string().contains("MISSING"));
    }

    #[test]
    fn select_of_int_is_an_error() {
        let limits = Limits::default();
        let size = Symbol::new("SIZE", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        foo.selects.push(ReverseDep::always("SIZE"));
        let table = table_with(vec![size, foo]);
        let err = evaluate(table, &AssignmentSet::new(), limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::TypeMismatch));
        assert!(err.to_string().contains("SIZE"));
    }

    #[test]
    fn imply_raises_when_deps_met_and_user_did_not_assign() {
        let limits = Limits::default();
        let log = Symbol::new("LOG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut debug = Symbol::new("DEBUG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        debug.implies.push(ReverseDep::always("LOG"));
        let table = table_with(vec![log, debug]);
        let result = evaluate(table, &assign("DEBUG", "y", 1, limits), limits).unwrap();
        assert_eq!(result.get("LOG"), Some(&Value::Bool(true)));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn imply_respects_user_n() {
        let limits = Limits::default();
        let log = Symbol::new("LOG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut debug = Symbol::new("DEBUG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        debug.implies.push(ReverseDep::always("LOG"));
        let table = table_with(vec![log, debug]);
        let result = evaluate(
            table,
            &assign_many(&[("DEBUG", "y"), ("LOG", "n")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(result.get("LOG"), Some(&Value::Bool(false)));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn imply_does_not_bypass_target_dependencies() {
        let limits = Limits::default();
        let bus = Symbol::new("BUS", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut log = Symbol::new("LOG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        log.depends.push(Expression::symbol("BUS"));
        let mut debug = Symbol::new("DEBUG", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        debug.implies.push(ReverseDep::always("LOG"));
        let table = table_with(vec![bus, log, debug]);
        let result = evaluate(
            table,
            &assign_many(&[("DEBUG", "y"), ("BUS", "n")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(result.get("LOG"), Some(&Value::Bool(false)));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn select_of_dependency_makes_int_default_visible() {
        let limits = Limits::default();
        let net = Symbol::new("NET", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut size = Symbol::new("SIZE", limits)
            .unwrap()
            .with_type(SymbolType::Int);
        size.depends.push(Expression::symbol("NET"));
        size.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Int(128)),
            condition: None,
        });
        size.ranges.push(ValueRange {
            min: RangeBound::Number(8),
            max: RangeBound::Number(4096),
            condition: None,
        });
        let mut driver = Symbol::new("DRIVER", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        driver.selects.push(ReverseDep::always("NET"));
        let table = table_with(vec![net, size, driver]);
        let result = evaluate(table, &assign("DRIVER", "y", 1, limits), limits).unwrap();
        assert_eq!(result.get("NET"), Some(&Value::Bool(true)));
        assert_eq!(result.get("SIZE"), Some(&Value::Int(128)));
    }

    #[test]
    fn cyclic_direct_depends_is_an_error() {
        let limits = Limits::default();
        let mut a = Symbol::new("A", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        a.depends.push(Expression::symbol("B"));
        a.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Bool(true)),
            condition: None,
        });
        let mut b = Symbol::new("B", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        b.depends.push(Expression::symbol("A"));
        b.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Bool(true)),
            condition: None,
        });
        let err = evaluate(table_with(vec![a, b]), &AssignmentSet::new(), limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::CyclicDependency));
        assert!(err.to_string().contains("cyclic dependency"));
    }

    #[test]
    fn tristate_depends_is_an_upper_bound() {
        let limits = Limits::default();
        let dep = Symbol::new("DEP", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        let mut foo = Symbol::new("FOO", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        foo.depends.push(Expression::symbol("DEP"));
        foo.defaults.push(DefaultValue {
            value: Expression::Constant(Value::Tristate(Tristate::Yes)),
            condition: None,
        });
        let table = table_with(vec![dep, foo]);
        let result = evaluate(table, &assign("DEP", "m", 1, limits), limits).unwrap();
        assert_eq!(result.get("FOO"), Some(&Value::Tristate(Tristate::Module)));
    }

    fn console_choice(limits: Limits) -> (SymbolTable, ChoiceGroup) {
        let uart = Symbol::new("UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let rtt = Symbol::new("RTT", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut table = table_with(vec![uart, rtt]);
        let mut choice = ChoiceGroup::new("console", vec!["UART".into(), "RTT".into()])
            .with_type(SymbolType::Bool);
        choice.prompt = Some("Console".into());
        choice.defaults.push(DefaultValue {
            value: Expression::symbol("UART"),
            condition: None,
        });
        table.push_choice(choice.clone());
        (table, choice)
    }

    #[test]
    fn choice_enables_its_default_member() {
        let limits = Limits::default();
        let (table, _) = console_choice(limits);
        let result = evaluate(table, &AssignmentSet::new(), limits).unwrap();
        assert_eq!(result.get("UART"), Some(&Value::Bool(true)));
        assert_eq!(result.get("RTT"), Some(&Value::Bool(false)));
    }

    #[test]
    fn choice_rejects_two_selected_members() {
        let limits = Limits::default();
        let (table, _) = console_choice(limits);
        let err = evaluate(
            table,
            &assign_many(&[("UART", "y"), ("RTT", "y")], limits),
            limits,
        )
        .unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::ChoiceConflict));
        assert!(err.to_string().contains("more than one member enabled"));
    }

    #[test]
    fn optional_choice_allows_no_member() {
        let limits = Limits::default();
        let uart = Symbol::new("UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let rtt = Symbol::new("RTT", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut table = table_with(vec![uart, rtt]);
        table.push_choice(
            ChoiceGroup::new("console", vec!["UART".into(), "RTT".into()])
                .with_type(SymbolType::Bool)
                .optional(),
        );
        let result = evaluate(table, &AssignmentSet::new(), limits).unwrap();
        assert_eq!(result.get("UART"), Some(&Value::Bool(false)));
        assert_eq!(result.get("RTT"), Some(&Value::Bool(false)));
    }

    #[test]
    fn user_choice_member_overrides_default() {
        let limits = Limits::default();
        let (table, _) = console_choice(limits);
        let result = evaluate(table, &assign("RTT", "y", 1, limits), limits).unwrap();
        assert_eq!(result.get("UART"), Some(&Value::Bool(false)));
        assert_eq!(result.get("RTT"), Some(&Value::Bool(true)));
    }

    #[test]
    fn tristate_choice_allows_multiple_modules_but_not_y_and_m() {
        let limits = Limits::default();
        let a = Symbol::new("A", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        let b = Symbol::new("B", limits)
            .unwrap()
            .with_type(SymbolType::Tristate);
        let mut table = table_with(vec![a, b]);
        table.push_choice(
            ChoiceGroup::new("drivers", vec!["A".into(), "B".into()])
                .with_type(SymbolType::Tristate),
        );
        let ok = evaluate(
            table.clone(),
            &assign_many(&[("A", "m"), ("B", "m")], limits),
            limits,
        )
        .unwrap();
        assert_eq!(ok.get("A"), Some(&Value::Tristate(Tristate::Module)));
        assert_eq!(ok.get("B"), Some(&Value::Tristate(Tristate::Module)));
        let err = evaluate(
            table,
            &assign_many(&[("A", "y"), ("B", "m")], limits),
            limits,
        )
        .unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::ChoiceConflict));
    }

    #[test]
    fn reverse_dep_iteration_limit_is_enforced() {
        let limits = Limits {
            max_resolution_iterations: 1,
            ..Limits::default()
        };
        let serial = Symbol::new("HAS_SERIAL", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        let mut uart = Symbol::new("HAS_UART", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        uart.selects.push(ReverseDep::always("HAS_SERIAL"));
        let mut driver = Symbol::new("UART_FOO", limits)
            .unwrap()
            .with_type(SymbolType::Bool);
        driver.selects.push(ReverseDep::always("HAS_UART"));
        let table = table_with(vec![serial, uart, driver]);
        let err = evaluate(table, &assign("UART_FOO", "y", 1, limits), limits).unwrap_err();
        assert!(err.has_kind(crate::domain::IssueKind::LimitExceeded));
    }
}
