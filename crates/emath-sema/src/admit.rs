//! The Phase 1 admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind, Section, StmtKind, SyntaxTree, TypeExpr,
    TypeKind as SynTypeKind, UnaryOp as SynUnOp,
};
use emath_core::{Diagnostic, Diagnostics, QualifiedName, SchemaId, Severity, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::{CompileSpec, EvidenceLevel};
use emath_core::tree::CommandArgument;
use emath_ir::{
    Declaration, ExprId, ExprNode, HostBinding, HostMethod, ImportEntry, ImportSelection,
    KindSchema, Literal, NumericProfile, RepeatPolicy, SafetyProfile, TypeId, TypeNode, Unit,
    UnitDim, UnitFamily, check_compatible, check_error_limit, check_precision_demand, lookup_unit,
    parse_numeric_profile, per_unit,
};
use std::collections::{BTreeMap, BTreeSet};

pub const E_DUPLICATE_FIELD: &str = "E-NAME-020";
pub const E_UNKNOWN_VARIABLE: &str = "E-TYPE-002";
pub const E_UNKNOWN_FUNCTION: &str = "E-TYPE-003";
pub const E_UNSUPPORTED_TYPE: &str = "E-TYPE-010";

/// Sections the Phase 1 admission pass consumes; any other section is
/// refused with `E-SEC-101` instead of being silently dropped.
const PHASE1_SECTIONS: &[&str] = &[
    "inputs",
    "outputs",
    "state",
    "definitions",
    "constructors",
    "goals",
    "exports",
    "tests",
    "compile",
    "about",
    "evidence",
    "host",
];

/// Folds a declaration name for confusable-collision detection (spec
/// `01_LEXICAL_LAYOUT_AND_SOURCE.md`: confusable lint for public
/// declarations). Glyphs visually identical to Latin letters map to their
/// Latin equivalent; everything else is identity (ASCII folds
/// byte-for-byte). Two names that fold alike are distinguishable only by
/// lookalike glyphs, so admission refuses the second as `E-NAME-024`.
///
/// Seed set (Latin/Cyrillic/Greek lookalikes; not a full Unicode
/// confusables table, which would require external data).
#[must_use]
pub fn confusable_fold(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        out.push(match ch {
            // Cyrillic lowercase lookalikes.
            '\u{0430}' | '\u{03B1}' => 'a', // а, α
            '\u{0435}' => 'e',              // е
            '\u{043A}' | '\u{03BA}' => 'k', // к, κ
            '\u{043C}' | '\u{03BC}' => 'm', // м, μ
            '\u{043D}' => 'h',              // н
            '\u{043E}' | '\u{03BF}' => 'o', // о, ο
            '\u{0440}' | '\u{03C1}' => 'p', // р, ρ
            '\u{0441}' => 'c',              // с
            '\u{0442}' | '\u{03C4}' => 't', // т, τ
            '\u{0443}' => 'y',              // у
            '\u{0445}' | '\u{03C7}' => 'x', // х, χ
            '\u{0455}' => 's',              // ѕ
            '\u{0456}' | '\u{03B9}' => 'i', // і, ι
            '\u{0458}' => 'j',              // ј
            // Cyrillic uppercase lookalikes.
            '\u{0410}' => 'A',              // А
            '\u{0415}' => 'E',              // Е
            '\u{041A}' | '\u{039A}' => 'K', // К, Κ
            '\u{041C}' | '\u{039C}' => 'M', // М, Μ
            '\u{041D}' => 'H',              // Н
            '\u{041E}' | '\u{039F}' => 'O', // О, Ο
            '\u{0420}' | '\u{03A1}' => 'P', // Р, Ρ
            '\u{0421}' => 'C',              // С
            '\u{0422}' | '\u{03A4}' => 'T', // Т, Τ
            '\u{0423}' => 'Y',              // У
            '\u{0425}' | '\u{03A7}' => 'X', // Х, Χ
            '\u{0405}' => 'S',              // Ѕ
            '\u{0406}' | '\u{0399}' => 'I', // І, Ι
            '\u{0408}' => 'J',              // Ј
            // Greek lowercase lookalikes.
            '\u{03BD}' => 'v', // ν
            // Greek uppercase lookalikes.
            '\u{039D}' => 'N', // Ν
            other => other,
        });
    }
    out
}

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
    Unit { dims: UnitDim, family: UnitFamily },
    /// Whole host-imported record. Not a scalar.
    Opaque,
    /// Host-deferred field access; numeric use is admitted without fabricating a field type.
    HostDeferred,
}

impl Infer {
    fn from_unit(unit: &Unit) -> Self {
        if unit.dims == UnitDim::one() {
            Self::F64
        } else {
            Self::Unit {
                dims: unit.dims,
                family: unit.family,
            }
        }
    }

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
    host_types: BTreeSet<String>,
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
            host_types: BTreeSet::new(),
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

    fn note(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic {
            code,
            severity: Severity::Note,
            message: message.into(),
            primary: span,
            related: Vec::new(),
            help: None,
        });
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
            ExprKind::Quantity { value, unit } => {
                let name = unit.last().map_or("", String::as_str);
                match lookup_unit(name) {
                    Ok(looked_up) => {
                        let inner = match &value.kind {
                            ExprKind::Int(text) | ExprKind::Float(text) => text.as_str(),
                            _ => {
                                self.error(
                                    "E-UNIT-105",
                                    "quantity value must be a numeric literal",
                                    expr.source,
                                );
                                return None;
                            }
                        };
                        let parsed = parse_float_constant(inner);
                        match parsed {
                            Some(number) if number.is_finite() => {
                                self.record(
                                    "sema",
                                    format!("quantity `{inner} {name}` → {}", looked_up.name),
                                    expr.source,
                                );
                                let id = self.push_expr(
                                    ExprNode::Literal(Literal::FloatBits(number.to_bits())),
                                    expr.source,
                                );
                                Some((id, Infer::from_unit(&looked_up)))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-011",
                                    format!(
                                        "non-finite quantity `{inner} {name}` refused under the selected numeric model"
                                    ),
                                    expr.source,
                                );
                                None
                            }
                        }
                    }
                    Err(error) => {
                        self.error(error.code, error.message, expr.source);
                        None
                    }
                }
            }
            ExprKind::Path { segments, .. } => {
                let name = segments.join(".");
                if let Some(infer) = self.lookup(&name) {
                    let id = self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                    return Some((id, infer));
                }
                if segments.len() >= 2 {
                    if matches!(self.lookup(&segments[0]), Some(Infer::Opaque)) {
                        self.record(
                            "sema",
                            format!("host field `{name}` deferred to the host boundary"),
                            expr.source,
                        );
                        let id =
                            self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                        return Some((id, Infer::HostDeferred));
                    }
                }
                if segments.len() == 1 {
                    if let Ok(unit) = lookup_unit(&segments[0]) {
                        self.record(
                            "sema",
                            format!("unit literal `{}` → {}", segments[0], unit.name),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
                            expr.source,
                        );
                        return Some((id, Infer::from_unit(&unit)));
                    }
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
                    if !matches!(infer, Infer::F64 | Infer::HostDeferred) {
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
                    (SynUnOp::Neg, Infer::F64 | Infer::Unit { .. } | Infer::HostDeferred) => {
                        self.record("sema", "negate → strict negate", expr.source);
                        Some((
                            self.push_expr(
                                ExprNode::Unary {
                                    operation: emath_ir::UnaryOp::Negate,
                                    value: id,
                                },
                                expr.source,
                            ),
                            infer,
                        ))
                    }
                    (SynUnOp::Pos, Infer::F64 | Infer::Unit { .. } | Infer::HostDeferred) => {
                        Some((id, infer))
                    }
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
                                  r: ExprId,
                                  result: Infer| {
                    Some((
                        admitter.push_expr(
                            ExprNode::Binary {
                                operation,
                                left: l,
                                right: r,
                            },
                            expr.source,
                        ),
                        result,
                    ))
                };
                match op {
                    SynBinOp::Add => {
                        self.record("sema", "add → strict f64 add", expr.source);
                        let result = combine_numeric(l_infer, r_infer, NumericCombine::Add, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r, result)
                    }
                    SynBinOp::Sub => {
                        self.record("sema", "subtract → strict f64 subtract", expr.source);
                        let result = combine_numeric(l_infer, r_infer, NumericCombine::Add, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r, result)
                    }
                    SynBinOp::Mul => {
                        self.record("sema", "multiply → strict f64 multiply", expr.source);
                        let result = combine_numeric(l_infer, r_infer, NumericCombine::Mul, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r, result)
                    }
                    SynBinOp::Div => {
                        self.record("sema", "divide → strict f64 divide", expr.source);
                        let result = combine_numeric(l_infer, r_infer, NumericCombine::Div, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatDiv, expr, l, r, result)
                    }
                    SynBinOp::Pow => {
                        self.record("sema", "power → strict f64 powf", expr.source);
                        if !matches!(
                            (l_infer, r_infer),
                            (
                                Infer::F64 | Infer::HostDeferred,
                                Infer::F64 | Infer::HostDeferred
                            )
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "operator `^` requires dimensionless Float64 operands",
                                expr.source,
                            );
                            return None;
                        }
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatPow, expr, l, r, Infer::F64)
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
                        ) && !comparable_numeric(l_infer, r_infer)
                        {
                            self.error(
                                "E-UNIT-101",
                                "ordered comparisons require dimensionally compatible numeric operands",
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

#[derive(Clone, Copy)]
enum NumericCombine {
    Add,
    Mul,
    Div,
}

fn infer_from_node(node: &TypeNode) -> Infer {
    match node {
        TypeNode::Bool => Infer::Bool,
        TypeNode::UnitRef { name } => unit_infer_from_name(name),
        TypeNode::Refinement { base, .. } | TypeNode::Interval(base) => infer_from_node(base),
        TypeNode::Opaque { .. } => Infer::Opaque,
        _ => Infer::F64,
    }
}

fn unit_infer_from_name(name: &str) -> Infer {
    if let Ok(unit) = lookup_unit(name) {
        return Infer::from_unit(&unit);
    }
    if let Some(inner) = name.strip_prefix("1/") {
        if let Ok(unit) = lookup_unit(inner) {
            return Infer::Unit {
                dims: UnitDim::one().div(unit.dims),
                family: unit.family,
            };
        }
    }
    Infer::F64
}

fn comparable_numeric(left: Infer, right: Infer) -> bool {
    match (left, right) {
        (Infer::F64, Infer::F64) => true,
        (Infer::HostDeferred, Infer::F64)
        | (Infer::F64, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::Unit { .. })
        | (Infer::Unit { .. }, Infer::HostDeferred) => true,
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
        ) => left_family == right_family && left_dims == right_dims,
        _ => false,
    }
}

fn combine_numeric(
    left: Infer,
    right: Infer,
    combine: NumericCombine,
    expr: &Expr,
    admitter: &mut Admitter,
) -> Option<Infer> {
    match (left, right, combine) {
        (Infer::Opaque, _, _) | (_, Infer::Opaque, _) => {
            admitter.error(
                "E-TYPE-012",
                "opaque host value is not a scalar; access a field",
                expr.source,
            );
            None
        }
        (Infer::HostDeferred, Infer::HostDeferred, _) => Some(Infer::F64),
        (Infer::HostDeferred, Infer::F64, _) | (Infer::F64, Infer::HostDeferred, _) => {
            Some(Infer::F64)
        }
        (Infer::HostDeferred, Infer::Unit { .. }, NumericCombine::Add)
        | (Infer::Unit { .. }, Infer::HostDeferred, NumericCombine::Add) => {
            if let Infer::Unit { dims, family } = left {
                Some(Infer::Unit { dims, family })
            } else if let Infer::Unit { dims, family } = right {
                Some(Infer::Unit { dims, family })
            } else {
                Some(Infer::F64)
            }
        }
        (Infer::HostDeferred, Infer::Unit { .. }, NumericCombine::Mul | NumericCombine::Div)
        | (Infer::Unit { .. }, Infer::HostDeferred, NumericCombine::Mul | NumericCombine::Div) => {
            Some(Infer::F64)
        }
        (Infer::F64, Infer::F64, _) => Some(Infer::F64),
        (Infer::Unit { .. }, Infer::F64, NumericCombine::Add)
        | (Infer::F64, Infer::Unit { .. }, NumericCombine::Add) => {
            admitter.error(
                "E-UNIT-101",
                "dimension mismatch: cannot add a quantity to a dimensionless value",
                expr.source,
            );
            None
        }
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Add,
        ) => {
            let dummy_left = Unit::with_family("left".into(), left_dims, 1.0, 0.0, left_family);
            let dummy_right = Unit::with_family("right".into(), right_dims, 1.0, 0.0, right_family);
            match check_compatible(&dummy_left, &dummy_right) {
                Ok(()) => Some(left),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        (Infer::Unit { dims, family }, Infer::F64, NumericCombine::Mul | NumericCombine::Div)
        | (Infer::F64, Infer::Unit { dims, family }, NumericCombine::Mul) => {
            Some(Infer::Unit { dims, family })
        }
        (Infer::F64, Infer::Unit { dims, family }, NumericCombine::Div) => Some(Infer::Unit {
            dims: UnitDim::one().div(dims),
            family,
        }),
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Mul,
        ) => {
            let dummy_left = Unit::with_family("left".into(), left_dims, 1.0, 0.0, left_family);
            let dummy_right = Unit::with_family("right".into(), right_dims, 1.0, 0.0, right_family);
            match dummy_left.mul(&dummy_right) {
                Ok(product) => Some(Infer::from_unit(&product)),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Div,
        ) => {
            let dummy_left = Unit::with_family("left".into(), left_dims, 1.0, 0.0, left_family);
            let dummy_right = Unit::with_family("right".into(), right_dims, 1.0, 0.0, right_family);
            match dummy_left.div(&dummy_right) {
                Ok(quotient) => Some(Infer::from_unit(&quotient)),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        _ => {
            admitter.error(
                "E-TYPE-012",
                "operator requires numeric operands",
                expr.source,
            );
            None
        }
    }
}

/// Map a surface type to a neutral type node (Phase 1 subset).
fn map_type(
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
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
    if host_types.contains(leaf) {
        return Some(TypeNode::Opaque {
            name: QualifiedName(leaf.to_string()),
            provider_contract: Some(SchemaId("emath.host.deferred".into())),
        });
    }
    match leaf {
        "Real" | "Float64" | "float64" | "f64" => Some(TypeNode::Float64),
        "Bool" => Some(TypeNode::Bool),
        "Self" => Some(TypeNode::Other(QualifiedName("Self".into()))),
        "NonNegative" | "Positive" | "Probability" => {
            let inner = generic_args
                .first()
                .and_then(|arg| map_type(arg, diagnostics, host_types));
            let base = inner.unwrap_or(TypeNode::Float64);
            Some(TypeNode::Refinement {
                base: Box::new(base),
                predicate: leaf.to_string(),
            })
        }
        "Per" => {
            if generic_args.len() != 1 {
                diagnostics.error(
                    "E-UNIT-105",
                    "`Per<U>` requires exactly one inner unit",
                    ty.source,
                );
                return None;
            }
            let inner_name = match &generic_args[0].kind {
                SynTypeKind::Path { segments, .. } => segments.last().map_or("", String::as_str),
                _ => {
                    diagnostics.error(
                        "E-UNIT-105",
                        "`Per<U>` inner argument must be a unit type",
                        generic_args[0].source,
                    );
                    return None;
                }
            };
            match per_unit(inner_name) {
                Ok(unit) => Some(TypeNode::UnitRef { name: unit.name }),
                Err(error) => {
                    diagnostics.error(error.code, error.message, ty.source);
                    None
                }
            }
        }
        "Interval" => {
            let inner = generic_args
                .first()
                .and_then(|arg| map_type(arg, diagnostics, host_types))
                .unwrap_or(TypeNode::Float64);
            Some(TypeNode::Interval(Box::new(inner)))
        }
        "Vector" | "Matrix" | "Tensor" => {
            map_shape_type(leaf, generic_args, ty, diagnostics, host_types)
        }
        "Result" => {
            let error_name = generic_args
                .get(1)
                .map_or_else(|| "ConfigError".to_string(), type_display);
            Some(TypeNode::Other(QualifiedName(error_name)))
        }
        "Option"
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
        | "Rational" => {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                format!("type `{leaf}` is outside the Phase 1 subset"),
                ty.source,
            );
            None
        }
        other => match lookup_unit(other) {
            Ok(unit) => Some(TypeNode::UnitRef { name: unit.name }),
            Err(error) if error.code == "E-UNIT-104" => {
                diagnostics.error("E-TYPE-001", format!("unknown type `{other}`"), ty.source);
                None
            }
            Err(error) => {
                diagnostics.error(error.code, error.message, ty.source);
                None
            }
        },
    }
}

fn map_shape_type(
    leaf: &str,
    generic_args: &[TypeExpr],
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
    let element = generic_args
        .first()
        .and_then(|arg| map_type(arg, diagnostics, host_types))
        .unwrap_or(TypeNode::Float64);
    let extent_args = generic_args.get(1..).unwrap_or(&[]);
    let mut extents = Vec::new();
    for arg in extent_args {
        match &arg.kind {
            SynTypeKind::List(items) if items.is_empty() => {
                diagnostics.error(
                    "E-SHAPE-004",
                    "declared tensor/vector shape must have rank >= 1",
                    arg.source,
                );
                return None;
            }
            SynTypeKind::List(items) => {
                for item in items {
                    extents.push(extent_from_type(item, diagnostics)?);
                }
            }
            SynTypeKind::Path { segments, .. } => {
                let name = segments.last().map_or("", String::as_str);
                extents.push(emath_ir::Extent::Symbolic(name.to_string()));
            }
            _ => {
                diagnostics.error(
                    "E-SHAPE-004",
                    format!("shape extent `{}` is not well-formed", type_display(arg)),
                    arg.source,
                );
                return None;
            }
        }
    }
    if leaf == "Tensor" && extents.is_empty() && extent_args.iter().any(|arg| {
        matches!(arg.kind, SynTypeKind::List(_))
    }) {
        return None;
    }
    if !extents.is_empty() {
        if let Err(error) = emath_ir::Shape::declare(extents.clone()) {
            diagnostics.error(error.code, error.message, ty.source);
            return None;
        }
    }
    match leaf {
        "Vector" => Some(TypeNode::Vector {
            element: Box::new(element),
            extent: extents.first().map(|extent| match extent {
                emath_ir::Extent::Fixed(size) => size.to_string(),
                emath_ir::Extent::Symbolic(name) => name.clone(),
            }),
        }),
        "Matrix" => Some(TypeNode::Matrix {
            element: Box::new(element),
            rows: extents.first().map(|extent| match extent {
                emath_ir::Extent::Fixed(size) => size.to_string(),
                emath_ir::Extent::Symbolic(name) => name.clone(),
            }),
            cols: extents.get(1).map(|extent| match extent {
                emath_ir::Extent::Fixed(size) => size.to_string(),
                emath_ir::Extent::Symbolic(name) => name.clone(),
            }),
        }),
        _ => Some(TypeNode::Tensor {
            element: Box::new(element),
            shape: extents
                .iter()
                .map(|extent| match extent {
                    emath_ir::Extent::Fixed(size) => size.to_string(),
                    emath_ir::Extent::Symbolic(name) => name.clone(),
                })
                .collect(),
        }),
    }
}

fn extent_from_type(
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
) -> Option<emath_ir::Extent> {
    match &ty.kind {
        SynTypeKind::Path { segments, .. } => {
            let name = segments.last().map_or("", String::as_str);
            Some(emath_ir::Extent::Symbolic(name.to_string()))
        }
        _ => {
            diagnostics.error(
                "E-SHAPE-004",
                format!("shape extent `{}` is not well-formed", type_display(ty)),
                ty.source,
            );
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
pub fn admit_declaration(
    decl: &emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
) -> AdmitResult {
    let mut admitter = Admitter::new();
    admitter.host_types = host_types.clone();
    let kind_label = decl.as_kind.clone();
    let is_policy = kind_label == "policy";
    let schema = if is_policy {
        KindSchema::core_policy()
    } else {
        KindSchema::core_function()
    };

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

    // Kind schema is the required/optional source of truth (`E-KIND-011`).
    for (name, section_schema) in schema.sections() {
        if section_schema.repeat == RepeatPolicy::ExactlyOne && !by_name.contains_key(name) {
            admitter.error(
                "E-KIND-011",
                format!("kind `{}` requires section `{name}`", schema.name()),
                decl.head_source,
            );
        }
    }

    // Phase 1 whitelist: a section outside the subset is a typed refusal,
    // never a silent drop (AGENTS.md rule 6). `request:` / `requests:`
    // are the pre-`goals:` spellings; refuse with a migration hint.
    for section in decl.sections() {
        if matches!(section.name.as_str(), "request" | "requests") {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}:` was renamed to `goals:`; use `goals:`",
                    section.name
                ),
                section.head_source,
            );
            continue;
        }
        if !PHASE1_SECTIONS.contains(&section.name.as_str()) {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}` is outside the Phase 1 subset (known: {})",
                    section.name,
                    PHASE1_SECTIONS.join(", ")
                ),
                section.head_source,
            );
        }
    }

    // Fields: inputs, outputs, state. Head-args lower into the same Field
    // IR as an `inputs:` section. `-> T` declares a single output named
    // after the declaration (the example `square = x * x` binds the
    // declaration name). Mixing the head spelling with the equivalent
    // section forks identity and is refused.
    let mut fields_infer: BTreeMap<String, Infer> = BTreeMap::new();
    let mut fields_by_section: BTreeMap<&str, Vec<Field>> = BTreeMap::new();
    let mut outputs_from_head = false;
    if let Some(signature) = &decl.signature {
        let stateful = by_name.contains_key("state") || by_name.contains_key("constructors");
        let refuse_head = kind_label != "function" || stateful;
        if refuse_head {
            admitter.error(
                "E-SYN-123",
                "declaration head arguments are only admitted on stateless `emath function` declarations (no `state:` or `constructors:`)",
                decl.head_source,
            );
        }
        if by_name.contains_key("inputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head arguments cannot be mixed with an `inputs:` section; use one spelling",
                decl.head_source,
            );
        }
        if signature.ret.is_some() && by_name.contains_key("outputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head `->` return type cannot be mixed with an `outputs:` section; use one spelling",
                decl.head_source,
            );
        }
        let mix_inputs = by_name.contains_key("inputs");
        let mix_outputs = signature.ret.is_some() && by_name.contains_key("outputs");
        if !refuse_head && !mix_inputs {
            for param in &signature.params {
                if param.by_ref {
                    admitter.error(
                        "E-SYN-101",
                        "by-ref declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                if param.default.is_some() {
                    admitter.error(
                        "E-SYN-101",
                        "default values on declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "inputs",
                    &param.name,
                    &param.ty,
                    param.source,
                    true,
                );
            }
        }
        if !refuse_head && !mix_outputs {
            if let Some(ret) = &signature.ret {
                outputs_from_head = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "outputs",
                    &decl.name,
                    ret,
                    ret.source,
                    false,
                );
            }
        }
    }

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
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    section_name,
                    name,
                    ty,
                    stmt.source,
                    section_name == "inputs",
                );
            }
        }
    }

    let inputs = fields_by_section.get("inputs").cloned().unwrap_or_default();
    let outputs_omitted = !by_name.contains_key("outputs") && !outputs_from_head;
    let mut outputs_raw = fields_by_section
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
            if definitions.contains_key(name) {
                admitter.error(
                    E_DUPLICATE_FIELD,
                    format!("duplicate definition `{name}`"),
                    target.source,
                );
                continue;
            }
            match admitter.lower_expr(value) {
                Some((id, Infer::F64 | Infer::Unit { .. } | Infer::HostDeferred)) => {
                    admitter.record(
                        "sema",
                        format!("definition `{name}` typed numeric"),
                        value.source,
                    );
                    definitions.insert(name.clone(), id);
                    // Later definitions may name earlier ones (`b = a * a`).
                    admitter.definitions.insert(name.clone(), id);
                }
                Some((_, Infer::Bool)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("definition `{name}` must be Float64 in Phase 1"),
                        value.source,
                    );
                }
                Some((_, Infer::Opaque)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("definition `{name}` must be numeric; opaque host values are not scalars"),
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
    if outputs_omitted && schema.default_for("outputs") == Some("definitions") {
        let ty = admitter.type_id(TypeNode::Float64);
        for name in definitions.keys() {
            outputs_raw.push(Field {
                name: name.clone(),
                ty,
                visibility: Visibility::Public,
                source: decl.source,
            });
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
                        || !matches!(visibility, Some(emath_core::tree::Visibility::Public))
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
                    let from_diff = name.strip_prefix("gradient_").is_some_and(|target| {
                        by_name.get("goals").is_some_and(|section| {
                            section.suite.statements.iter().any(|stmt| {
                                matches!(
                                    &stmt.kind,
                                    StmtKind::Section(goal)
                                        if goal.name == "differentiate"
                                            && goal.generic.as_deref() == Some(target)
                                )
                            })
                        })
                    });
                    if !definitions.contains_key(name)
                        && !outputs_raw.iter().any(|o| o.name == *name)
                        && !from_diff
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
                                format!(
                                    "`given` name `{name}` is not an input or constructor parameter"
                                ),
                                inner.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((id, Infer::F64 | Infer::Unit { .. } | Infer::HostDeferred)) => {
                                given.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be Float64"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be Float64; opaque host values are not scalars"),
                                    inner.source,
                                );
                            }
                            None => {}
                        }
                    }
                    StmtKind::Expect(expr) => match admitter.lower_expr(expr) {
                        Some((id, Infer::Bool)) => expect = Some(id),
                        Some((
                            _,
                            Infer::F64
                            | Infer::Unit { .. }
                            | Infer::HostDeferred
                            | Infer::Opaque,
                        )) => {
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

    let about = admit_about(&mut admitter, by_name.get("about").copied());
    let evidence = admit_evidence(&mut admitter, by_name.get("evidence").copied());
    let host = admit_host(&mut admitter, by_name.get("host").copied());

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
        about,
        evidence,
        host,
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

fn is_infer_marker(ty: &TypeExpr) -> bool {
    matches!(
        &ty.kind,
        SynTypeKind::Path {
            segments,
            generic_args,
        } if generic_args.is_empty() && segments.last().map(String::as_str) == Some("Infer")
    )
}

/// Admits one `name: Type` (or untyped `Infer`) field into the structural
/// maps used for `inputs` / `outputs` / `state`. Untyped names are allowed
/// only when `allow_infer` is set (bare `inputs:` fields and head-args);
/// they default to Float64 and emit `N-TYPE-001`.
fn admit_named_field(
    admitter: &mut Admitter,
    fields_infer: &mut BTreeMap<String, Infer>,
    fields_by_section: &mut BTreeMap<&str, Vec<Field>>,
    section_name: &'static str,
    name: &str,
    ty: &TypeExpr,
    span: Span,
    allow_infer: bool,
) -> bool {
    let (infer, ty_id) = if is_infer_marker(ty) {
        if !allow_infer {
            admitter.error(
                "E-SYN-101",
                format!("only `name: Type` declarations are allowed in `{section_name}`"),
                span,
            );
            return false;
        }
        let ty_id = admitter.type_id(TypeNode::Float64);
        admitter.note(
            "N-TYPE-001",
            format!("input `{name}` defaulted to Float64"),
            span,
        );
        admitter.record(
            "sema",
            format!("field `{name}` typed as Float64 (defaulted)"),
            span,
        );
        (Infer::F64, ty_id)
    } else {
        let Some(node) = map_type(ty, &mut admitter.diagnostics, &admitter.host_types) else {
            return false;
        };
        let infer = infer_from_node(&node);
        let ty_id = admitter.type_id(node);
        admitter.record(
            "sema",
            format!(
                "field `{name}` typed as {}",
                ty_display(admitter.types.get(ty_id.index()).unwrap())
            ),
            span,
        );
        (infer, ty_id)
    };
    if fields_infer.contains_key(name) {
        admitter.error(
            E_DUPLICATE_FIELD,
            format!("duplicate field `{name}` (declared in section `{section_name}`)"),
            span,
        );
        return false;
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
    true
}

impl Admitter {
    fn type_of(&self, id: TypeId) -> Infer {
        self.types
            .get(id.index())
            .map(infer_from_node)
            .unwrap_or(Infer::F64)
    }
}

fn admit_constructor(
    admitter: &mut Admitter,
    params: &[emath_core::tree::Param],
    ret: Option<&TypeExpr>,
    suite: Option<&emath_core::tree::Suite>,
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
        let Some(node) = map_type(&param.ty, &mut admitter.diagnostics, &admitter.host_types) else {
            continue;
        };
        let infer = infer_from_node(&node);
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
        if let Some(node) = map_type(ret, &mut admitter.diagnostics, &admitter.host_types) {
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
                            Some((id, Infer::F64 | Infer::Unit { .. } | Infer::HostDeferred)) => {
                                assignments.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("state field `{name}` must be Float64"),
                                    value.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("state field `{name}` must be numeric; opaque host values are not scalars"),
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
        numeric: NumericProfile::default_phase1(),
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
            Some(CommandArgument::Expr(expr)) => match &expr.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                ExprKind::Int(text) | ExprKind::Float(text) => Some(text.clone()),
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
            "numeric" => match parse_numeric_profile(&value_text) {
                Ok(profile) => {
                    spec.numeric = profile;
                    admitter.record(
                        "sema",
                        format!("numeric model `{}`", profile.as_str()),
                        stmt.source,
                    );
                }
                Err(error) => admitter.error(error.code, error.message, stmt.source),
            },
            "precision" => {
                let Some(bits) = command_u16(argument.as_ref(), &value_text) else {
                    admitter.error(
                        "E-NUM-002",
                        format!("precision demand `{value_text}` is not a bit count"),
                        stmt.source,
                    );
                    continue;
                };
                if let Err(error) = check_precision_demand(spec.numeric, bits) {
                    admitter.error(error.code, error.message, stmt.source);
                }
            }
            "error-limit" => {
                let Some(limit) = command_f64(argument.as_ref(), &value_text) else {
                    admitter.error(
                        "E-NUM-003",
                        format!("error-limit `{value_text}` is not a finite bound"),
                        stmt.source,
                    );
                    continue;
                };
                if let Err(error) = check_error_limit(spec.numeric, limit) {
                    admitter.error(error.code, error.message, stmt.source);
                }
            }
            "domain" => {
                if !admit_domain_directive(admitter, argument.as_ref(), stmt.source) {
                    continue;
                }
            }
            "representation" => {
                admit_representation(admitter, &mut spec, head, argument.as_ref(), stmt.source);
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
                if value_text != "parametric" {
                    admitter.error(
                        "E-CODEGEN-055",
                        format!(
                            "`unresolved {value_text}` is outside the Phase 1 subset (parametric)"
                        ),
                        stmt.source,
                    );
                } else {
                    admitter.record(
                        "sema",
                        "compile unresolved parametric: host types stay host-deferred",
                        stmt.source,
                    );
                }
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

fn command_u16(argument: Option<&CommandArgument>, fallback: &str) -> Option<u16> {
    command_f64(argument, fallback).and_then(|value| {
        if value.is_finite() && value >= 0.0 && value == value.trunc() && value <= f64::from(u16::MAX)
        {
            Some(value as u16)
        } else {
            None
        }
    })
}

fn command_f64(argument: Option<&CommandArgument>, fallback: &str) -> Option<f64> {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => parse_float_constant(text),
            ExprKind::Unary {
                op: SynUnOp::Neg,
                value,
            } => match &value.kind {
                ExprKind::Int(text) | ExprKind::Float(text) => {
                    parse_float_constant(text).map(|value| -value)
                }
                _ => None,
            },
            _ => parse_float_constant(fallback),
        },
        _ => parse_float_constant(fallback),
    }
}

fn admit_domain_directive(
    admitter: &mut Admitter,
    argument: Option<&CommandArgument>,
    span: Span,
) -> bool {
    let Some(CommandArgument::Expr(expr)) = argument else {
        admitter.error("E-DOM-002", "domain directive requires an interval `lo..hi`", span);
        return false;
    };
    let ExprKind::Range {
        start: Some(start),
        end: Some(end),
        ..
    } = &expr.kind
    else {
        admitter.error(
            "E-DOM-002",
            "domain directive requires a bounded interval `lo..hi`",
            span,
        );
        return false;
    };
    let (Some(low), Some(high)) = (expr_number(start), expr_number(end)) else {
        admitter.error(
            "E-DOM-002",
            "domain bounds must be numeric literals",
            span,
        );
        return false;
    };
    match emath_ir::Interval::checked(low, high) {
        Ok(_) => true,
        Err(error) => {
            admitter.error(error.code, error.message, span);
            false
        }
    }
}

fn expr_number(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => parse_float_constant(text),
        ExprKind::Unary {
            op: SynUnOp::Neg,
            value,
        } => expr_number(value).map(|value| -value),
        _ => None,
    }
}

fn admit_representation(
    admitter: &mut Admitter,
    spec: &mut CompileSpec,
    head: &[String],
    argument: Option<&CommandArgument>,
    span: Span,
) {
    let source_name = head.get(1).map(String::as_str).unwrap_or("");
    let model_name = head.get(2).cloned().or_else(|| match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => Some(segments.join(".")),
            ExprKind::Call { function, .. } => match &function.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    });
    if source_name == "Real" && model_name.is_none() {
        admitter.error(
            "E-NUM-004",
            "do not map `Real` to `f64` without profile evidence (`representation Real => Float64` or `numeric strict-f64`)",
            span,
        );
        return;
    }
    let Some(model_name) = model_name.or_else(|| {
        if source_name.is_empty() {
            None
        } else {
            Some(source_name.to_string())
        }
    }) else {
        admitter.error(
            "E-NUM-004",
            "`representation` requires a named numeric model (Float64 or Interval)",
            span,
        );
        return;
    };
    match parse_numeric_profile(&model_name) {
        Ok(profile) => {
            spec.numeric = profile;
            admitter.record(
                "sema",
                format!(
                    "representation evidence: {} → {}",
                    if source_name.is_empty() {
                        "declared"
                    } else {
                        source_name
                    },
                    profile.as_str()
                ),
                span,
            );
        }
        Err(error) => admitter.error(error.code, error.message, span),
    }
}

fn host_imported_types(imports: &[ImportEntry]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for import in imports {
        if import.path.first().map(String::as_str) != Some("host") {
            continue;
        }
        if let ImportSelection::Named(pairs) = &import.selection {
            for (name, alias) in pairs {
                names.insert(alias.clone().unwrap_or_else(|| name.clone()));
            }
        }
    }
    names
}

fn admit_about(admitter: &mut Admitter, section: Option<&Section>) -> Option<String> {
    let section = section?;
    let mut summary = None;
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("summary") => {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Str(text) = &expr.kind {
                        summary = Some(text.clone());
                        admitter.record("sema", "about summary retained", expr.source);
                        continue;
                    }
                }
                admitter.error(
                    "E-SYN-101",
                    "`about.summary` must be a string literal",
                    stmt.source,
                );
            }
            _ => {
                admitter.error(
                    "E-SYN-101",
                    "`about:` admits `summary: \"...\"` in Phase 1",
                    stmt.source,
                );
            }
        }
    }
    summary
}

fn admit_evidence(admitter: &mut Admitter, section: Option<&Section>) -> Vec<EvidenceClaim> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut claims = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(claim) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected `claim <name>:` blocks inside `evidence:`",
                stmt.source,
            );
            continue;
        };
        if claim.name != "claim" {
            admitter.error(
                "E-SYN-101",
                format!("unknown evidence block `{}`", claim.name),
                claim.head_source,
            );
            continue;
        }
        let id = claim.generic.clone().unwrap_or_default();
        if id.is_empty() {
            admitter.error(
                "E-SYN-101",
                "`claim` requires a name in `<...>`",
                claim.head_source,
            );
            continue;
        }
        let mut statement = String::new();
        let mut class = String::new();
        for inner in &claim.suite.statements {
            match &inner.kind {
                StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("statement") => {
                    statement = match argument {
                        Some(CommandArgument::Expr(expr)) => expr_text(expr),
                        _ if head.len() > 1 => head[1..].join(" "),
                        _ => String::new(),
                    };
                }
                StmtKind::Require(expr) => {
                    class = expr_text(expr);
                }
                StmtKind::Command { head, .. } if head.first().map(String::as_str) == Some("require") => {
                    class = head.get(1).cloned().unwrap_or_default();
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "evidence claims admit `statement ...` and `require ...`",
                        inner.source,
                    );
                }
            }
        }
        admitter.record(
            "sema",
            format!("evidence claim `{id}` recorded (verdict not-run)"),
            claim.head_source,
        );
        claims.push(EvidenceClaim {
            id,
            statement,
            class,
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: ClaimVerdict::NotRun,
            level: EvidenceLevel::E1,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    claims
}

fn admit_host(admitter: &mut Admitter, section: Option<&Section>) -> Vec<HostBinding> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut bindings = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(language) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected a language section (`rust:`) inside `host:`",
                stmt.source,
            );
            continue;
        };
        for inner in &language.suite.statements {
            let StmtKind::Section(implement) = &inner.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `implement Trait for Type:` inside `host:`",
                    inner.source,
                );
                continue;
            };
            if implement.name != "implement" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown host block `{}`", implement.name),
                    implement.head_source,
                );
                continue;
            }
            let generic = implement.generic.clone().unwrap_or_default();
            let (trait_path, target) = match generic.rsplit_once("::") {
                Some((trait_path, target)) => (trait_path.to_string(), target.to_string()),
                None => (generic, String::new()),
            };
            let mut methods = Vec::new();
            for method_stmt in &implement.suite.statements {
                let StmtKind::FnDecl {
                    name, params, ret, suite, ..
                } = &method_stmt.kind
                else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `method name(...)` inside `implement`",
                        method_stmt.source,
                    );
                    continue;
                };
                let mut body = Vec::new();
                if let Some(suite) = suite {
                    for body_stmt in &suite.statements {
                        body.push(stmt_text(body_stmt));
                    }
                }
                methods.push(HostMethod {
                    name: name.clone(),
                    params: params
                        .iter()
                        .map(|param| {
                            let ty = type_display(&param.ty);
                            let ty = if param.by_ref {
                                format!("&{ty}")
                            } else {
                                ty
                            };
                            (param.name.clone(), ty)
                        })
                        .collect(),
                    ret: ret.as_ref().map(type_display),
                    body,
                });
            }
            admitter.record(
                "sema",
                format!(
                    "host binding `{}/{}` retained (trait impl codegen is a Phase 1 no-claim)",
                    language.name, trait_path
                ),
                implement.head_source,
            );
            bindings.push(HostBinding {
                language: language.name.clone(),
                trait_path,
                target,
                methods,
            });
        }
    }
    bindings
}

fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.join("."),
        ExprKind::Call { function, args } => {
            format!(
                "{}({})",
                expr_text(function),
                args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Str(text) => format!("\"{text}\""),
        ExprKind::Int(text) | ExprKind::Float(text) => text.clone(),
        ExprKind::Bool(value) => value.to_string(),
        _ => "expr".to_string(),
    }
}

fn stmt_text(stmt: &emath_core::tree::Stmt) -> String {
    match &stmt.kind {
        StmtKind::Command { head, argument } => {
            let mut text = head.join(" ");
            if let Some(argument) = argument {
                text.push(' ');
                text.push_str(&command_argument_text(argument));
            }
            text
        }
        _ => "stmt".to_string(),
    }
}

fn command_argument_text(argument: &CommandArgument) -> String {
    match argument {
        CommandArgument::Expr(expr) => expr_text(expr),
        CommandArgument::Assignment { name, value } => {
            format!("{name} = {}", expr_text(value))
        }
        CommandArgument::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    // Front-end: package identity and `use` imports. External file
    // imports remain a Phase 2 refusal (E-PKG-050).
    let has_recognition_items = tree.items.iter().any(|item| match item {
        emath_core::tree::Item::Package { .. } | emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => decl.item_kind != "custom",
    });
    let recognition = if has_recognition_items {
        let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
        package.package_path = front_end.package_path;
        package.imports = front_end.imports;
        Some(crate::recognition::collect_kind_defs(tree))
    } else {
        None
    };
    let host_types = host_imported_types(&package.imports);

    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
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
        // Duplicate declaration names are a typed refusal (E-NAME-022):
        // two `custom <Foo>` declarations would collide in generated
        // Rust, so the second is never admitted.
        if !seen_declaration_names.insert(decl.name.clone()) {
            diagnostics.error(
                "E-NAME-022",
                format!("duplicate declaration name `{}`", decl.name),
                decl.head_source,
            );
            continue;
        }
        // `_` is not a valid Rust type name and cannot be escaped; a
        // declaration named `_` is refused up front (E-NAME-023).
        if decl.name == "_" {
            diagnostics.error(
                "E-NAME-023",
                "declaration name `_` is reserved and cannot be a Rust type",
                decl.head_source,
            );
            continue;
        }
        // Confusable identity (spec `01_LEXICAL_LAYOUT_AND_SOURCE`): a
        // declaration name that differs from an already-seen one only by
        // lookalike glyphs (Latin `o` vs Cyrillic `о`) is refused
        // (E-NAME-024) — the public API would present two visually
        // indistinguishable names.
        let folded = confusable_fold(&decl.name);
        if let Some(existing) = seen_folded_declaration_names.get(&folded) {
            diagnostics.error(
                "E-NAME-024",
                format!(
                    "declaration name `{}` is confusable with `{existing}` and is refused",
                    decl.name
                ),
                decl.head_source,
            );
            continue;
        }
        seen_folded_declaration_names.insert(folded, decl.name.clone());
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
            admit_declaration(decl, &host_types);
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
