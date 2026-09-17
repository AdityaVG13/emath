use super::*;
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

pub(super) fn admit_constructor_surface(tree: &SyntaxTree) -> Result<(), ConstructorError> {
    for item in &tree.items {
        match item {
            Item::Package { .. } | Item::Use { .. } => {}
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
        &mut BTreeSet::new(),
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
    Sequence,
    Tuple,
    Record,
    Closure,
    Code,
    Receipt,
    Schema,
    Str,
    Unknown,
}

impl CType {
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
            _ => None,
        }
    }

    pub(super) fn conforms(&self, declared: &Self) -> bool {
        self == declared
            || matches!(
                (self, declared),
                (Self::Int, Self::Rat)
                    | (Self::Unknown, _)
                    | (_, Self::Unknown)
                    | (Self::Schema, Self::Record)
                    | (Self::Record, Self::Schema)
            )
    }
}

pub(super) fn ctype_from_type(ty: &TypeExpr) -> CType {
    match &ty.kind {
        TypeKind::Fn { .. } => CType::Closure,
        TypeKind::List(_) => CType::Sequence,
        TypeKind::Tuple(_) => CType::Tuple,
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") | Some("Nat") => CType::Int,
            Some("Bool") => CType::Bool,
            Some("Rat") => CType::Rat,
            Some("Float64") | Some("F64") => CType::Float64,
            Some("Code") => CType::Code,
            Some("sequence") | Some("Sequence") => CType::Sequence,
            _ => CType::Unknown,
        },
        _ => CType::Unknown,
    }
}

pub(super) fn admit_function(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
    refuse_duplicate_fields(decl, "inputs")?;
    refuse_duplicate_fields(decl, "outputs")?;
    let mut types = BTreeMap::new();
    for (name, ty) in section_typed_fields(decl, "inputs") {
        types.insert(name, ctype_from_type(&ty));
    }
    let outputs: BTreeMap<String, CType> = section_typed_fields(decl, "outputs")
        .into_iter()
        .map(|(name, ty)| (name, ctype_from_type(&ty)))
        .collect();
    for (name, expr) in constructor_defs(decl) {
        let got = engine.infer(&types, &expr)?;
        if let Some(declared) = outputs.get(&name)
            && !got.conforms(declared)
        {
            return Err(fault(
                "type",
                format!("definition `{name}` does not have the declared output type"),
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
    admit_tests(engine, decl, &types)
}

pub(super) fn admit_query(engine: &Engine, decl: &Declaration) -> Result<(), ConstructorError> {
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
    admit_tests(engine, decl, &types)
}

/// Refuse unrecognized `tests:` row forms instead of silently skipping
/// them (bead emath-7zplf). The collectors admit exactly `example
/// <label>:` blocks (with `given`/`expect` rows inside), loose `given`
/// and `expect` rows, and the legacy `given`-headed assignment inside
/// an example. Anything else — an invented `fault <label>:` section, an
/// expression row, a field row — used to vanish without a diagnostic,
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
            let local = types.clone();
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        let Some(declared) = local.get(name).cloned() else {
                            return Err(fault(
                                "unbound",
                                format!("`given` name `{name}` is not an input"),
                            ));
                        };
                        // A `Rat`-annotated input binds its exact decimal
                        // rational when the given value is a bare decimal.
                        let value = if declared == CType::Rat {
                            exact_decimal_spine(value)
                        } else {
                            value.clone()
                        };
                        let got = engine.infer(&local, &value)?;
                        if !got.conforms(&declared) {
                            return Err(fault(
                                "type",
                                format!("`given` `{name}` does not have the declared input type"),
                            ));
                        }
                    }
                    StmtKind::Expect(expr) => {
                        // Expect rows may reference fault names nominally
                        // (Engine::expect_atoms); anything else still
                        // resolves or refuses as usual.
                        engine.expect_atoms.set(true);
                        let got = engine.infer(&local, expr);
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
                Ok(match op {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt
                    | BinaryOp::Ge | BinaryOp::And | BinaryOp::Or | BinaryOp::Imply
                    | BinaryOp::Iff => CType::Bool,
                    BinaryOp::Div => match (l, r) {
                        (CType::Float64, _) | (_, CType::Float64) => CType::Float64,
                        (CType::Unknown, _) | (_, CType::Unknown) => CType::Unknown,
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
            ExprKind::List(_) | ExprKind::SequenceCons { .. } | ExprKind::Range { .. } => {
                Ok(CType::Sequence)
            }
            ExprKind::Tuple(_) => Ok(CType::Tuple),
            ExprKind::Record { .. } => Ok(CType::Record),
            ExprKind::Index { value, indices } => {
                let _ = self.infer(types, value)?;
                if let Some(index) = indices.first() {
                    let _ = self.infer(types, index)?;
                }
                Ok(CType::Unknown)
            }
            ExprKind::Call { function, args } => self.infer_call(types, function, args),
            ExprKind::FunctionAbs { param, domain, body } => {
                let mut inner = types.clone();
                inner.insert(param.clone(), ctype_from_type_expr_or_path(domain));
                let _ = self.infer(&inner, body)?;
                Ok(CType::Closure)
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
                Ok(CType::Closure)
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
        if segments.len() == 2 {
            if let Some(ty) = types.get(&segments[0]) {
                return Ok(match (ty, segments[1].as_str()) {
                    (CType::Sequence, "length") => CType::Int,
                    (CType::Rat, "numer" | "denom") | (CType::Int, "numer" | "denom") => CType::Int,
                    // Projection type is not reconstructed from a record tag.
                    // Unknown conforms to a declared field type; Schema does not.
                    (CType::Receipt, _)
                    | (CType::Record, _)
                    | (CType::Tuple, _)
                    | (CType::Code, _)
                    | (CType::Schema, _)
                    | (CType::Unknown, _) => CType::Unknown,
                    _ => CType::Unknown,
                });
            }
            if self.objects.contains_key(&segments[0]) {
                return Ok(match segments[1].as_str() {
                    "pack" | "open" => CType::Closure,
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
        if self.functions.contains_key(&name) {
            return Ok(CType::Closure);
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
                let _ = self.infer(types, domain)?;
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
            if let Some(decl) = self.functions.get(&name) {
                for arg in args {
                    let _ = self.infer(types, arg)?;
                }
                return Ok(match decl.output_types.as_slice() {
                    [ty] => ctype_from_type(ty),
                    [] => CType::Unknown,
                    _ => CType::Record,
                });
            }
            // A name the user bound (a local def, input, or prior binding)
            // is the user's; the recipe refusal is for UNBOUND names only.
            if is_refused_recipe(&name) && !types.contains_key(&name) {
                return Err(fault(
                    "method_unavailable",
                    format!("`{name}` is an ordinary module method, not a constructor operation"),
                ));
            }
            // Postfix `.field` on a non-path is parsed as `field(recv)`.
            if segments.len() == 1 && args.len() == 1 && !self.is_typed_callee(types, &name) {
                let recv = self.infer(types, &args[0])?;
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
        }
        let _ = self.infer(types, function)?;
        for arg in args {
            let _ = self.infer(types, arg)?;
        }
        Ok(CType::Unknown)
    }

    pub(super) fn is_typed_callee(&self, types: &BTreeMap<String, CType>, name: &str) -> bool {
        self.functions.contains_key(name)
            || matches!(types.get(name), Some(CType::Closure))
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
            "int_fact" | "int_double_fact" | "int_totient" => {
                expect_int_args(1, self)?;
                Ok(CType::Int)
            }
            "int_egcd" => {
                expect_int_args(2, self)?;
                Ok(CType::Sequence)
            }
            "int_sum" | "int_prod" => {
                if args.len() != 1 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects one sequence(Int) argument"),
                    ));
                }
                let got = self.infer(types, &args[0])?;
                if !got.conforms(&CType::Sequence) {
                    return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                }
                Ok(CType::Int)
            }
            "int_sum_from" | "int_prod_from" => {
                if args.len() != 2 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects a sequence(Int) and an Int start"),
                    ));
                }
                let items = self.infer(types, &args[0])?;
                if !items.conforms(&CType::Sequence) {
                    return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                }
                let start = self.infer(types, &args[1])?;
                if !start.conforms(&CType::Int) {
                    return Err(fault("type", format!("`{op}` expects Int")));
                }
                Ok(CType::Int)
            }
            "int_hamming" => {
                if args.len() != 2 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects two sequence(Int) arguments"),
                    ));
                }
                for arg in args {
                    let got = self.infer(types, arg)?;
                    if !got.conforms(&CType::Sequence) {
                        return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                    }
                }
                Ok(CType::Int)
            }
            "int_weighted_prod" => {
                if args.len() != 4 {
                    return Err(fault(
                        "arity",
                        format!("`{op}` expects sequence(Int), sequence(Int), Int, Int"),
                    ));
                }
                for arg in &args[..2] {
                    let got = self.infer(types, arg)?;
                    if !got.conforms(&CType::Sequence) {
                        return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                    }
                }
                for arg in &args[2..] {
                    let got = self.infer(types, arg)?;
                    if !got.conforms(&CType::Int) {
                        return Err(fault("type", format!("`{op}` expects Int")));
                    }
                }
                Ok(CType::Int)
            }
            "int_poly_eval" => {
                if args.len() != 3 {
                    return Err(fault("arity", format!("`{op}` expects three arguments")));
                }
                let coeffs = self.infer(types, &args[0])?;
                if !coeffs.conforms(&CType::Sequence) {
                    return Err(fault("type", format!("`{op}` expects sequence(Int)")));
                }
                for arg in &args[1..] {
                    let got = self.infer(types, arg)?;
                    if !got.conforms(&CType::Int) {
                        return Err(fault("type", format!("`{op}` expects Int")));
                    }
                }
                Ok(CType::Int)
            }
            "int_powmod" => {
                expect_int_args(3, self)?;
                Ok(CType::Int)
            }
            _ => {
                expect_int_args(2, self)?;
                Ok(CType::Int)
            }
        }
    }
}

pub(super) fn ctype_from_type_expr_or_path(expr: &Expr) -> CType {
    match &expr.kind {
        ExprKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") | Some("Nat") => CType::Int,
            Some("Bool") => CType::Bool,
            Some("Rat") => CType::Rat,
            Some("Float64") | Some("F64") => CType::Float64,
            Some("Code") => CType::Code,
            _ => CType::Unknown,
        },
        ExprKind::Call { function, .. } if is_fn_ctor(function) => CType::Closure,
        _ => CType::Unknown,
    }
}

