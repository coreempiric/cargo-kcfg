use crate::domain::{
    ChoiceGroup, CompareOp, DefaultValue, DomainError, Expression, Limits, RangeBound, ReverseDep,
    Symbol, SymbolTable, SymbolType, Tristate, Value, ValueRange,
};
use crate::error::Error;
use crate::infra::defconfig::check_file_size;
use nom_kconfig::attribute::expression::{
    AndExpression, Atom, CompareExpression, CompareOperand, CompareOperator, OrExpression, Term,
};
use nom_kconfig::attribute::r#macro::Macro;
use nom_kconfig::attribute::r#type::Type;
use nom_kconfig::attribute::{Attribute, Expression as NomExpression};
use nom_kconfig::entry::config::Config;
use nom_kconfig::entry::source::Source;
use nom_kconfig::symbol::{ConstantSymbol, Symbol as NomSymbol};
use nom_kconfig::tristate::Tristate as NomTristate;
use nom_kconfig::{Entry, Kconfig, KconfigFile, KconfigInput, parse_kconfig};
use std::path::{Path, PathBuf};

/// Result of loading a Kconfig tree by following `source` directives.
#[derive(Debug, Clone)]
pub struct LoadedKconfig {
    pub table: SymbolTable,
    /// Files actually visited while walking the `source` graph.
    pub loaded_files: Vec<PathBuf>,
}

/// Parse `root_kconfig` with `nom-kconfig` and convert the AST into a symbol table.
///
/// `source` entries are followed by the parser (and then walked here). The
/// filesystem is never crawled for `Kconfig*` names.
pub fn load_kconfig(
    root_dir: &Path,
    kconfig: &Path,
    limits: Limits,
) -> Result<LoadedKconfig, Error> {
    check_file_size(kconfig, limits)?;
    let relative = relative_to_root(root_dir, kconfig);
    let kconfig_file = KconfigFile::new(root_dir.to_path_buf(), relative);
    let content = kconfig_file
        .read_to_string()
        .map_err(|e| Error::io(kconfig, e))?;
    if content.len() as u64 > limits.max_file_bytes {
        return Err(DomainError::FileTooLarge {
            path: kconfig.display().to_string(),
            size: content.len() as u64,
            max: limits.max_file_bytes,
        }
        .into());
    }
    let input = KconfigInput::new_extra(&content, kconfig_file);
    let (remaining, parsed) = parse_kconfig(input).map_err(|e| {
        Error::Parse(format!(
            "could not parse `{}`: {e}. Fix the Kconfig syntax and try again",
            kconfig.display()
        ))
    })?;
    let leftover = remaining.fragment().trim();
    if !leftover.is_empty() {
        return Err(Error::Parse(format!(
            "unparsed trailing input in `{}` (shown below). Fix the Kconfig syntax near that text:\n{}",
            kconfig.display(),
            leftover.chars().take(200).collect::<String>()
        )));
    }

    let mut table = SymbolTable::new();
    let mut loaded_files = Vec::new();
    let mut walker = Walker {
        table: &mut table,
        loaded_files: &mut loaded_files,
        limits,
    };
    walker.walk_kconfig(&parsed, 0, Vec::new())?;
    Ok(LoadedKconfig {
        table,
        loaded_files,
    })
}

fn relative_to_root(root: &Path, file: &Path) -> PathBuf {
    file.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| file.to_path_buf())
}

struct Walker<'a> {
    table: &'a mut SymbolTable,
    loaded_files: &'a mut Vec<PathBuf>,
    limits: Limits,
}

impl Walker<'_> {
    fn walk_kconfig(
        &mut self,
        kconfig: &Kconfig,
        source_depth: usize,
        inherited: Vec<Expression>,
    ) -> Result<(), Error> {
        if source_depth > self.limits.max_source_depth {
            return Err(DomainError::SourceDepthExceeded {
                max: self.limits.max_source_depth,
            }
            .into());
        }
        self.loaded_files.push(PathBuf::from(&kconfig.file));
        self.walk_entries(&kconfig.entries, source_depth, inherited)
    }

    fn walk_entries(
        &mut self,
        entries: &[Entry],
        source_depth: usize,
        inherited: Vec<Expression>,
    ) -> Result<(), Error> {
        for entry in entries {
            self.walk_entry(entry, source_depth, inherited.clone())?;
        }
        Ok(())
    }

    fn walk_entry(
        &mut self,
        entry: &Entry,
        source_depth: usize,
        inherited: Vec<Expression>,
    ) -> Result<(), Error> {
        match entry {
            Entry::Config(config) | Entry::MenuConfig(config) => {
                self.add_config(config, &inherited)
            }
            Entry::Choice(choice) => self.walk_choice(choice, source_depth, inherited),
            Entry::Menu(menu) => {
                let mut extra = Vec::new();
                for dep in &menu.depends_on {
                    extra.push(convert_expr(&dep.expression, 0, self.limits)?);
                }
                if let Some(Some(visible)) = &menu.visible {
                    extra.push(convert_expr(visible, 0, self.limits)?);
                }
                self.walk_entries(
                    &menu.entries,
                    source_depth,
                    concat_depends(&inherited, extra),
                )
            }
            Entry::If(if_entry) => {
                let cond = convert_expr(&if_entry.condition, 0, self.limits)?;
                let mut extra = inherited;
                extra.push(cond);
                self.walk_entries(&if_entry.entries, source_depth, extra)
            }
            Entry::Source(source) => self.walk_source(source, source_depth, inherited),
            Entry::OSource(source) | Entry::RSource(source) | Entry::OrSource(source) => {
                self.walk_source(source, source_depth, inherited)
            }
            Entry::ConfigDefault(def) => {
                let mut symbol = Symbol::new(&def.symbol, self.limits)?;
                symbol.depends.extend(inherited);
                for d in &def.default_attributes {
                    symbol.defaults.push(DefaultValue {
                        value: convert_expr(&d.expression, 0, self.limits)?,
                        condition: match &d.r#if {
                            Some(c) => Some(convert_expr(c, 0, self.limits)?),
                            None => None,
                        },
                    });
                }
                self.table.insert(symbol, self.limits)?;
                Ok(())
            }
            Entry::Comment(_)
            | Entry::MainMenu(_)
            | Entry::VariableAssignment(_)
            | Entry::FunctionCall(_)
            | Entry::Function(_) => Ok(()),
        }
    }

    fn walk_source(
        &mut self,
        source: &Source,
        source_depth: usize,
        inherited: Vec<Expression>,
    ) -> Result<(), Error> {
        let next = source_depth
            .checked_add(1)
            .ok_or(DomainError::SourceDepthExceeded {
                max: self.limits.max_source_depth,
            })?;
        for child in &source.kconfigs {
            self.walk_kconfig(child, next, inherited.clone())?;
        }
        Ok(())
    }

    fn add_config(&mut self, config: &Config, inherited: &[Expression]) -> Result<(), Error> {
        let mut symbol = convert_config(config, self.limits)?;
        symbol.depends.extend(inherited.iter().cloned());
        self.table.insert(symbol, self.limits)?;
        Ok(())
    }

    fn walk_choice(
        &mut self,
        choice: &nom_kconfig::entry::choice::Choice,
        source_depth: usize,
        inherited: Vec<Expression>,
    ) -> Result<(), Error> {
        let members = collect_choice_members(&choice.entries)?;
        let extra = choice_visibility(&choice.options, self.limits)?;
        let inherited_for_members = concat_depends(&inherited, extra.clone());
        self.walk_entries(&choice.entries, source_depth, inherited_for_members)?;
        let group = convert_choice(
            choice,
            concat_depends(&inherited, extra),
            members,
            self.limits,
        )?;
        self.table.push_choice(group);
        Ok(())
    }
}

fn collect_choice_members(entries: &[Entry]) -> Result<Vec<String>, Error> {
    let mut members = Vec::new();
    collect_choice_members_into(entries, &mut members)?;
    Ok(members)
}

fn collect_choice_members_into(entries: &[Entry], out: &mut Vec<String>) -> Result<(), Error> {
    for entry in entries {
        match entry {
            Entry::Config(config) | Entry::MenuConfig(config) => {
                out.push(config.symbol.clone());
            }
            Entry::If(if_entry) => collect_choice_members_into(&if_entry.entries, out)?,
            Entry::Menu(menu) => collect_choice_members_into(&menu.entries, out)?,
            Entry::Choice(_) => {
                return Err(Error::Parse(
                    "nested `choice` is not supported. Flatten the inner choice into the outer one, or move it out"
                        .into(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn choice_visibility(attrs: &[Attribute], limits: Limits) -> Result<Vec<Expression>, Error> {
    let mut depends = Vec::new();
    for attr in attrs {
        match attr {
            Attribute::DependsOn(dep) => depends.push(convert_depends(dep, limits)?),
            Attribute::Visible(Some(expr)) | Attribute::Requires(expr) => {
                depends.push(convert_expr(expr, 0, limits)?);
            }
            Attribute::Prompt(prompt) => {
                if let Some(cond) = &prompt.r#if {
                    depends.push(convert_expr(cond, 0, limits)?);
                }
            }
            Attribute::Type(config_type) => {
                if let Some(cond) = &config_type.r#if {
                    depends.push(convert_expr(cond, 0, limits)?);
                }
            }
            _ => {}
        }
    }
    Ok(depends)
}

fn convert_choice(
    choice: &nom_kconfig::entry::choice::Choice,
    depends: Vec<Expression>,
    members: Vec<String>,
    limits: Limits,
) -> Result<ChoiceGroup, Error> {
    let mut group = ChoiceGroup {
        id: String::new(),
        prompt: None,
        kind: None,
        optional: false,
        defaults: Vec::new(),
        depends,
        members,
    };
    for attr in &choice.options {
        match attr {
            Attribute::Type(config_type) => {
                apply_choice_type(&mut group, &config_type.r#type)?;
            }
            Attribute::Default(def) => {
                group.defaults.push(DefaultValue {
                    value: convert_expr(&def.expression, 0, limits)?,
                    condition: match &def.r#if {
                        Some(c) => Some(convert_expr(c, 0, limits)?),
                        None => None,
                    },
                });
            }
            Attribute::Prompt(prompt) => {
                if group.prompt.is_none() {
                    group.prompt = Some(prompt.prompt.clone());
                }
            }
            Attribute::Optional => group.optional = true,
            Attribute::Help(_)
            | Attribute::DependsOn(_)
            | Attribute::Visible(_)
            | Attribute::Requires(_)
            | Attribute::Select(_)
            | Attribute::Imply(_)
            | Attribute::Range(_)
            | Attribute::Modules
            | Attribute::Transitional
            | Attribute::Option(_) => {}
        }
    }
    Ok(group)
}

fn apply_choice_type(group: &mut ChoiceGroup, ty: &Type) -> Result<(), Error> {
    let (kind, prompt) = match ty {
        Type::Bool(p) => (SymbolType::Bool, p.clone()),
        Type::Tristate(p) => (SymbolType::Tristate, p.clone()),
        other => {
            return Err(Error::Parse(format!(
                "choice `{}` has type {other:?}; choices must be bool or tristate",
                group.display_name()
            )));
        }
    };
    if let Some(existing) = group.kind
        && existing != kind
    {
        return Err(DomainError::ConflictingType {
            name: group.display_name().to_string(),
            first: existing.to_string(),
            second: kind.to_string(),
        }
        .into());
    }
    group.kind = Some(kind);
    if group.prompt.is_none() {
        group.prompt = prompt;
    }
    Ok(())
}

fn concat_depends(base: &[Expression], extra: Vec<Expression>) -> Vec<Expression> {
    let mut out = base.to_vec();
    out.extend(extra);
    out
}

fn convert_config(config: &Config, limits: Limits) -> Result<Symbol, Error> {
    let mut symbol = Symbol::new(&config.symbol, limits)?;
    for attr in &config.attributes {
        apply_attribute(&mut symbol, attr, limits)?;
    }
    Ok(symbol)
}

fn apply_attribute(symbol: &mut Symbol, attr: &Attribute, limits: Limits) -> Result<(), Error> {
    match attr {
        Attribute::Type(config_type) => {
            apply_type(symbol, &config_type.r#type, limits)?;
            if let Some(cond) = &config_type.r#if {
                symbol.depends.push(convert_expr(cond, 0, limits)?);
            }
        }
        Attribute::Default(def) => {
            symbol.defaults.push(DefaultValue {
                value: convert_expr(&def.expression, 0, limits)?,
                condition: match &def.r#if {
                    Some(c) => Some(convert_expr(c, 0, limits)?),
                    None => None,
                },
            });
        }
        Attribute::DependsOn(dep) => {
            symbol.depends.push(convert_depends(dep, limits)?);
        }
        Attribute::Range(range) => {
            symbol.ranges.push(ValueRange {
                min: convert_range_bound(&range.lower_bound),
                max: convert_range_bound(&range.upper_bound),
                condition: match &range.r#if {
                    Some(c) => Some(convert_expr(c, 0, limits)?),
                    None => None,
                },
            });
        }
        Attribute::Help(help) => {
            if symbol.help.is_none() {
                symbol.help = Some(help.clone());
            }
        }
        Attribute::Prompt(prompt) => {
            if symbol.prompt.is_none() {
                symbol.prompt = Some(prompt.prompt.clone());
            }
            if let Some(cond) = &prompt.r#if {
                symbol.depends.push(convert_expr(cond, 0, limits)?);
            }
        }
        Attribute::Visible(Some(expr)) => {
            symbol.depends.push(convert_expr(expr, 0, limits)?);
        }
        Attribute::Requires(expr) => {
            symbol.depends.push(convert_expr(expr, 0, limits)?);
        }
        Attribute::Select(sel) => {
            symbol
                .selects
                .push(convert_reverse_dep(sel.symbol.clone(), &sel.r#if, limits)?);
        }
        Attribute::Imply(imp) => {
            symbol.implies.push(convert_reverse_dep(
                imply_target_name(&imp.symbol)?,
                &imp.r#if,
                limits,
            )?);
        }
        Attribute::Optional
        | Attribute::Modules
        | Attribute::Transitional
        | Attribute::Option(_)
        | Attribute::Visible(None) => {}
    }
    Ok(())
}

fn imply_target_name(symbol: &NomSymbol) -> Result<String, Error> {
    match symbol {
        NomSymbol::NonConstant(name) => Ok(name.clone()),
        NomSymbol::Constant(_) => Err(Error::Parse(
            "imply target must be a symbol name, not a constant. Write `imply FOO` with a defined bool or tristate".into(),
        )),
    }
}

fn convert_reverse_dep(
    symbol: String,
    condition: &Option<NomExpression>,
    limits: Limits,
) -> Result<ReverseDep, Error> {
    Ok(ReverseDep {
        symbol,
        condition: match condition {
            Some(expr) => Some(convert_expr(expr, 0, limits)?),
            None => None,
        },
    })
}

fn apply_type(symbol: &mut Symbol, ty: &Type, limits: Limits) -> Result<(), Error> {
    let (kind, prompt, default) = match ty {
        Type::Bool(p) => (SymbolType::Bool, p.clone(), None),
        Type::Tristate(p) => (SymbolType::Tristate, p.clone(), None),
        Type::String(p) => (SymbolType::String, p.clone(), None),
        Type::Hex(p) => (SymbolType::Hex, p.clone(), None),
        Type::Int(p) => (SymbolType::Int, p.clone(), None),
        Type::DefBool(expr) => (SymbolType::Bool, None, Some(convert_expr(expr, 0, limits)?)),
        Type::DefTristate(expr) => (
            SymbolType::Tristate,
            None,
            Some(convert_expr(expr, 0, limits)?),
        ),
        Type::DefInt(expr) => (SymbolType::Int, None, Some(convert_expr(expr, 0, limits)?)),
        Type::DefHex(expr) => (SymbolType::Hex, None, Some(convert_expr(expr, 0, limits)?)),
        Type::DefString(expr) => (
            SymbolType::String,
            None,
            Some(convert_expr(expr, 0, limits)?),
        ),
    };
    if let Some(existing) = symbol.kind {
        if existing != kind {
            return Err(DomainError::ConflictingType {
                name: symbol.name.clone(),
                first: existing.to_string(),
                second: kind.to_string(),
            }
            .into());
        }
    } else {
        symbol.kind = Some(kind);
    }
    if symbol.prompt.is_none() {
        symbol.prompt = prompt;
    }
    if let Some(value) = default {
        symbol.defaults.push(DefaultValue {
            value,
            condition: None,
        });
    }
    Ok(())
}

fn convert_depends(
    dep: &nom_kconfig::attribute::depends_on::DependsOn,
    limits: Limits,
) -> Result<Expression, Error> {
    let expr = convert_expr(&dep.expression, 0, limits)?;
    if let Some(cond) = &dep.r#if {
        Ok(Expression::and(vec![expr, convert_expr(cond, 0, limits)?]))
    } else {
        Ok(expr)
    }
}

fn convert_range_bound(bound: &nom_kconfig::attribute::range::RangeBound) -> RangeBound {
    match bound {
        nom_kconfig::attribute::range::RangeBound::Number(n) => RangeBound::Number(*n),
        nom_kconfig::attribute::range::RangeBound::Hex(s) => {
            let t = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            match i64::from_str_radix(t, 16) {
                Ok(n) => RangeBound::Number(n),
                Err(_) => RangeBound::Symbol(s.clone()),
            }
        }
        nom_kconfig::attribute::range::RangeBound::Symbol(s)
        | nom_kconfig::attribute::range::RangeBound::Variable(s) => {
            RangeBound::from_number_or_symbol(s)
        }
    }
}

fn convert_expr(expr: &NomExpression, depth: usize, limits: Limits) -> Result<Expression, Error> {
    if depth > limits.max_expression_depth {
        return Err(DomainError::ExpressionDepthExceeded {
            max: limits.max_expression_depth,
        }
        .into());
    }
    convert_or(expr, depth, limits)
}

fn convert_or(expr: &OrExpression, depth: usize, limits: Limits) -> Result<Expression, Error> {
    match expr {
        OrExpression::Term(and) => convert_and(and, depth, limits),
        OrExpression::Expression(parts) => {
            let converted = parts
                .iter()
                .map(|p| convert_and(p, depth + 1, limits))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::or(converted))
        }
    }
}

fn convert_and(expr: &AndExpression, depth: usize, limits: Limits) -> Result<Expression, Error> {
    match expr {
        AndExpression::Term(term) => convert_term(term, depth, limits),
        AndExpression::Expression(parts) => {
            let converted = parts
                .iter()
                .map(|p| convert_term(p, depth + 1, limits))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::and(converted))
        }
    }
}

fn convert_term(term: &Term, depth: usize, limits: Limits) -> Result<Expression, Error> {
    match term {
        Term::Atom(atom) => convert_atom(atom, depth, limits),
        Term::Not(atom) => Ok(Expression::Not(Box::new(convert_atom(
            atom,
            depth + 1,
            limits,
        )?))),
    }
}

fn convert_atom(atom: &Atom, depth: usize, limits: Limits) -> Result<Expression, Error> {
    match atom {
        Atom::Symbol(symbol) => convert_nom_symbol(symbol),
        Atom::Compare(cmp) => convert_compare(cmp, depth, limits),
        Atom::Macro(m) => convert_macro(m),
        Atom::Parenthesis(inner) => convert_expr(inner, depth + 1, limits),
    }
}

fn convert_compare(
    cmp: &CompareExpression,
    depth: usize,
    limits: Limits,
) -> Result<Expression, Error> {
    Ok(Expression::Compare {
        left: Box::new(convert_operand(&cmp.left, depth + 1, limits)?),
        op: convert_op(&cmp.operator),
        right: Box::new(convert_operand(&cmp.right, depth + 1, limits)?),
    })
}

fn convert_operand(
    operand: &CompareOperand,
    _depth: usize,
    _limits: Limits,
) -> Result<Expression, Error> {
    match operand {
        CompareOperand::Symbol(s) => convert_nom_symbol(s),
        CompareOperand::Macro(m) => convert_macro(m),
    }
}

fn convert_op(op: &CompareOperator) -> CompareOp {
    match op {
        CompareOperator::Equal => CompareOp::Equal,
        CompareOperator::NotEqual => CompareOp::NotEqual,
        CompareOperator::LowerThan => CompareOp::Less,
        CompareOperator::LowerOrEqual => CompareOp::LessOrEqual,
        CompareOperator::GreaterThan => CompareOp::Greater,
        CompareOperator::GreaterOrEqual => CompareOp::GreaterOrEqual,
    }
}

fn convert_nom_symbol(symbol: &NomSymbol) -> Result<Expression, Error> {
    match symbol {
        NomSymbol::NonConstant(name) => Ok(Expression::Symbol(name.clone())),
        NomSymbol::Constant(c) => Ok(Expression::Constant(convert_constant(c)?)),
    }
}

fn convert_constant(c: &ConstantSymbol) -> Result<Value, Error> {
    match c {
        ConstantSymbol::Integer(n) => Ok(Value::Int(*n)),
        ConstantSymbol::Hex(s) => {
            let t = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            u64::from_str_radix(t, 16)
                .map(Value::Hex)
                .map_err(|_| Error::Parse(format!("invalid hex constant `{s}`")))
        }
        ConstantSymbol::Boolean(b) => Ok(Value::Bool(*b)),
        ConstantSymbol::String(s) => Ok(Value::String(s.clone())),
        ConstantSymbol::Tristate(t) => Ok(Value::Tristate(convert_tristate(t))),
    }
}

fn convert_tristate(t: &NomTristate) -> Tristate {
    match t {
        NomTristate::Yes => Tristate::Yes,
        NomTristate::Module => Tristate::Module,
        NomTristate::No => Tristate::No,
    }
}

fn convert_macro(m: &Macro) -> Result<Expression, Error> {
    match m {
        Macro::Variable(name) => Ok(Expression::Symbol(name.clone())),
        Macro::DoubleQuoted(inner) => convert_macro(inner),
        Macro::FunctionCall(call) => Err(Error::Parse(format!(
            "function call `{}` is not supported in expressions",
            call.name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::load_kconfig;
    use crate::domain::{Expression, Limits};
    use std::fs;

    #[test]
    fn converts_select_and_imply_attributes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Kconfig"),
            "config HAS_UART\n\
             \tbool\n\
             \n\
             config BUS\n\
             \tbool\n\
             \n\
             config LOGGING\n\
             \tbool\n\
             \n\
             config UART\n\
             \tbool\n\
             \tselect HAS_UART if BUS\n\
             \timply LOGGING\n",
        )
        .unwrap();
        let loaded = load_kconfig(dir.path(), &dir.path().join("Kconfig"), Limits::default())
            .expect("parse");
        let uart = loaded.table.get("UART").expect("UART");
        assert_eq!(uart.selects.len(), 1);
        assert_eq!(uart.selects[0].symbol, "HAS_UART");
        assert_eq!(uart.selects[0].condition, Some(Expression::symbol("BUS")));
        assert_eq!(uart.implies.len(), 1);
        assert_eq!(uart.implies[0].symbol, "LOGGING");
        assert!(uart.implies[0].condition.is_none());
    }

    #[test]
    fn converts_choice_group_and_optional() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Kconfig"),
            "choice\n\
             \tprompt \"Console\"\n\
             \tdefault UART\n\
             \n\
             config UART\n\
             \tbool \"UART\"\n\
             config RTT\n\
             \tbool \"RTT\"\n\
             endchoice\n\
             \n\
             choice\n\
             \toptional\n\
             \n\
             config NONE_A\n\
             \tbool\n\
             config NONE_B\n\
             \tbool\n\
             endchoice\n",
        )
        .unwrap();
        let loaded = load_kconfig(dir.path(), &dir.path().join("Kconfig"), Limits::default())
            .expect("parse");
        assert_eq!(loaded.table.choices().len(), 2);
        let console = &loaded.table.choices()[0];
        assert_eq!(console.prompt.as_deref(), Some("Console"));
        assert_eq!(console.members, ["UART", "RTT"]);
        assert!(!console.optional);
        assert_eq!(console.defaults.len(), 1);
        let optional = &loaded.table.choices()[1];
        assert!(optional.optional);
        assert_eq!(optional.members, ["NONE_A", "NONE_B"]);
    }
}
