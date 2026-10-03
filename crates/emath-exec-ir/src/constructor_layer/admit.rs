use super::{SyntaxTree, ConstructorError, Item, Path, TypeExpr, TypeKind, Engine, Declaration, BTreeMap, StmtKind, Expr, ExprKind, BinaryOp, FnDecl};
use emath_core::tree::GenericArg;
use super::prelude::*;

/// Type-admit a constructor module with the same environment as evaluation.
pub(super) const CONSTRUCTOR_SECTIONS: &[&str] = &[
    "parameters",
    "representation",
    "invariants",
    "inputs",
    "outputs",
    "definitions",
    "question",
    "using",
    "answer",
    "budget",
    "tests",
    "exports",
];

/// E-NAME-026: one `exports:` line per file, naming only declarations
/// the file makes, each once.
pub(super) fn check_exports(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    let declared: std::collections::BTreeSet<&str> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration(decl) => Some(decl.name.as_str()),
            _ => None,
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut lines = 0;
    for item in &tree.items {
        let Item::Exports { names, .. } = item else {
            continue;
        };
        lines += 1;
        if lines > 1 {
            return Err(fault("E-NAME-026", "a file has at most one `exports:` line"));
        }
        for name in names {
            if !declared.contains(name.as_str()) {
                return Err(fault(
                    "E-NAME-026",
                    format!("`exports:` lists `{name}`, but this file declares no `{name}`"),
                ));
            }
            if !seen.insert(name.as_str()) {
                return Err(fault("E-NAME-026", format!("`exports:` lists `{name}` twice")));
            }
        }
    }
    Ok(())
}

pub(super) fn admit_constructor_surface(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    check_exports(tree)?;
    for item in &tree.items {
        match item {
            Item::Package { .. } | Item::Use { .. } | Item::Exports { .. } => {}
            Item::Notation(_) => {
                return Err(fault(
                    "E-KIND-GONE",
                    "notation aliases are not constructor surface; write the scalar operator or `use` an ordinary module",
                ));
            }
            Item::Declaration(decl) => {
                if !matches!(decl.as_kind.as_str(), "object" | "function" | "query") {
                    // The parser maps `emath custom C:` to an empty
                    // `as_kind`; the refusal names what the user wrote.
                    let kind_name = if decl.as_kind.is_empty() {
                        "custom"
                    } else {
                        decl.as_kind.as_str()
                    };
                    return Err(fault(
                        "E-KIND-GONE",
                        format!(
                            "declaration kind `{kind_name}` is not a core kind; write `emath object`, `emath function`, or `emath query`"
                        ),
                    ));
                }
                for section in decl.sections() {
                    if !CONSTRUCTOR_SECTIONS.contains(&section.name.as_str()) {
                        return Err(fault(
                            "E-SEC-101",
                            format!(
                                "`{}:` is not a constructor section",
                                section.name
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn admit_tree(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    admit_tree_at(tree, None)
}

/// Type-admit a module, resolving `use` paths from `source` or the repo roots.
pub fn admit_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<(), ConstructorError> {
    let mut engine = empty_engine();
    install_local_items(&mut engine, tree)?;
    load_imports(
        &mut engine,
        tree,
        &module_roots_for(source),
        source,
    )?;
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        match decl.as_kind.as_str() {
            "function" => admit_function(&engine, decl)?,
            "query" => admit_query(&engine, decl)?,
            "object" => {}
            other => {
                return Err(fault(
                    "E-KIND-GONE",
                    format!("declaration kind `{other}` is not a core kind"),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum CType {
    Bool,
    Int,
    Rat,
    Float64,
    /// `sequence(T)`; `Unknown` element when the element is not known.
    Sequence(Box<CType>),
    /// The mutable buffer carrier: unparameterized
    /// in this cut; element values are checked at use, statically
    /// claimed nowhere.
    Buffer,
    Tuple,
    Record,
    /// `A -> B`, curried: a multi-input function value is
    /// `A -> (B -> C)`. `Unknown` parts when not known.
    Closure(Box<CType>, Box<CType>),
    /// A type parameter (`T` in `emath function map<T, U>`). Inside the
    /// declaring function it is opaque: it conforms only to itself.
    Var(String),
    Code,
    Receipt,
    Schema,
    Str,
    Unknown,
}

impl std::fmt::Display for CType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sequence(element) => write!(f, "sequence({element})"),
            Self::Closure(domain, codomain) => write!(f, "({domain} -> {codomain})"),
            Self::Var(name) => f.write_str(name),
            Self::Unknown => f.write_str("_"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl CType {
    pub(super) fn seq(element: CType) -> Self {
        Self::Sequence(Box::new(element))
    }

    pub(super) fn any_seq() -> Self {
        Self::seq(Self::Unknown)
    }

    pub(super) fn closure(domain: CType, codomain: CType) -> Self {
        Self::Closure(Box::new(domain), Box::new(codomain))
    }

    pub(super) fn any_closure() -> Self {
        Self::closure(Self::Unknown, Self::Unknown)
    }

    pub(super) fn is_closure(&self) -> bool {
        matches!(self, Self::Closure(..))
    }

    pub(super) fn join(&self, other: &Self) -> Option<Self> {
        if self == other {
            return Some(self.clone());
        }
        match (self, other) {
            (Self::Unknown, t) | (t, Self::Unknown) => Some(t.clone()),
            (Self::Int, Self::Rat) | (Self::Rat, Self::Int) => Some(Self::Rat),
            (Self::Int | Self::Rat, Self::Float64) | (Self::Float64, Self::Int | Self::Rat) => {
                Some(Self::Float64)
            }
            (Self::Schema, Self::Record) | (Self::Record, Self::Schema) => Some(Self::Record),
            // Element and signature disagreements widen to Unknown rather
            // than refuse: sequences carry heterogeneous rows (tuples,
            // tagged records) the element type does not model.
            (Self::Sequence(a), Self::Sequence(b)) => {
                Some(Self::seq(a.join(b).unwrap_or(Self::Unknown)))
            }
            (Self::Closure(d1, r1), Self::Closure(d2, r2)) => Some(Self::closure(
                d1.join(d2).unwrap_or(Self::Unknown),
                r1.join(r2).unwrap_or(Self::Unknown),
            )),
            _ => None,
        }
    }

    pub(super) fn conforms(&self, declared: &Self) -> bool {
        match (self, declared) {
            (Self::Unknown, _) | (_, Self::Unknown) => true,
            (Self::Sequence(a), Self::Sequence(b)) => a.conforms(b),
            // A closure fits where its domain accepts the declared domain
            // and its result fits the declared result.
            (Self::Closure(d1, r1), Self::Closure(d2, r2)) => d2.conforms(d1) && r1.conforms(r2),
            _ => {
                self == declared
                    || matches!(
                        (self, declared),
                        (Self::Int, Self::Rat)
                            | (Self::Schema, Self::Record)
                            | (Self::Record, Self::Schema)
                    )
            }
        }
    }

    /// Replace bound type parameters; unbound ones become `Unknown` so a
    /// callee's parameter never leaks into its caller.
    pub(super) fn substitute(&self, bound: &BTreeMap<String, CType>, vars: &[String]) -> Self {
        match self {
            Self::Var(name) if vars.contains(name) => {
                bound.get(name).cloned().unwrap_or(Self::Unknown)
            }
            Self::Sequence(element) => Self::seq(element.substitute(bound, vars)),
            Self::Closure(domain, codomain) => {
                Self::closure(domain.substitute(bound, vars), codomain.substitute(bound, vars))
            }
            other => other.clone(),
        }
    }

    /// Match a declared parameter type (which may mention `vars`)
    /// against an argument type, binding each parameter. A parameter
    /// seen twice widens through `join` (`Int` then `Rat` is `Rat`); an
    /// incompatible second sighting is a mismatch.
    pub(super) fn unify(
        declared: &Self,
        got: &Self,
        vars: &[String],
        bound: &mut BTreeMap<String, CType>,
    ) -> bool {
        match (declared, got) {
            (Self::Var(name), _) if vars.contains(name) => match bound.get(name) {
                None => {
                    bound.insert(name.clone(), got.clone());
                    true
                }
                Some(previous) => match previous.join(got) {
                    Some(joined) => {
                        bound.insert(name.clone(), joined);
                        true
                    }
                    None => false,
                },
            },
            (_, Self::Unknown) | (Self::Unknown, _) => true,
            (Self::Sequence(a), Self::Sequence(b)) => Self::unify(a, b, vars, bound),
            // The domain is contravariant: bind its parameters, then the
            // declared domain must fit the closure's domain.
            (Self::Closure(d1, r1), Self::Closure(d2, r2)) => {
                Self::bind_vars(d1, d2, vars, bound)
                    && d1.substitute(bound, vars).conforms(d2)
                    && Self::unify(r1, r2, vars, bound)
            }
            _ => got.conforms(declared),
        }
    }

    /// Bind type parameters by shape only, without checking conformance.
    fn bind_vars(
        declared: &Self,
        got: &Self,
        vars: &[String],
        bound: &mut BTreeMap<String, CType>,
    ) -> bool {
        match (declared, got) {
            (Self::Var(name), _) if vars.contains(name) => {
                Self::unify(declared, got, vars, bound)
            }
            (Self::Sequence(a), Self::Sequence(b)) => Self::bind_vars(a, b, vars, bound),
            (Self::Closure(d1, r1), Self::Closure(d2, r2)) => {
                Self::bind_vars(d1, d2, vars, bound) && Self::bind_vars(r1, r2, vars, bound)
            }
            _ => true,
        }
    }
}

pub(super) fn ctype_from_type(ty: &TypeExpr) -> CType {
    ctype_from_type_in(ty, &[])
}

/// Declared type to `CType`; names in `vars` are type parameters.
pub(super) fn ctype_from_type_in(ty: &TypeExpr, vars: &[String]) -> CType {
    match &ty.kind {
        TypeKind::Fn { domain, codomain } => CType::closure(
            ctype_from_type_in(domain, vars),
            ctype_from_type_in(codomain, vars),
        ),
        TypeKind::List(items) => CType::seq(
            items.first().map_or(CType::Unknown, |item| ctype_from_type_in(item, vars)),
        ),
        TypeKind::Tuple(_) => CType::Tuple,
        TypeKind::Path { segments, generic_args } => {
            let last = segments.last().map(String::as_str);
            if segments.len() == 1 && vars.iter().any(|var| Some(var.as_str()) == last) {
                return CType::Var(segments[0].clone());
            }
            match last {
                Some("sequence" | "Sequence") => CType::seq(
                    generic_args.first().map_or(CType::Unknown, |arg| ctype_from_generic_arg(arg, vars)),
                ),
                Some(name) => ctype_from_name(name),
                None => CType::Unknown,
            }
        }
        _ => CType::Unknown,
    }
}

fn ctype_from_generic_arg(arg: &GenericArg, vars: &[String]) -> CType {
    match arg {
        GenericArg::Type(ty) => ctype_from_type_in(ty, vars),
        GenericArg::Value(Expr {
            kind: ExprKind::Path { segments, .. },
            ..
        }) if segments.len() == 1 => {
            if vars.contains(&segments[0]) {
                CType::Var(segments[0].clone())
            } else {
                ctype_from_name(&segments[0])
            }
        }
        _ => CType::Unknown,
    }
}

fn ctype_from_name(name: &str) -> CType {
    match name {
        "Int" | "Nat" => CType::Int,
        "Bool" => CType::Bool,
        "Rat" => CType::Rat,
        "Str" => CType::Str,
        "Float64" | "F64" => CType::Float64,
        "Code" => CType::Code,
        "buffer" => CType::Buffer,
        _ => CType::Unknown,
    }
}

pub(super) fn admit_function(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    admit_function_body(engine, decl).map_err(|err| in_declaration(decl, err))
}

pub(super) fn admit_query(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    admit_query_body(engine, decl).map_err(|err| in_declaration(decl, err))
}

fn in_declaration(decl: &Declaration, mut err: ConstructorError) -> ConstructorError {
    err.message = format!("in `{}`: {}", decl.name, err.message);
    err
}

fn admit_function_body(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    refuse_duplicate_fields(decl, "inputs")?;
    refuse_duplicate_fields(decl, "outputs")?;
    let vars = generic_names(decl);
    let mut types = BTreeMap::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        types.insert(name, ctype_from_type_in(&ty, &vars));
    }
    let outputs: BTreeMap<String, CType> = section_typed_fields(decl, "outputs")
        .into_iter()
        .map(|(name, ty)| (name, ctype_from_type_in(&ty, &vars)))
        .collect();
    for (name, expr) in constructor_defs(decl) {
        let got = engine.infer(&types, &expr)?;
        if let Some(declared) = outputs.get(&name)
            && !got.conforms(declared)
        {
            return Err(fault(
                "type",
                format!("definition `{name}` does not have the declared output type: declared {declared}, got {got}"),
            ));
        }
        types.insert(name, got);
    }
    for name in outputs.keys() {
        if !types.contains_key(name) {
            return Err(fault(
                "type",
                format!("output `{name}` has no definition"),
            ));
        }
    }
    // Fault-demand rows reference `diagnostic.code` exactly as query test
    // rows do (see `admit_query`); the runner binds the same record for
    // function examples, carrying the fault name on a faulting call. A
    // declaration that itself binds the name keeps its own binding.
    types.entry("diagnostic".into()).or_insert(CType::Record);
    admit_tests(engine, decl, &types, &vars)
}

pub(super) fn generic_names(decl: &Declaration) -> Vec<String> {
    decl.generics.iter().map(|generic| generic.name.clone()).collect()
}

fn admit_query_body(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    refuse_duplicate_fields(decl, "inputs")?;
    let mut types = BTreeMap::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        types.insert(name, ctype_from_type(&ty));
    }
    for (name, expr) in section_assigns(decl, "definitions") {
        let got = engine.infer(&types, &expr)?;
        types.insert(name, got);
    }
    types.insert("receipt".into(), CType::Receipt);
    types.insert("diagnostic".into(), CType::Record);
    admit_tests(engine, decl, &types, &[])
}

/// Refuse unrecognized `tests:` row forms instead of silently skipping
/// them. The collectors admit exactly `example
/// <label>:` blocks (with `given`/`expect` rows inside), loose `given`
/// and `expect` rows, and the legacy `given`-headed assignment inside
/// an example. Anything else (an invented `fault <label>:` section, an
/// expression row, a field row) used to vanish without a diagnostic,
/// so authored intent silently did not run.
pub(super) fn refuse_unknown_test_rows(decl: &Declaration) -> Result<(), ConstructorError> {
    for section in decl.sections().filter(|section| section.name == "tests") {
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::Section(example) if example.name == "example" => {
                    for inner in &example.suite.statements {
                        match &inner.kind {
                            StmtKind::Given { .. } | StmtKind::Expect(_) => {}
                            StmtKind::Assign { target, .. }
                                if target.segments.first().map(String::as_str) == Some("given") => {}
                            other => {
                                return Err(fault(
                                    "unknown_test_row",
                                    format!(
                                        "row inside `example {}` is not a test form ({}); \
                                         admitted rows: `given <name> = <value>`, `expect <expr>`",
                                        example.generic.as_deref().unwrap_or(""),
                                        row_word(other),
                                    ),
                                ));
                            }
                        }
                    }
                }
                StmtKind::Given { .. } | StmtKind::Expect(_) => {}
                other => {
                    return Err(fault(
                        "unknown_test_row",
                        format!(
                            "row in `tests:` is not a test form ({}); admitted rows: \
                             `example <label>:` blocks, `given <name> = <value>`, `expect <expr>`",
                            row_word(other),
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// One- or two-word name of a refused row for the diagnostic message.
fn row_word(kind: &StmtKind) -> String {
    match kind {
        StmtKind::Section(section) => format!("section `{}`", section.name),
        StmtKind::Assign { .. } => "assignment row".into(),
        StmtKind::Expr(_) => "expression row".into(),
        StmtKind::Command { head, .. } => format!("command `{}`", head.join(" ")),
        StmtKind::FieldDecl { name, .. } => format!("field `{name}`"),
        _ => "statement row".into(),
    }
}

pub(super) fn admit_tests(
    engine: &Engine,
    decl: &Declaration,
    types: &BTreeMap<String, CType>,
    vars: &[String],
) -> Result<(), ConstructorError> {
    refuse_unknown_test_rows(decl)?;
    for section in decl.sections().filter(|section| section.name == "tests") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                continue;
            };
            if example.name != "example" {
                continue;
            }
            let mut local = types.clone();
            // Each example instantiates the type parameters afresh from
            // its `given` values.
            let mut bound = BTreeMap::new();
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        // A `given` that is not an input is an example-local
                        // binding the `expect` rows may use (`result(k)`).
                        let Some(declared) = local.get(name).cloned() else {
                            let got = engine.infer(&local, value)?;
                            local.insert(name.clone(), got);
                            continue;
                        };
                        // A `Rat`-annotated input binds its exact decimal
                        // rational when the given value is a bare decimal.
                        let value = if declared == CType::Rat {
                            exact_decimal_spine(value)
                        } else {
                            value.clone()
                        };
                        let got = engine.infer(&local, &value)?;
                        if !CType::unify(&declared, &got, vars, &mut bound) {
                            return Err(fault(
                                "type",
                                format!("`given` `{name}` does not have the declared input type: declared {declared}, got {got}"),
                            ));
                        }
                    }
                    StmtKind::Expect(expr) => {
                        // Expect rows may reference fault names nominally
                        // (Engine::expect_atoms); anything else still
                        // resolves or refuses as usual.
                        let instantiated: BTreeMap<String, CType> = local
                            .iter()
                            .map(|(name, ty)| (name.clone(), ty.substitute(&bound, vars)))
                            .collect();
                        engine.expect_atoms.set(true);
                        let got = engine.infer(&instantiated, expr);
                        engine.expect_atoms.set(false);
                        let got = got?;
                        if !got.conforms(&CType::Bool) {
                            return Err(fault("type", "`expect` must be Bool"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

impl Engine {
    pub(super) fn infer(&self, types: &BTreeMap<String, CType>, expr: &Expr) -> Result<CType, ConstructorError> {
        match &expr.kind {
            ExprKind::Int(_) => Ok(CType::Int),
            ExprKind::Rational { .. } => Ok(CType::Rat),
            ExprKind::Float(_) => Ok(CType::Float64),
            ExprKind::Bool(_) => Ok(CType::Bool),
            ExprKind::Path { segments, .. } => self.infer_path(types, segments),
            ExprKind::Unary { value, .. } => self.infer(types, value),
            ExprKind::Binary { op, left, right } => {
                let l = self.infer(types, left)?;
                let r = self.infer(types, right)?;
                // Mutable state has no total value equality: refuse
                // `==`/`!=` on buffers by name;
                // compare a projection (`buf[i]`, `buf.length`).
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                    && (l == CType::Buffer || r == CType::Buffer)
                {
                    return Err(fault(
                        "buffer_equality_refused",
                        "buffer comparison is refused: a mutable carrier has no total value equality",
                    ));
                }
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                    && (matches!(l, CType::Closure(..)) || matches!(r, CType::Closure(..)))
                {
                    return Err(fault(
                        "function_equality_refused",
                        "function comparison is refused: equality of functions is not decidable; compare their values at chosen inputs",
                    ));
                }
                // Str carries structural equality only (bead e6gvs): the
                // ordering ops refuse at admission so the admit lane
                // agrees with the runtime's `type` fault; the carrier
                // exists without the string library.
                if matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)
                    && (l == CType::Str || r == CType::Str)
                {
                    return Err(fault(
                        "type",
                        "Str carries structural equality only; ordering is not part of the carrier",
                    ));
                }
                Ok(match op {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt
                    | BinaryOp::Ge | BinaryOp::And | BinaryOp::Or | BinaryOp::Imply
                    | BinaryOp::Iff => CType::Bool,
                    BinaryOp::Div => match (l, r) {
                        (CType::Float64, _) | (_, CType::Float64) => CType::Float64,
                        (CType::Unknown | CType::Var(_), _) | (_, CType::Unknown | CType::Var(_)) => {
                            CType::Unknown
                        }
                        _ => CType::Rat,
                    },
                    _ => l.join(&r).unwrap_or(CType::Unknown),
                })
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let cond = self.infer(types, condition)?;
                if !cond.conforms(&CType::Bool) {
                    return Err(fault("type", "if condition must be Bool"));
                }
                let then_ty = self.infer(types, then_value)?;
                let else_ty = self.infer(types, else_value)?;
                then_ty
                    .join(&else_ty)
                    .ok_or_else(|| fault("type", "if branches must have the same type"))
            }
            ExprKind::List(items) => {
                let mut element = CType::Unknown;
                for (index, item) in items.iter().enumerate() {
                    let got = self.infer(types, item)?;
                    element = if index == 0 {
                        got
                    } else {
                        element.join(&got).unwrap_or(CType::Unknown)
                    };
                }
                Ok(CType::seq(element))
            }
            ExprKind::SequenceCons {
                head,
                tail,
                spread_head,
            } => {
                let head = self.infer(types, head)?;
                let tail = self.infer(types, tail)?;
                let element = if *spread_head {
                    match head {
                        CType::Sequence(front) => *front,
                        _ => CType::Unknown,
                    }
                } else {
                    head
                };
                Ok(match tail {
                    CType::Sequence(rest) => CType::seq(element.join(&rest).unwrap_or(CType::Unknown)),
                    _ => CType::any_seq(),
                })
            }
            ExprKind::Range { .. } => Ok(CType::seq(CType::Int)),
            ExprKind::Tuple(_) => Ok(CType::Tuple),
            ExprKind::Record { .. } => Ok(CType::Record),
            ExprKind::Index { value, indices } => {
                let carrier = self.infer(types, value)?;
                if let Some(index) = indices.first() {
                    let _ = self.infer(types, index)?;
                }
                Ok(match (carrier, indices.len()) {
                    (CType::Sequence(element), 1) => *element,
                    _ => CType::Unknown,
                })
            }
            ExprKind::Call { function, args } => self.infer_call(types, function, args),
            ExprKind::FunctionAbs { param, domain, body } => {
                let mut inner = types.clone();
                let domain = ctype_from_type_expr_or_path(domain);
                inner.insert(param.clone(), domain.clone());
                let body = self.infer(&inner, body)?;
                Ok(CType::closure(domain, body))
            }
            ExprKind::Recur { ty, body, .. } => {
                if !is_fn_type(ty) {
                    return Err(fault(
                        "type",
                        "recur binder type must be a function type A -> B or a record of function types",
                    ));
                }
                if !is_guarded_recur_body(body) {
                    return Err(fault(
                        "unguarded_recursive_binding",
                        "recur body must be a function literal or a record of function literals",
                    ));
                }
                Ok(ctype_from_type_expr_or_path(ty))
            }
            ExprKind::Quote { .. } | ExprKind::QuoteBind { .. } => Ok(CType::Code),
            ExprKind::CallableBinder {
                callee,
                param,
                domain,
                body,
            } => self.infer_callable_binder(types, callee, param, domain, body),
            ExprKind::Cases {
                subject,
                arms,
                else_arm,
            } => {
                if let Some(subject) = subject {
                    let _ = self.infer(types, subject)?;
                }
                let mut result = self.infer(types, else_arm)?;
                for (cond, value) in arms {
                    let _ = self.infer(types, cond)?;
                    let arm = self.infer(types, value)?;
                    result = result
                        .join(&arm)
                        .ok_or_else(|| fault("type", "cases arms must have the same type"))?;
                }
                Ok(result)
            }
            // A string literal is its own carrier: it conforms to no
            // numeric output (the output-conformance check fires
            // E-TYPE-010 via "type" instead of silently admitting).
            ExprKind::Str(_) => Ok(CType::Str),
            _ => Ok(CType::Unknown),
        }
    }

    pub(super) fn infer_path(
        &self,
        types: &BTreeMap<String, CType>,
        segments: &[String],
    ) -> Result<CType, ConstructorError> {
        let name = segments.join(".");
        if let Some(ty) = types.get(&name) {
            return Ok(ty.clone());
        }
        if segments.len() >= 2 {
            if let Some(ty) = types.get(&segments[0]) {
                if segments.len() == 2 {
                    return Ok(match (ty, segments[1].as_str()) {
                        (CType::Sequence(_), "length") => CType::Int,
                        (CType::Rat | CType::Int, "numer" | "denom") => CType::Int,
                        // Projection type is not reconstructed from a record tag.
                        // Unknown conforms to a declared field type; Schema does not.
                        (CType::Receipt | CType::Record | CType::Tuple | CType::Code | CType::Schema |
CType::Unknown, _) => CType::Unknown,
                        _ => CType::Unknown,
                    });
                }
                // Nested projection beyond one hop (`o.inner.x`): every
                // deeper hop projects through a record carrier whose
                // field type is not reconstructed from the record tag
                // (the two-segment record policy, extended to any
                // depth). A projection miss still faults `unbound` at
                // runtime in the engine.
                return Ok(CType::Unknown);
            }
            if segments.len() == 2 && self.objects.contains_key(&segments[0]) {
                return Ok(match segments[1].as_str() {
                    "pack" | "open" => CType::any_closure(),
                    _ => CType::Unknown,
                });
            }
        }
        if matches!(name.as_str(), "true" | "false") {
            return Ok(CType::Bool);
        }
        if is_schema_tag(&name) {
            return Ok(CType::Schema);
        }
        if let Some(decl) = self.functions.get(&name) {
            return Ok(function_value_type(decl));
        }
        if self.objects.contains_key(&name) {
            return Ok(CType::Record);
        }
        if self.queries.contains_key(&name) {
            return Ok(CType::Unknown);
        }
        if self.expect_atoms.get() && segments.len() == 1 {
            // Expect-row fault-name atom (see Engine::expect_atoms): the
            // name infers as a nominal schema atom so a fault demand such
            // as `division_by_zero` admits; a wrong name fails the example.
            return Ok(CType::Schema);
        }
        // Retired builtin spellings (the constructor constitution,
        // emath-xx0x.1): `grad`'s Wengert tape is gone and
        // differentiation is authored (`analysis.autodiff` /
        // `analysis.derivative`, the exact Rat tier). The spelling
        // refuses by name instead of reporting a bare unbound - after
        // every legitimate name source, so an authored object,
        // function, or query named `grad` still admits.
        if segments.len() == 1 && name == "grad" {
            return Err(fault(
                "retired_builtin",
                "`grad` is retired: differentiation is authored - use the analysis.autodiff / analysis.derivative modules",
            ));
        }
        Err(fault("unbound", format!("unbound `{name}`")))
    }

    pub(super) fn infer_callable_binder(
        &self,
        types: &BTreeMap<String, CType>,
        callee: &Expr,
        param: &str,
        domain: &Expr,
        body: &Expr,
    ) -> Result<CType, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &callee.kind {
            let name = segments.join(".");
            if name == "quote.open" || name == "quote.view" || name == "quote.bind" {
                // The domain slot carries what the callee family
                // means: quote.open/view take a VALUE (the package /
                // the code); quote.bind takes the binder's DOMAIN
                // TYPE, embedded symbolically into the minted
                // function literal (the VM's bind_fresh) - it is not
                // a value expression, so it is not inferred as one.
                if name != "quote.bind" {
                    let _ = self.infer(types, domain)?;
                }
                let mut inner = types.clone();
                let bound = if name == "quote.view" {
                    CType::Record
                } else {
                    CType::Code
                };
                inner.insert(param.to_string(), bound);
                return self.infer(&inner, body);
            }
        }
        let _ = self.infer(types, callee)?;
        let _ = self.infer(types, domain)?;
        Ok(CType::Unknown)
    }

    pub(super) fn infer_call(
        &self,
        types: &BTreeMap<String, CType>,
        function: &Expr,
        args: &[Expr],
    ) -> Result<CType, ConstructorError> {
        if let ExprKind::Path { segments, .. } = &function.kind {
            let name = segments.join(".");
            if name == "transformation_rule_unavailable" || name.ends_with(".opaque") {
                return Ok(CType::Unknown);
            }
            if name == "length" {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(CType::Int);
            }
            if name == "constructor_refuse" {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(CType::Unknown);
            }
            if let Some(op) = machine_int_basename(&name) {
                return self.infer_machine_int(op, types, args);
            }
            if let Some(op) = machine_buffer_basename(&name) {
                return self.infer_machine_buffer(op, types, args);
            }
            if name.starts_with("quote.") {
                for arg in args {
                    let _ = self.infer_quote_arg(types, arg)?;
                }
                return Ok(CType::Unknown);
            }
            if segments.len() == 2 && segments[1] == "pack" {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(CType::Record);
            }
            let local = segments.len() == 1 && types.get(&name).is_some_and(CType::is_closure);
            if let Some(decl) = self.functions.get(&name).filter(|_| !local) {
                return self.infer_named_call(types, &name, decl, args);
            }
            // Postfix `.field` on a non-path receiver is parsed as
            // `field(recv)` (indistinguishable from a call), so the
            // field-access lane runs BEFORE the leftover-name refusal: a
            // record field named like a reserved ordinary-method name
            // (`total`, `sum`, ...) is a field access, never a method
            // call. A non-record receiver still falls through to the
            // refusal below (the runtime mirrors this order; see
            // `engine_step/call.rs`).
            if segments.len() == 1 && args.len() == 1 && !self.is_typed_callee(types, &name) {
                let recv = self.infer(types, &args[0])?;
                match (&recv, name.as_str()) {
                    (CType::Rat | CType::Int, "numer" | "denom")
                    | (CType::Sequence(_) | CType::Buffer, "length") => return Ok(CType::Int),
                    _ => {}
                }
                if matches!(
                    recv,
                    CType::Record
                        | CType::Tuple
                        | CType::Receipt
                        | CType::Code
                        | CType::Schema
                        | CType::Unknown
                ) {
                    return Ok(CType::Unknown);
                }
            }
            // A name the user bound (a local def, input, or prior binding)
            // is the user's; the recipe refusal is for UNBOUND names only.
            if is_refused_recipe(&name) && !types.contains_key(&name) {
                return Err(fault(
                    "method_unavailable",
                    format!("`{name}` is an ordinary module method, not a constructor operation; import it with `use`, or rename the record field: a field access `recv.{name}` resolves only when the receiver carries a field named `{name}`"),
                ));
            }
        }
        // A closure-typed callee applies one argument per arrow.
        let mut result = self.infer(types, function)?;
        for arg in args {
            let got = self.infer(types, arg)?;
            result = match result {
                CType::Closure(domain, codomain) => {
                    if !got.conforms(&domain) {
                        return Err(fault(
                            "type",
                            format!("closure argument does not fit its domain: expected {domain}, got {got}"),
                        ));
                    }
                    *codomain
                }
                _ => CType::Unknown,
            };
        }
        Ok(if args.is_empty() { CType::Unknown } else { result })
    }

    /// A call to a declared function: each argument must fit its input
    /// type, binding the callee's type parameters; the result type is the
    /// output type with those parameters substituted.
    pub(super) fn infer_named_call(
        &self,
        types: &BTreeMap<String, CType>,
        name: &str,
        decl: &FnDecl,
        args: &[Expr],
    ) -> Result<CType, ConstructorError> {
        let vars = &decl.generics;
        let mut bound = BTreeMap::new();
        for (index, arg) in args.iter().enumerate() {
            let got = self.infer(types, arg)?;
            let Some(declared) = decl.input_types.get(index) else {
                continue;
            };
            let declared = ctype_from_type_in(declared, vars);
            if !CType::unify(&declared, &got, vars, &mut bound) {
                let input = decl.inputs.get(index).map_or("?", String::as_str);
                return Err(fault(
                    "type",
                    format!(
                        "argument `{input}` of `{name}` expects {}, got {got}",
                        declared.substitute(&bound, vars)
                    ),
                ));
            }
        }
        Ok(match decl.output_types.as_slice() {
            [ty] => ctype_from_type_in(ty, vars).substitute(&bound, vars),
            [] => CType::Unknown,
            _ => CType::Record,
        })
    }

    pub(super) fn is_typed_callee(&self, types: &BTreeMap<String, CType>, name: &str) -> bool {
        self.functions.contains_key(name) || types.get(name).is_some_and(CType::is_closure)
    }

    pub(super) fn infer_quote_arg(
        &self,
        types: &BTreeMap<String, CType>,
        expr: &Expr,
    ) -> Result<CType, ConstructorError> {
        match self.infer(types, expr) {
            Ok(ty) => Ok(ty),
            Err(err) if err.code == "unbound" => match &expr.kind {
                ExprKind::Path { segments, .. } if segments.len() == 1 => Ok(CType::Schema),
                _ => Err(err),
            },
            Err(err) => Err(err),
        }
    }

    /// Buffer-carrier machine ops:
    /// `buffer(size, fill)` -> `CType::Buffer` (unparameterized;
    /// elements are checked at use), `buffer_set(buf, i, v)` -> Unit
    /// (inferred Unknown). Equality on buffers refuses elsewhere.
    pub(super) fn infer_machine_buffer(
        &self,
        op: &str,
        types: &BTreeMap<String, CType>,
        args: &[Expr],
    ) -> Result<CType, ConstructorError> {
        match op {
            "buffer" => {
                if args.len() != 2 {
                    return Err(fault("arity", "`buffer` expects a size and a fill value"));
                }
                let size = self.infer(types, &args[0])?;
                if !size.conforms(&CType::Int) {
                    return Err(fault("type", "`buffer` expects an Int size"));
                }
                let _ = self.infer(types, &args[1])?;
                Ok(CType::Buffer)
            }
            "buffer_set" => {
                if args.len() != 3 {
                    return Err(fault(
                        "arity",
                        "`buffer_set` expects a buffer, an index, and a value",
                    ));
                }
                let carrier = self.infer(types, &args[0])?;
                if !carrier.conforms(&CType::Buffer) {
                    return Err(fault("type", "`buffer_set` expects a buffer"));
                }
                let index = self.infer(types, &args[1])?;
                if !index.conforms(&CType::Int) {
                    return Err(fault("type", "`buffer_set` expects an Int index"));
                }
                let _ = self.infer(types, &args[2])?;
                Ok(CType::Unknown)
            }
            _ => Err(fault(
                "implementation_unavailable",
                format!("unknown buffer op `{op}`"),
            )),
        }
    }

    pub(super) fn infer_machine_int(
        &self,
        op: &str,
        types: &BTreeMap<String, CType>,
        args: &[Expr],
    ) -> Result<CType, ConstructorError> {
        let expect_int_args = |count: usize, this: &Self| -> Result<(), ConstructorError> {
            if args.len() != count {
                return Err(fault(
                    "arity",
                    format!("`{op}` expects {count} argument(s)"),
                ));
            }
            for arg in args {
                let got = this.infer(types, arg)?;
                if !got.conforms(&CType::Int) {
                    return Err(fault("type", format!("`{op}` expects Int")));
                }
            }
            Ok(())
        };
        match op {
            "int_sum" | "int_prod" => {
                if args.len() != 1 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects one sequence(Int) argument"),
                    ));
                }
                let got = self.infer(types, &args[0])?;
                if !got.conforms(&CType::seq(CType::Int)) {
                    return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                }
                Ok(CType::Int)
            }
            _ => {
                expect_int_args(2, self)?;
                Ok(CType::Int)
            }
        }
    }
}

/// A declared function as a value: curried over its inputs. A
/// multi-output function returns a record.
pub(super) fn function_value_type(decl: &FnDecl) -> CType {
    let vars = &decl.generics;
    let mut result = match decl.output_types.as_slice() {
        [ty] => ctype_from_type_in(ty, vars).substitute(&BTreeMap::new(), vars),
        [] => CType::Unknown,
        _ => CType::Record,
    };
    for ty in decl.input_types.iter().rev() {
        result = CType::closure(ctype_from_type_in(ty, vars).substitute(&BTreeMap::new(), vars), result);
    }
    if decl.input_types.is_empty() {
        CType::any_closure()
    } else {
        result
    }
}

pub(super) fn ctype_from_type_expr_or_path(expr: &Expr) -> CType {
    match &expr.kind {
        ExprKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int" | "Nat") => CType::Int,
            Some("Bool") => CType::Bool,
            Some("Rat") => CType::Rat,
            Some("Float64" | "F64") => CType::Float64,
            Some("Code") => CType::Code,
            _ => CType::Unknown,
        },
        ExprKind::Call { function, .. } if is_fn_ctor(function) => CType::any_closure(),
        ExprKind::Call { function, args } => match (&function.kind, args.as_slice()) {
            (ExprKind::Path { segments, .. }, [element]) if segments.len() == 1 && segments[0] == "sequence" => {
                CType::seq(ctype_from_type_expr_or_path(element))
            }
            _ => CType::Unknown,
        },
        _ => CType::Unknown,
    }
}

