//! The Phase 1 admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::goal::CompileSpec;
use emath_ir::{
    Declaration, ExprId, ExprNode, Literal, NumericProfile, SafetyProfile, TypeId, TypeNode,
};
use emath_syntax::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind, Section, StmtKind, SyntaxTree, TypeExpr,
    TypeKind as SynTypeKind, UnaryOp as SynUnOp,
};
use std::collections::{BTreeMap, BTreeSet};

pub const E_DUPLICATE_FIELD: &str = "E-NAME-020";
pub const E_UNKNOWN_VARIABLE: &str = "E-TYPE-002";
pub const E_UNKNOWN_FUNCTION: &str = "E-TYPE-003";
pub const E_UNSUPPORTED_TYPE: &str = "E-TYPE-010";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEntry {
    pub phase: String,
    pub detail: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticTrace {
    pub entries: Vec<TraceEntry>,
}

impl SemanticTrace {
    pub fn record(&mut self, phase: &str, detail: impl Into<String>, span: Option<Span>) {
        self.entries.push(TraceEntry {
            phase: phase.to_string(),
            detail: detail.into(),
            span,
        });
    }
}

#[derive(Clone, Debug)]
pub struct CheckResult {
    pub package: emath_ir::SemanticPackage,
    pub diagnostics: Diagnostics,
    pub trace: SemanticTrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Infer {
    F64,
    Bool,
}

struct Admitter {
    diagnostics: Diagnostics,
    trace: Vec<TraceEntry>,
    params: BTreeMap<String, Infer>,
    inputs: BTreeMap<String, Infer>,
    states: BTreeMap<String, Infer>,
    definitions: BTreeMap<String, ExprId>,
    exprs: Vec<(ExprNode, Span)>,
    types: Vec<TypeNode>,
}

impl Admitter {
    fn new() -> Self {
        Self {
            diagnostics: Diagnostics::new(),
            trace: Vec::new(),
            params: BTreeMap::new(),
            inputs: BTreeMap::new(),
            states: BTreeMap::new(),
            definitions: BTreeMap::new(),
            exprs: Vec::new(),
            types: Vec::new(),
        }
    }

    fn type_id(&mut self, node: TypeNode) -> TypeId {
        self.types.push(node);
        TypeId(u32::try_from(self.types.len() - 1).unwrap_or(u32::MAX))
    }

    fn push_expr(&mut self, node: ExprNode, span: Span) -> ExprId {
        self.exprs.push((node, span));
        ExprId(u32::try_from(self.exprs.len() - 1).unwrap_or(u32::MAX))
    }

    fn record(&mut self, phase: &str, detail: impl Into<String>, span: Span) {
        self.trace.push(TraceEntry {
            phase: phase.to_string(),
            detail: detail.into(),
            span: Some(span),
        });
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.error(code, message, span);
    }

    fn lookup(&self, name: &str) -> Option<Infer> {
        if let Some(infer) = self.params.get(name) {
            return Some(*infer);
        }
        if let Some(infer) = self.inputs.get(name) {
            return Some(*infer);
        }
        if let Some(stripped) = name.strip_prefix("state.") {
            return self.states.get(stripped).copied();
        }
        self.definitions.get(name).map(|_| Infer::F64)
    }

    fn lower_expr(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        match &expr.kind {
            ExprKind::Int(text) => {
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Integer(text.clone())),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Float(text) => {
                let value = parse_float_constant(text);
                match value {
                    Some(value) if value.is_finite() => {
                        self.record(
                            "sema",
                            format!("constant `{text}` → strict f64"),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    _ => {
                        self.error(
                            "E-TYPE-011",
                            format!("non-finite constant `{text}` refused under strict-f64 policy"),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Bool(value) => {
                let id = self.push_expr(ExprNode::Literal(Literal::Bool(*value)), expr.source);
                Some((id, Infer::Bool))
            }
            ExprKind::Str(_) => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    "string values are outside the Phase 1 subset",
                    expr.source,
                );
                None
            }
            ExprKind::Quantity { .. } => {
                self.error(
                    "E-UNIT-001",
                    "quantity literals (e.g. `1 ms`) are outside the Phase 1 subset (Phase 5)",
                    expr.source,
                );
                None
            }
            ExprKind::Path { segments, .. } => {
                let name = segments.join(".");
                if let Some(infer) = self.lookup(&name) {
                    let id = self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                    return Some((id, infer));
                }
                self.error(
                    E_UNKNOWN_VARIABLE,
                    format!("unknown variable `{name}`"),
                    expr.source,
                );
                None
            }
            ExprKind::Call { function, args } => {
                let ExprKind::Path { segments, .. } = &function.kind else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "callable must be a plain path in the Phase 1 subset",
                        function.source,
                    );
                    return None;
                };
                let name = segments.join(".");
                let arity: Option<usize> = match name.as_str() {
                    "is_finite" | "exp" | "ln" | "log" | "sqrt" | "sin" | "cos" | "tan"
                    | "tanh" | "abs" | "floor" | "ceil" => Some(1),
                    "min" | "max" | "atan2" | "pow" => Some(2),
                    _ => {
                        self.error(
                            E_UNKNOWN_FUNCTION,
                            format!(
                                "unknown function `{name}` (Phase 1 builtins: exp, ln, log, sqrt, sin, cos, tan, tanh, abs, floor, ceil, min, max, atan2, pow, is_finite)"
                            ),
                            function.source,
                        );
                        return None;
                    }
                };
                if arity != Some(args.len()) {
                    self.error(
                        "E-TYPE-012",
                        format!(
                            "`{name}` expects {arity:?} argument(s), found {}",
                            args.len()
                        ),
                        expr.source,
                    );
                    return None;
                }
                let mut lowered = Vec::new();
                for arg in args {
                    let (id, infer) = self.lower_expr(arg)?;
                    if !matches!(infer, Infer::F64) {
                        self.error(
                            "E-TYPE-012",
                            format!("argument to `{name}` must be Float64"),
                            arg.source,
                        );
                        return None;
                    }
                    lowered.push(id);
                }
                let id = self.push_expr(
                    ExprNode::Call {
                        function: QualifiedName(name.clone()),
                        arguments: lowered,
                    },
                    expr.source,
                );
                let result = if name == "is_finite" {
                    Infer::Bool
                } else {
                    Infer::F64
                };
                Some((id, result))
            }
            ExprKind::Unary { op, value } => {
                let (id, infer) = self.lower_expr(value)?;
                match (op, infer) {
                    (SynUnOp::Neg, Infer::F64) => {
                        self.record("sema", "negate → strict negate", expr.source);
                        Some((
                            self.push_expr(
                                ExprNode::Unary {
                                    operation: emath_ir::UnaryOp::Negate,
                                    value: id,
                                },
                                expr.source,
                            ),
                            Infer::F64,
                        ))
                    }
                    (SynUnOp::Pos, Infer::F64) => Some((id, Infer::F64)),
                    (SynUnOp::Not, Infer::Bool) => Some((
                        self.push_expr(
                            ExprNode::Unary {
                                operation: emath_ir::UnaryOp::Not,
                                value: id,
                            },
                            expr.source,
                        ),
                        Infer::Bool,
                    )),
                    _ => {
                        self.error(
                            "E-TYPE-012",
                            "unary operator applied to an incompatible value",
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let (l, l_infer) = self.lower_expr(left)?;
                let (r, r_infer) = self.lower_expr(right)?;
                let arithmetic = |admitter: &mut Admitter,
                                  operation: emath_ir::BinaryOp,
                                  expr: &Expr,
                                  l: ExprId,
                                  r: ExprId| {
                    if !matches!(l_infer, Infer::F64) || !matches!(r_infer, Infer::F64) {
                        admitter.error(
                            "E-TYPE-012",
                            format!("operator `{}` requires Float64 operands", operation.name()),
                            expr.source,
                        );
                        return None;
                    }
                    Some((
                        admitter.push_expr(
                            ExprNode::Binary {
                                operation,
                                left: l,
                                right: r,
                            },
                            expr.source,
                        ),
                        Infer::F64,
                    ))
                };
                match op {
                    SynBinOp::Add => {
                        self.record("sema", "add → strict f64 add", expr.source);
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r)
                    }
                    SynBinOp::Sub => {
                        self.record("sema", "subtract → strict f64 subtract", expr.source);
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r)
                    }
                    SynBinOp::Mul => {
                        self.record("sema", "multiply → strict f64 multiply", expr.source);
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r)
                    }
                    SynBinOp::Div => {
                        self.record("sema", "divide → strict f64 divide", expr.source);
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatDiv, expr, l, r)
                    }
                    SynBinOp::Pow => {
                        self.record("sema", "power → strict f64 powf", expr.source);
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatPow, expr, l, r)
                    }
                    SynBinOp::Eq
                    | SynBinOp::Ne
                    | SynBinOp::Lt
                    | SynBinOp::Le
                    | SynBinOp::Gt
                    | SynBinOp::Ge => {
                        let operation = match op {
                            SynBinOp::Eq => emath_ir::BinaryOp::Equal,
                            SynBinOp::Ne => emath_ir::BinaryOp::NotEqual,
                            SynBinOp::Lt => emath_ir::BinaryOp::Less,
                            SynBinOp::Le => emath_ir::BinaryOp::LessEqual,
                            SynBinOp::Gt => emath_ir::BinaryOp::Greater,
                            _ => emath_ir::BinaryOp::GreaterEqual,
                        };
                        if matches!(
                            op,
                            SynBinOp::Lt | SynBinOp::Le | SynBinOp::Gt | SynBinOp::Ge
                        ) && (!matches!(l_infer, Infer::F64) || !matches!(r_infer, Infer::F64))
                        {
                            self.error(
                                "E-TYPE-012",
                                "ordered comparisons require Float64 operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation,
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                    SynBinOp::And | SynBinOp::Or => {
                        if !matches!(l_infer, Infer::Bool) || !matches!(r_infer, Infer::Bool) {
                            self.error(
                                "E-TYPE-012",
                                "boolean operators require Boolean operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation: if matches!(op, SynBinOp::And) {
                                        emath_ir::BinaryOp::And
                                    } else {
                                        emath_ir::BinaryOp::Or
                                    },
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                }
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let (cond, cond_infer) = self.lower_expr(condition)?;
                if !matches!(cond_infer, Infer::Bool) {
                    self.error(
                        "E-TYPE-012",
                        "`if` condition must be Boolean",
                        condition.source,
                    );
                    return None;
                }
                let (then_id, then_infer) = self.lower_expr(then_value)?;
                let (else_id, else_infer) = self.lower_expr(else_value)?;
                if then_infer != else_infer {
                    self.error(
                        "E-TYPE-012",
                        "`if` branches must have the same type",
                        expr.source,
                    );
                    return None;
                }
                Some((
                    self.push_expr(
                        ExprNode::If {
                            condition: cond,
                            then_value: then_id,
                            else_value: else_id,
                        },
                        expr.source,
                    ),
                    then_infer,
                ))
            }
            other => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "expression form `{}` is outside the Phase 1 strict-f64 subset",
                        expr_form_name(other)
                    ),
                    expr.source,
                );
                None
            }
        }
    }

    fn lower_requirement(&mut self, expr: &Expr) -> Option<ExprId> {
        let (id, infer) = self.lower_expr(expr)?;
        if !matches!(infer, Infer::Bool) {
            self.error(
                "E-CTOR-032",
                "`require` must be a Boolean expression",
                expr.source,
            );
            return None;
        }
        Some(id)
    }
}

fn parse_float_constant(text: &str) -> Option<f64> {
    // strip float suffix (`1e-12f32` → `1e-12`)
    let mut cleaned = text.to_string();
    for suffix in ["bf16", "f16", "f32", "f64", "f128"] {
        if let Some(stripped) = cleaned.strip_suffix(suffix) {
            cleaned = stripped.to_string();
            break;
        }
    }
    cleaned.replace('_', "").parse().ok()
}

fn expr_form_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Int(_) => "integer",
        ExprKind::Float(_) => "float",
        ExprKind::Str(_) => "string",
        ExprKind::Bool(_) => "bool",
        ExprKind::Quantity { .. } => "quantity",
        ExprKind::Path { .. } => "path",
        ExprKind::Call { .. } => "call",
        ExprKind::Index { .. } => "index",
        ExprKind::Unary { .. } => "unary",
        ExprKind::Binary { .. } => "binary",
        ExprKind::If { .. } => "if",
        ExprKind::List(_) => "list",
        ExprKind::Tuple(_) => "tuple",
        ExprKind::Range { .. } => "range",
        ExprKind::Binder { .. } => "binder",
        ExprKind::Derivative { .. } => "derivative",
        ExprKind::At { .. } => "at",
        ExprKind::On { .. } => "on",
        ExprKind::Conditioned { .. } => "conditioned",
    }
}

/// Map a surface type to a neutral type node (Phase 1 subset).
fn map_type(ty: &TypeExpr, diagnostics: &mut Diagnostics) -> Option<TypeNode> {
    let SynTypeKind::Path {
        segments,
        generic_args,
    } = &ty.kind
    else {
        diagnostics.error(
            E_UNSUPPORTED_TYPE,
            format!(
                "type `{}` is outside the Phase 1 subset (scalar Float64/Real/Bool only)",
                type_display(ty)
            ),
            ty.source,
        );
        return None;
    };
    let leaf = segments.last().map_or("", String::as_str);
    match leaf {
        "Real" | "Float64" | "float64" | "f64" => Some(TypeNode::Float64),
        "Bool" => Some(TypeNode::Bool),
        "Self" => Some(TypeNode::Other(QualifiedName("Self".into()))),
        "Length" | "Time" | "Duration" | "Velocity" | "Mass" | "Damping" | "Stiffness"
        | "Force" | "Temperature" | "Information" | "Token" | "Byte" | "MiB" | "m" | "s" | "K"
        | "kg" | "N" | "Hz" | "W" => {
            diagnostics.error(
                "E-UNIT-001",
                format!(
                    "type `{leaf}` carries units/refinements outside the Phase 1 subset \
                     (unit system arrives in Phase 5)"
                ),
                ty.source,
            );
            None
        }
        "Option"
        | "Vector"
        | "Matrix"
        | "Tensor"
        | "NonNegative"
        | "Positive"
        | "Per"
        | "Interval"
        | "Sequence"
        | "Set"
        | "Array"
        | "Field"
        | "DirectedGraph"
        | "SearchResult"
        | "RecoveryCertificate"
        | "RequestProfile"
        | "ArtifactId"
        | "NodeId"
        | "CacheCandidate"
        | "Route"
        | "Witness"
        | "Nat"
        | "Int"
        | "Rational"
        | "Probability" => {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "type `{leaf}` is outside the Phase 1 subset (Phase 4/5 introduces refinements, shapes and units)"
                ),
                ty.source,
            );
            None
        }
        "Result" => {
            let error_name = generic_args
                .get(1)
                .map_or_else(|| "ConfigError".to_string(), type_display);
            Some(TypeNode::Other(QualifiedName(error_name)))
        }
        other => {
            diagnostics.error("E-TYPE-001", format!("unknown type `{other}`"), ty.source);
            None
        }
    }
}

fn type_display(expr: &TypeExpr) -> String {
    match &expr.kind {
        SynTypeKind::Path { segments, .. } => segments.join("::"),
        SynTypeKind::List(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(type_display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        SynTypeKind::Tuple(items) => {
            format!(
                "({})",
                items
                    .iter()
                    .map(type_display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        SynTypeKind::Ref(inner) => format!("&{}", type_display(inner)),
        SynTypeKind::Product(items) => format!(
            "({})",
            items
                .iter()
                .map(type_display)
                .collect::<Vec<_>>()
                .join(" * ")
        ),
    }
}

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
type AdmitResult = (
    Option<Declaration>,
    Vec<TestCase>,
    Vec<TypeNode>,
    Vec<(ExprNode, Span)>,
    Vec<TraceEntry>,
    Diagnostics,
);

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
pub fn admit_declaration(decl: &emath_syntax::tree::Declaration) -> AdmitResult {
    let mut admitter = Admitter::new();
    let kind_label = decl.as_kind.clone();
    let is_policy = kind_label == "policy";

    // Section collection with duplicate detection (E-SYN-103).
    let mut by_name: BTreeMap<&str, &Section> = BTreeMap::new();
    for section in decl.sections() {
        if let Some(previous) = by_name.get(section.name.as_str()) {
            admitter.error(
                "E-SYN-103",
                format!(
                    "duplicate section `{}` (first declared at bytes {}..{})",
                    section.name, previous.source.start, previous.source.end
                ),
                section.source,
            );
        } else {
            by_name.insert(&section.name, section);
        }
    }

    // Fields: inputs, outputs, state.
    let mut fields_infer: BTreeMap<String, Infer> = BTreeMap::new();
    let mut fields_by_section: BTreeMap<&str, Vec<Field>> = BTreeMap::new();
    let mut insert_field = |admitter: &mut Admitter,
                            section_name: &'static str,
                            name: &str,
                            infer: Infer,
                            ty_id: TypeId,
                            span: Span| {
        if fields_infer.contains_key(name) {
            admitter.error(
                E_DUPLICATE_FIELD,
                format!("duplicate field `{name}` (declared in section `{section_name}`)"),
                span,
            );
            return;
        }
        fields_infer.insert(name.to_string(), infer);
        fields_by_section
            .entry(section_name)
            .or_default()
            .push(Field {
                name: name.to_string(),
                ty: ty_id,
                visibility: Visibility::Public,
                source: span,
            });
    };

    for section_name in ["inputs", "outputs", "state"] {
        if let Some(section) = by_name.get(section_name) {
            for stmt in &section.suite.statements {
                let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                    admitter.error(
                        "E-SYN-101",
                        format!("only `name: Type` declarations are allowed in `{section_name}`"),
                        stmt.source,
                    );
                    continue;
                };
                let Some(node) = map_type(ty, &mut admitter.diagnostics) else {
                    continue;
                };
                let infer = match &node {
                    TypeNode::Bool => Infer::Bool,
                    _ => Infer::F64,
                };
                let ty_id = admitter.type_id(node);
                admitter.record(
                    "sema",
                    format!(
                        "field `{name}` typed as {}",
                        ty_display(admitter.types.get(ty_id.index()).unwrap())
                    ),
                    stmt.source,
                );
                insert_field(&mut admitter, section_name, name, infer, ty_id, stmt.source);
            }
        }
    }

    let inputs = fields_by_section.get("inputs").cloned().unwrap_or_default();
    let outputs_raw = fields_by_section
        .get("outputs")
        .cloned()
        .unwrap_or_default();
    let state = fields_by_section.get("state").cloned().unwrap_or_default();
    admitter.inputs = inputs
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();
    admitter.states = state
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();
    for output in &outputs_raw {
        let _ = output;
    }

    // Definitions.
    let mut definitions: BTreeMap<String, ExprId> = BTreeMap::new();
    if let Some(section) = by_name.get("definitions") {
        for stmt in &section.suite.statements {
            let StmtKind::Assign { target, value } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only `name = expression` definitions are allowed in Phase 1",
                    stmt.source,
                );
                continue;
            };
            if target.segments.len() != 1 || !target.indices.is_empty() {
                admitter.error(
                    E_UNSUPPORTED_TYPE,
                    "indexed and nested definitions are outside the Phase 1 subset",
                    target.source,
                );
                continue;
            }
            let name = &target.segments[0];
            if !outputs_raw.iter().any(|o| &o.name == name) {
                admitter.error(
                    "E-NAME-024",
                    format!(
                        "definition of `{name}` is not an output (Phase 1 defines outputs only)"
                    ),
                    target.source,
                );
                continue;
            }
            if definitions.contains_key(name) {
                admitter.error(
                    E_DUPLICATE_FIELD,
                    format!("duplicate definition `{name}`"),
                    target.source,
                );
                continue;
            }
            match admitter.lower_expr(value) {
                Some((id, Infer::F64)) => {
                    admitter.record(
                        "sema",
                        format!("definition `{name}` typed Float64"),
                        value.source,
                    );
                    definitions.insert(name.clone(), id);
                }
                Some((_, Infer::Bool)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("definition `{name}` must be Float64 in Phase 1"),
                        value.source,
                    );
                }
                None => {}
            }
        }
    }
    for output in &outputs_raw {
        if !definitions.contains_key(&output.name) {
            admitter.error(
                "E-NAME-023",
                format!("output `{}` has no definition", output.name),
                output.source,
            );
        }
    }
    admitter.definitions = definitions.clone();

    // Constructors.
    let mut constructors: Vec<Constructor> = Vec::new();
    if is_policy {
        if let Some(section) = by_name.get("constructors") {
            for stmt in &section.suite.statements {
                if let StmtKind::FnDecl {
                    visibility,
                    name,
                    params,
                    ret,
                    suite,
                    ..
                } = &stmt.kind
                {
                    if name != "new"
                        || !matches!(visibility, Some(emath_syntax::tree::Visibility::Public))
                    {
                        admitter.error(
                            "E-CTOR-036",
                            format!(
                                "Phase 1 admits exactly one public `new` constructor, found `{name}`"
                            ),
                            stmt.source,
                        );
                        continue;
                    }
                    if !constructors.is_empty() {
                        admitter.error(
                            "E-CTOR-036",
                            "multiple public `new` constructors are outside the Phase 1 subset",
                            stmt.source,
                        );
                        continue;
                    }
                    let mut constructor = admit_constructor(
                        &mut admitter,
                        params,
                        ret.as_ref(),
                        suite.as_ref(),
                        stmt.source,
                    );
                    constructor.name.clone_from(name);
                    constructor.is_public = true;
                    constructors.push(constructor);
                } else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `public fn new(...)` inside `constructors:`",
                        stmt.source,
                    );
                }
            }
        } else {
            admitter.error(
                "E-CTOR-031",
                "policy declarations require a `constructors:` section with a public `new`",
                decl.head_source,
            );
        }
        // Constructor assignments must cover all state fields.
        if let Some(constructor) = constructors.first() {
            for field in &state {
                if !constructor.assignments.contains_key(&field.name) {
                    admitter.error(
                        "E-CTOR-030",
                        format!("missing state assignment for `{}`", field.name),
                        decl.head_source,
                    );
                }
            }
        }
    } else if let Some(section) = by_name.get("constructors") {
        admitter.error(
            "E-KIND-010",
            "function declarations cannot have state or constructors in Phase 1",
            section.source,
        );
    }
    if !is_policy && !state.is_empty() {
        admitter.error(
            "E-KIND-010",
            "function declarations cannot have state in Phase 1",
            decl.head_source,
        );
    }

    // Compile spec.
    let compile_spec = admit_compile_spec(&mut admitter, by_name.get("compile").copied());

    // Exports.
    let mut exports = Vec::new();
    if let Some(section) = by_name.get("exports") {
        for stmt in &section.suite.statements {
            let StmtKind::Command { head, .. } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "exports must be `public <kind> <name>` commands",
                    stmt.source,
                );
                continue;
            };
            let mut words = head.iter().map(String::as_str);
            let visibility_word = words.next().unwrap_or("");
            let kind = words.next().unwrap_or("");
            let name = words.next().unwrap_or("");
            let public = visibility_word == "public";
            if !public {
                admitter.error(
                    "E-NAME-021",
                    "Phase 1 exports must be `public`",
                    stmt.source,
                );
                continue;
            }
            match kind {
                "constructor" => {
                    if name != "new" || constructors.is_empty() {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported constructor `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "constructor".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "function" => {
                    if !definitions.contains_key(name)
                        && !outputs_raw.iter().any(|o| o.name == *name)
                    {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported function `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "function".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "type" => {
                    if name != decl.name {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported type `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "type".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                other => {
                    admitter.error(
                        "E-NAME-021",
                        format!("unsupported export kind `{other}`"),
                        stmt.source,
                    );
                }
            }
        }
    }

    // Tests.
    let mut tests: Vec<TestCase> = Vec::new();
    if let Some(section) = by_name.get("tests") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `example <name>:` blocks inside `tests:`",
                    stmt.source,
                );
                continue;
            };
            if example.name != "example" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown test block `{}`", example.name),
                    example.source,
                );
                continue;
            }
            let mut given: BTreeMap<String, ExprId> = BTreeMap::new();
            let mut expect: Option<ExprId> = None;
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        if !admitter.inputs.contains_key(name)
                            && !admitter.params.contains_key(name)
                        {
                            admitter.error(
                                "E-NAME-026",
                                format!("`given` name `{name}` is not an input or constructor parameter"),
                                inner.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((id, Infer::F64)) => {
                                given.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be Float64"),
                                    inner.source,
                                );
                            }
                            None => {}
                        }
                    }
                    StmtKind::Expect(expr) => match admitter.lower_expr(expr) {
                        Some((id, Infer::Bool)) => expect = Some(id),
                        Some((_, Infer::F64)) => {
                            admitter.error(
                                "E-TYPE-012",
                                "`expect` must be a Boolean comparison",
                                inner.source,
                            );
                        }
                        None => {}
                    },
                    other => {
                        let _ = other;
                        admitter.error(
                            "E-SYN-101",
                            "only `given x = ...` and `expect ...` are allowed in example blocks",
                            inner.source,
                        );
                    }
                }
            }
            let Some(expect) = expect else {
                admitter.error(
                    "E-NAME-026",
                    format!(
                        "example `{}` has no `expect`",
                        example.generic.clone().unwrap_or_default()
                    ),
                    example.source,
                );
                continue;
            };
            if is_policy {
                // constructor parameters must be supplied by `given` values
                let constructor_params: Vec<String> = constructors
                    .first()
                    .map(|c| c.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();
                for param in &constructor_params {
                    if !given.contains_key(param) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "policy example `{}` must supply constructor parameter `{param}` via `given`",
                                example.generic.clone().unwrap_or_default()
                            ),
                            example.source,
                        );
                    }
                }
            }
            tests.push(TestCase {
                name: example
                    .generic
                    .clone()
                    .unwrap_or_else(|| format!("test_{}", tests.len())),
                given,
                expect,
                source: example.source,
            });
        }
    }

    // Rebuild inputs/outputs/state as neutral fields.
    let input_fields = inputs.clone();
    let output_fields = outputs_raw.clone();
    let state_fields = state.clone();

    let declaration = Declaration {
        id: emath_ir::DeclarationId(0),
        name: QualifiedName::single(decl.name.clone()),
        kind: QualifiedName::single(if is_policy { "policy" } else { "function" }),
        kind_label,
        inputs: input_fields,
        outputs: output_fields,
        state: state_fields,
        constructors,
        definitions,
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports,
        compile_spec,
        source: decl.source,
    };

    (
        Some(declaration),
        tests,
        admitter.types,
        admitter.exprs,
        admitter.trace,
        admitter.diagnostics,
    )
}

fn ty_display(node: &TypeNode) -> String {
    node.display_name()
}

impl Admitter {
    fn type_of(&self, id: TypeId) -> Infer {
        match self.types.get(id.index()) {
            Some(TypeNode::Bool) => Infer::Bool,
            _ => Infer::F64,
        }
    }
}

fn admit_constructor(
    admitter: &mut Admitter,
    params: &[emath_syntax::tree::Param],
    ret: Option<&TypeExpr>,
    suite: Option<&emath_syntax::tree::Suite>,
    source: Span,
) -> Constructor {
    let mut parameters = Vec::new();
    let mut param_names = BTreeSet::new();
    for param in params {
        if !param_names.insert(param.name.clone()) {
            admitter.error(
                "E-CTOR-034",
                format!("duplicate constructor parameter `{}`", param.name),
                param.source,
            );
            continue;
        }
        let Some(node) = map_type(&param.ty, &mut admitter.diagnostics) else {
            continue;
        };
        let infer = match &node {
            TypeNode::Bool => Infer::Bool,
            _ => Infer::F64,
        };
        let ty_id = admitter.type_id(node);
        admitter.params.insert(param.name.clone(), infer);
        parameters.push(Field {
            name: param.name.clone(),
            ty: ty_id,
            visibility: Visibility::Public,
            source: param.source,
        });
    }

    let mut preconditions = Vec::new();
    let mut assignments: BTreeMap<String, ExprId> = BTreeMap::new();
    let mut postconditions = Vec::new();
    let mut error_type = None;
    if let Some(ret) = ret {
        if let Some(node) = map_type(ret, &mut admitter.diagnostics) {
            error_type = Some(admitter.type_id(node));
        }
    }

    // Inputs are not visible while constructing: save and restore scopes.
    let saved_inputs = std::mem::take(&mut admitter.inputs);
    if let Some(suite) = suite {
        for stmt in &suite.statements {
            match &stmt.kind {
                StmtKind::Require(expr) => {
                    if let Some(id) = admitter.lower_requirement(expr) {
                        admitter.record(
                            "sema",
                            format!(
                                "constructor precondition #{} enforced",
                                preconditions.len() + 1
                            ),
                            stmt.source,
                        );
                        preconditions.push(id);
                    }
                }
                StmtKind::Ensure(expr) | StmtKind::Invariant(expr) => {
                    if let Some(id) = admitter.lower_requirement(expr) {
                        postconditions.push(id);
                    }
                }
                StmtKind::SelfBlock { assignments: block } => {
                    for (name, value) in block {
                        if !admitter.states.contains_key(name) {
                            admitter.error(
                                "E-CTOR-033",
                                format!("`{name}` is not a state field"),
                                stmt.source,
                            );
                            continue;
                        }
                        if assignments.contains_key(name) {
                            admitter.error(
                                "E-CTOR-035",
                                format!("duplicate assignment for state field `{name}`"),
                                stmt.source,
                            );
                            continue;
                        }
                        // state references are not readable during construction
                        if contains_state_reference(value) {
                            admitter.error(
                                "E-CTOR-033",
                                format!(
                                    "constructor cannot read `state.{name}` while constructing"
                                ),
                                value.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((id, Infer::F64)) => {
                                assignments.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("state field `{name}` must be Float64"),
                                    value.source,
                                );
                            }
                            None => {}
                        }
                    }
                }
                other => {
                    let _ = other;
                    admitter.error(
                        "E-SYN-101",
                        "only `require`, `ensure`, `invariant` and `Self:` blocks are allowed in constructors",
                        stmt.source,
                    );
                }
            }
        }
    }
    admitter.inputs = saved_inputs;
    Constructor {
        name: "new".to_string(),
        parameters,
        preconditions,
        assignments,
        postconditions,
        defaults: BTreeMap::new(),
        error_type,
        is_public: true,
        source,
    }
}

fn contains_state_reference(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.first().is_some_and(|s| s == "state"),
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => false,
        ExprKind::Quantity { value, .. } | ExprKind::Unary { value, .. } => {
            contains_state_reference(value)
        }
        ExprKind::Call { function, args } => {
            contains_state_reference(function) || args.iter().any(contains_state_reference)
        }
        ExprKind::Binary { left, right, .. } => {
            contains_state_reference(left) || contains_state_reference(right)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            contains_state_reference(condition)
                || contains_state_reference(then_value)
                || contains_state_reference(else_value)
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            items.iter().any(contains_state_reference)
        }
        ExprKind::Index { value, indices } => {
            contains_state_reference(value) || indices.iter().any(contains_state_reference)
        }
        ExprKind::Range { start, end, .. } => {
            start.as_ref().is_some_and(|e| contains_state_reference(e))
                || end.as_ref().is_some_and(|e| contains_state_reference(e))
        }
        ExprKind::Binder { binders, body, .. } => {
            binders
                .iter()
                .any(|b| b.domain.as_ref().is_some_and(contains_state_reference))
                || contains_state_reference(body)
        }
        ExprKind::Derivative { value, wrt } => {
            contains_state_reference(value)
                || wrt
                    .as_ref()
                    .is_some_and(|v| v.iter().any(contains_state_reference))
        }
        ExprKind::At { value, location } | ExprKind::On { value, location } => {
            contains_state_reference(value) || contains_state_reference(location)
        }
        ExprKind::Conditioned { value, condition } => {
            contains_state_reference(value) || contains_state_reference(condition)
        }
    }
}

fn admit_compile_spec(admitter: &mut Admitter, section: Option<&Section>) -> CompileSpec {
    let mut spec = CompileSpec {
        target: "rust".into(),
        profile: "library".into(),
        numeric: NumericProfile::StrictF64,
        safety: SafetyProfile::ForbidUnsafe,
        unresolved: None,
    };
    let Some(section) = section else {
        admitter.record(
            "sema",
            "compile section absent; defaults: rust/library/strict-f64/forbid-unsafe",
            Span::default(),
        );
        return spec;
    };
    for stmt in &section.suite.statements {
        let StmtKind::Command { head, argument } = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "compile directives must be commands (e.g. `target rust`)",
                stmt.source,
            );
            continue;
        };
        let key = head.first().map_or("", String::as_str);
        let value_text = match argument {
            Some(emath_syntax::tree::CommandArgument::Expr(expr)) => match &expr.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                _ => None,
            },
            _ => None,
        }
        .or_else(|| head.get(1).cloned());
        let value_text = value_text.unwrap_or_default();
        match key {
            "target" => {
                if value_text != "rust" {
                    admitter.error(
                        "E-CODEGEN-051",
                        format!(
                            "compile target `{value_text}` is outside the Phase 1 subset (rust)"
                        ),
                        stmt.source,
                    );
                }
                spec.target = value_text;
            }
            "profile" => {
                if value_text != "library" {
                    admitter.error(
                        "E-CODEGEN-052",
                        format!(
                            "compile profile `{value_text}` is outside the Phase 1 subset (library)"
                        ),
                        stmt.source,
                    );
                }
                spec.profile = value_text;
            }
            "numeric" => {
                if value_text != "strict-f64" {
                    admitter.error(
                        "E-CODEGEN-053",
                        format!(
                            "numeric profile `{value_text}` is outside the Phase 1 subset (strict-f64)"
                        ),
                        stmt.source,
                    );
                }
                spec.numeric = NumericProfile::StrictF64;
            }
            "safety" => {
                if value_text != "forbid-unsafe" {
                    admitter.error(
                        "E-CODEGEN-054",
                        format!(
                            "safety profile `{value_text}` is outside the Phase 1 subset (forbid-unsafe)"
                        ),
                        stmt.source,
                    );
                }
                spec.safety = SafetyProfile::ForbidUnsafe;
            }
            "unresolved" => {
                admitter.error(
                    "E-CODEGEN-055",
                    "`unresolved <disposition>` is outside the Phase 1 subset (native only)",
                    stmt.source,
                );
                spec.unresolved = Some(value_text);
            }
            other => {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown compile directive `{other}`"),
                    stmt.source,
                );
            }
        }
    }
    spec
}

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree, _unknown_sections: &()) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    // Front-end: package identity and `use` imports. External file
    // imports remain a Phase 2 refusal (E-PKG-050).
    let has_recognition_items = tree.items.iter().any(|item| match item {
        emath_syntax::tree::Item::Package { .. } | emath_syntax::tree::Item::Use { .. } => true,
        emath_syntax::tree::Item::Declaration(decl) => decl.item_kind != "custom",
    });
    let recognition = if has_recognition_items {
        let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
        package.package_path = front_end.package_path;
        package.imports = front_end.imports;
        Some(crate::recognition::collect_kind_defs(tree))
    } else {
        None
    };

    let mut declaration_id = 0_u32;
    for item in &tree.items {
        let emath_syntax::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if let Some(kind_defs) = &recognition {
            if decl.item_kind != "custom" {
                crate::recognition::admit_declaration(
                    decl,
                    kind_defs,
                    &mut package,
                    &mut diagnostics,
                    &mut trace,
                );
                continue;
            }
        }
        if decl.item_kind != "custom" {
            diagnostics.error(
                "E-KIND-001",
                format!(
                    "declaration kind `{}` is not supported; Phase 1 uses `emath custom`",
                    decl.item_kind
                ),
                decl.head_source,
            );
            continue;
        }
        if decl.as_kind != "function" && decl.as_kind != "policy" {
            diagnostics.error(
                "E-KIND-100",
                format!(
                    "declaration type `{}` is outside the Phase 1 subset (function, policy)",
                    decl.as_kind
                ),
                decl.head_source,
            );
            continue;
        }
        let (declaration, tests, types, exprs, entries, admit_diagnostics) =
            admit_declaration(decl);
        diagnostics.extend_from(&admit_diagnostics);
        trace.entries.extend(entries);
        let Some(mut declaration) = declaration else {
            diagnostics.error(
                "E-KIND-002",
                "declaration could not be admitted",
                decl.head_source,
            );
            continue;
        };
        declaration.id = emath_ir::DeclarationId(declaration_id);
        declaration_id += 1;
        package.types.extend(types);
        package.exprs.extend(exprs.iter().map(|(e, _)| e.clone()));
        package.expr_spans.extend(exprs.iter().map(|(_, s)| *s));
        for test in tests {
            declaration.tests.push(package.push_test(test));
        }
        package.declarations.push(declaration);
    }
    if !package.declarations.is_empty() {
        package.seal();
    }
    CheckResult {
        package,
        diagnostics,
        trace,
    }
}
