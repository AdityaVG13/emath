//! The Phase 1 admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::tree::{
    BinderKind, BinaryOp as SynBinOp, Expr, ExprKind,
    UnaryOp as SynUnOp,
};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::{
    BinaryOp, ExprId, ExprNode, Extent,
    Literal, ModelResidual, TypeId, TypeNode,
    lookup_unit,
};
use std::collections::{BTreeMap, BTreeSet};

mod declaration;
use declaration::admit_declaration;
mod expr_helpers;
use expr_helpers::*;
mod infer;
use infer::*;
mod types;
mod equations;
use equations::*;
mod sections;
use sections::*;
pub use sections::check_tree;

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
    "algebraic",
    "definitions",
    "equations",
    "equation",
    "constructors",
    "goals",
    "exports",
    "tests",
    "compile",
    "about",
    "evidence",
    "host",
    "constraints",
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

struct Admitter {
    diagnostics: Diagnostics,
    trace: Vec<TraceEntry>,
    params: BTreeMap<String, Infer>,
    inputs: BTreeMap<String, Infer>,
    states: BTreeMap<String, Infer>,
    definitions: BTreeMap<String, (ExprId, Infer)>,
    /// Constraint expression IDs from `constraints:` section, for penalty method.
    constraints: Vec<ExprId>,
    /// Causalized implicit residuals admitted from `equations:` (unknowns
    /// solved by Newton at each time step; see `crate::ModelResidual`).
    residuals: Vec<ModelResidual>,
    /// Synthetic inputs `__rate_<state>` that replace `der(state)` inside
    /// residual expressions; inferred like the state field they derive.
    rate_placeholders: BTreeMap<String, Infer>,
    exprs: Vec<(ExprNode, Span)>,
    types: Vec<TypeNode>,
    host_types: BTreeSet<String>,
    /// Finite binder locals (`sum i in 0..n`). Looked up before inputs.
    index_locals: BTreeMap<String, i64>,
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
            constraints: Vec::new(),
            residuals: Vec::new(),
            rate_placeholders: BTreeMap::new(),
            exprs: Vec::new(),
            types: Vec::new(),
            host_types: BTreeSet::new(),
            index_locals: BTreeMap::new(),
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
        self.diagnostics.note(code, message, span);
    }

    fn lookup(&self, name: &str) -> Option<Infer> {
        if let Some(value) = self.index_locals.get(name) {
            return Some(if *value < 0 { Infer::Int } else { Infer::Nat });
        }
        if let Some(infer) = self.rate_placeholders.get(name) {
            return Some(infer.clone());
        }
        if let Some(infer) = self.params.get(name) {
            return Some(infer.clone());
        }
        if let Some(infer) = self.inputs.get(name) {
            return Some(infer.clone());
        }
        if let Some(stripped) = name.strip_prefix("state.") {
            return self.states.get(stripped).cloned();
        }
        if let Some((_, infer)) = self.definitions.get(name) {
            return Some(infer.clone());
        }
        self.states.get(name).cloned()
    }

    /// Recursively inline definition references in a SIR expression tree.
    /// Replaces `Variable(name)` nodes that reference definitions with
    /// the definition's own SIR expression, so that downstream consumers
    /// (e.g. forward-mode autodiff) see the actual computation rather
    /// than a constant reference.
    fn inline_defs(&mut self, expr_id: ExprId) -> ExprId {
        let Some((node, span)) = self.exprs.get(expr_id.0 as usize).cloned() else {
            return expr_id;
        };
        match node {
            ExprNode::Variable(name) => {
                if let Some((def_id, _)) = self.definitions.get(&name.0) {
                    self.inline_defs(*def_id)
                } else {
                    expr_id
                }
            }
            ExprNode::Literal(_) => expr_id,
            ExprNode::Unary { operation, value } => {
                let value = self.inline_defs(value);
                self.push_expr(ExprNode::Unary { operation, value }, span)
            }
            ExprNode::Binary { operation, left, right } => {
                let left = self.inline_defs(left);
                let right = self.inline_defs(right);
                self.push_expr(ExprNode::Binary { operation, left, right }, span)
            }
            ExprNode::Call { function, arguments } => {
                let arguments: Vec<_> =
                    arguments.into_iter().map(|a| self.inline_defs(a)).collect();
                self.push_expr(ExprNode::Call { function, arguments }, span)
            }
            ExprNode::If { condition, then_value, else_value } => {
                let condition = self.inline_defs(condition);
                let then_value = self.inline_defs(then_value);
                let else_value = self.inline_defs(else_value);
                self.push_expr(
                    ExprNode::If { condition, then_value, else_value },
                    span,
                )
            }
            ExprNode::Index { value, indices } => {
                let value = self.inline_defs(value);
                let indices: Vec<_> =
                    indices.into_iter().map(|i| self.inline_defs(i)).collect();
                self.push_expr(ExprNode::Index { value, indices }, span)
            }
            ExprNode::Vector(els) => {
                let els: Vec<_> = els.into_iter().map(|e| self.inline_defs(e)).collect();
                self.push_expr(ExprNode::Vector(els), span)
            }
            ExprNode::Matrix(rows) => {
                let rows: Vec<Vec<_>> = rows
                    .into_iter()
                    .map(|row| row.into_iter().map(|e| self.inline_defs(e)).collect())
                    .collect();
                self.push_expr(ExprNode::Matrix(rows), span)
            }
            ExprNode::Tensor { shape, elements } => {
                let elements: Vec<_> =
                    elements.into_iter().map(|e| self.inline_defs(e)).collect();
                self.push_expr(ExprNode::Tensor { shape, elements }, span)
            }
            ExprNode::Differentiate { body, var } => {
                let body = self.inline_defs(body);
                self.push_expr(ExprNode::Differentiate { body, var }, span)
            }
            // Slice, Record, Binder — keep as-is (rare in derivative bodies).
            _ => expr_id,
        }
    }

    /// Add penalty terms for each constraint to the optimization body.
    /// For inequality `a >= b`: penalty = `max(0, b - a)^2`.
    /// For inequality `a <= b`: penalty = `max(0, a - b)^2`.
    /// For equality `a == b`:  penalty = `(a - b)^2`.
    /// The penalized body is `body + weight * sum(penalties)`.
    fn add_constraint_penalties(&mut self, body: ExprId, span: Span) -> ExprId {
        if self.constraints.is_empty() {
            return body;
        }
        // Must stay stable with the optimizer's fixed learning_rate
        // (0.01): the penalty Hessian adds eigenvalue 4*w, so stability
        // needs lr < 2/(2+4w). At w=20, 2/L = 0.024 > 0.01 (stable) and
        // the equilibrium w/(1+2w) = 0.488 is within typical tolerance.
        // w=1000 (the prior value) gave L≈4002, needing lr<0.0005, so the
        // fixed lr=0.01 overshot and never converged.
        const PENALTY_WEIGHT: f64 = 20.0;
        let weight_id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(PENALTY_WEIGHT.to_bits())),
            span,
        );
        let mut result = body;
        for &constraint_id in &self.constraints.clone() {
            let Some(penalty) = self.constraint_penalty(constraint_id, span) else {
                continue;
            };
            let weighted = self.push_expr(
                ExprNode::Binary {
                    operation: BinaryOp::StrictFloatMul,
                    left: weight_id,
                    right: penalty,
                },
                span,
            );
            result = self.push_expr(
                ExprNode::Binary {
                    operation: BinaryOp::StrictFloatAdd,
                    left: result,
                    right: weighted,
                },
                span,
            );
        }
        if result != body {
            self.record(
                "sema",
                format!("added {} constraint penalty term(s) to optimization body", self.constraints.len()),
                span,
            );
        }
        result
    }

    /// Build a penalty expression for a single constraint.
    /// Returns None for non-comparison constraints (e.g. NotEqual).
    fn constraint_penalty(&mut self, constraint_id: ExprId, span: Span) -> Option<ExprId> {
        let (node, _) = self.exprs.get(constraint_id.0 as usize)?.clone();
        let ExprNode::Binary { operation, left, right } = node else {
            return None;
        };
        let zero = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(0.0f64.to_bits())),
            span,
        );
        let two = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(2.0f64.to_bits())),
            span,
        );
        // violation = amount by which constraint is violated (>= 0 when violated)
        let violation = match operation {
            BinaryOp::GreaterEqual | BinaryOp::Greater => {
                // a >= b violated when a < b → violation = b - a
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left: right,
                        right: left,
                    },
                    span,
                )
            }
            BinaryOp::LessEqual | BinaryOp::Less => {
                // a <= b violated when a > b → violation = a - b
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left,
                        right,
                    },
                    span,
                )
            }
            BinaryOp::Equal => {
                // a == b → violation = a - b (squared, so sign doesn't matter)
                self.push_expr(
                    ExprNode::Binary {
                        operation: BinaryOp::StrictFloatSub,
                        left,
                        right,
                    },
                    span,
                )
            }
            _ => return None,
        };
        // For inequalities: clamp violation at 0 with max(0, violation)
        let clamped = if matches!(operation, BinaryOp::Equal) {
            violation
        } else {
            self.push_expr(
                ExprNode::Call {
                    function: QualifiedName("max".to_string()),
                    arguments: vec![zero, violation],
                },
                span,
            )
        };
        // penalty = clamped^2
        Some(self.push_expr(
            ExprNode::Call {
                function: QualifiedName("pow".to_string()),
                arguments: vec![clamped, two],
            },
            span,
        ))
    }

    fn lower_expr(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        match &expr.kind {
            ExprKind::Int(text) => {
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Integer(text.clone())),
                    expr.source,
                );
                let infer = if text.starts_with('-') {
                    Infer::Int
                } else {
                    Infer::Nat
                };
                Some((id, infer))
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
                if segments.len() == 1 {
                    if let Some(value) = self.index_locals.get(&name).copied() {
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::Integer(value.to_string())),
                            expr.source,
                        );
                        let infer = if value < 0 { Infer::Int } else { Infer::Nat };
                        return Some((id, infer));
                    }
                }
                if let Some(infer) = self.lookup(&name) {
                    let ir_name = state_variable_name(self, segments, &name);
                    let id =
                        self.push_expr(ExprNode::Variable(QualifiedName(ir_name)), expr.source);
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
                if matches!(name.as_str(), "sum" | "product") {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 1 argument, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    return self.lower_reduction(expr, &name, &args[0]);
                }
                let arity: Option<usize> = match name.as_str() {
                    "is_finite" | "exp" | "ln" | "log" | "sqrt" | "sin" | "cos" | "tan"
                    | "tanh" | "abs" | "floor" | "ceil" | "round" | "sign" | "log2" | "log10" | "sinh" | "cosh" | "atan" | "cbrt" | "recip" | "fract"
                    | "norm" | "transpose" | "length" | "len" | "mean" => Some(1),
                    "min" | "max" | "atan2" | "pow" | "mod" | "hypot" | "dot" | "laplacian" | "laplacian_neumann" | "laplacian_2d" | "laplacian_2d_neumann" | "gradient" | "gradient_2d_x" | "gradient_2d_y" => Some(2),
                    "lerp" | "clamp" => Some(3),
                    "laplacian_dirichlet" => Some(4),
                    _ => {
                        self.error(
                            E_UNKNOWN_FUNCTION,
                            format!(
                                "unknown function `{name}` (Phase 1 builtins: exp, ln, log, sqrt, sin, cos, tan, tanh, abs, floor, ceil, round, sign, log2, log10, sinh, cosh, atan, cbrt, recip, fract, min, max, atan2, pow, mod, hypot, lerp, clamp, is_finite, norm, transpose, dot, length, sum, product, mean, laplacian, laplacian_neumann, laplacian_dirichlet, laplacian_2d, laplacian_2d_neumann, gradient, gradient_2d_x, gradient_2d_y)"
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
                match name.as_str() {
                    "norm" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`norm` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "laplacian" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_neumann" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_neumann` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_neumann` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_dirichlet" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_dirichlet` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let (g_left_id, g_left_infer) = self.lower_expr(&args[2])?;
                        if !matches!(g_left_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 left boundary value as the third argument",
                                args[2].source,
                            );
                            return None;
                        }
                        let (g_right_id, g_right_infer) = self.lower_expr(&args[3])?;
                        if !matches!(g_right_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 right boundary value as the fourth argument",
                                args[3].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id, g_left_id, g_right_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_2d" | "laplacian_2d_neumann" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "gradient" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Vector first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "gradient_2d_x" | "gradient_2d_y" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "transpose" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::Matrix { rows, cols } => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::Matrix { rows: cols, cols: rows }))
                            }
                            Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::Matrix { rows: None, cols: None }))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`transpose` expects a Matrix argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    "length" | "len" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`length` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "dot" => {
                        let (l_id, l_infer) = self.lower_expr(&args[0])?;
                        let (r_id, r_infer) = self.lower_expr(&args[1])?;
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: e1 }, Infer::Vector { extent: e2 }) => {
                                if let (Some(ext1), Some(ext2)) = (e1, e2) {
                                    if ext1 != ext2 {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in dot product: {ext1:?} vs {ext2:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            (Infer::HostDeferred, _) | (_, Infer::HostDeferred) => {
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`dot` expects two Vector arguments",
                                    expr.source,
                                );
                                None
                            }
                        }
                    }
                    "mean" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`mean` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        // mean = sum(arg) / length(arg), reusing the known-shape fold and len.
                        let (sum_id, _) = self.lower_reduction(expr, "sum", &args[0])?;
                        let length_id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName("length".to_string()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Binary {
                                operation: emath_ir::BinaryOp::StrictFloatDiv,
                                left: sum_id,
                                right: length_id,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "abs" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::F64 | Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName("abs".to_string()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            Infer::Vector { extent: Some(Extent::Fixed(n)) } => {
                                let mut elems = Vec::with_capacity(n);
                                for i in 0..n {
                                    let idx = self.push_expr(
                                        ExprNode::Literal(Literal::Integer(i.to_string())),
                                        expr.source,
                                    );
                                    let term = self.push_expr(
                                        ExprNode::Index {
                                            value: arg_id,
                                            indices: vec![idx],
                                        },
                                        expr.source,
                                    );
                                    let abs_term = self.push_expr(
                                        ExprNode::Call {
                                            function: QualifiedName("abs".to_string()),
                                            arguments: vec![term],
                                        },
                                        expr.source,
                                    );
                                    elems.push(abs_term);
                                }
                                let id = self.push_expr(ExprNode::Vector(elems), expr.source);
                                Some((
                                    id,
                                    Infer::Vector { extent: Some(Extent::Fixed(n)) },
                                ))
                            }
                            Infer::Vector { extent: None } => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` on a vector needs a known size",
                                    args[0].source,
                                );
                                None
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` expects a scalar or vector argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    _ => {
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
                }
            }
            ExprKind::Unary { op, value } => {
                let (id, infer) = self.lower_expr(value)?;
                match (op, &infer) {
                    (SynUnOp::Neg, Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred) => {
                        self.record("sema", "negate → strict negate", expr.source);
                        let result = if matches!(infer, Infer::Nat) {
                            Infer::Int
                        } else {
                            infer
                        };
                        Some((
                            self.push_expr(
                                ExprNode::Unary {
                                    operation: emath_ir::UnaryOp::Negate,
                                    value: id,
                                },
                                expr.source,
                            ),
                            result,
                        ))
                    }
                    (SynUnOp::Pos, Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred) => {
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
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                                if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                    if l_e != r_e {
                                        self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector addition: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                                self.record("sema", "vector add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorAdd, expr, l, r, Infer::Vector { extent: res_extent })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                    if r1_e != r2_e {
                                        self.error("E-SHAPE-005", "matrix row dimension mismatch in addition", expr.source);
                                        return None;
                                    }
                                }
                                if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                    if c1_e != c2_e {
                                        self.error("E-SHAPE-005", "matrix col dimension mismatch in addition", expr.source);
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixAdd, expr, l, r, Infer::Matrix { rows: r1.clone().or_else(|| r2.clone()), cols: c1.clone().or_else(|| c2.clone()) })
                            }
                            (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                                let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                                self.record("sema", "tensor add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::TensorAdd, expr, l, r, Infer::Tensor { shape })
                            }
                            _ => {
                                self.record("sema", "add → strict f64 add", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Add, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Sub => {
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                                if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                    if l_e != r_e {
                                        self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector subtraction: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                                self.record("sema", "vector subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorSub, expr, l, r, Infer::Vector { extent: res_extent })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                    if r1_e != r2_e {
                                        self.error("E-SHAPE-005", "matrix row dimension mismatch in subtraction", expr.source);
                                        return None;
                                    }
                                }
                                if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                    if c1_e != c2_e {
                                        self.error("E-SHAPE-005", "matrix col dimension mismatch in subtraction", expr.source);
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixSub, expr, l, r, Infer::Matrix { rows: r1.clone().or_else(|| r2.clone()), cols: c1.clone().or_else(|| c2.clone()) })
                            }
                            (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                                let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                                self.record("sema", "tensor subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::TensorSub, expr, l, r, Infer::Tensor { shape })
                            }
                            _ => {
                                self.record("sema", "subtract → strict f64 subtract", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Add, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Mul => {
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent }, Infer::F64 | Infer::HostDeferred) => {
                                self.record("sema", "vector scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorScale, expr, l, r, Infer::Vector { extent: extent.clone() })
                            }
                            (Infer::F64 | Infer::HostDeferred, Infer::Vector { extent }) => {
                                self.record("sema", "vector scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorScale, expr, r, l, Infer::Vector { extent: extent.clone() })
                            }
                            (Infer::Matrix { rows, cols }, Infer::F64 | Infer::HostDeferred) => {
                                self.record("sema", "matrix scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixScale, expr, l, r, Infer::Matrix { rows: rows.clone(), cols: cols.clone() })
                            }
                            (Infer::F64 | Infer::HostDeferred, Infer::Matrix { rows, cols }) => {
                                self.record("sema", "matrix scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixScale, expr, r, l, Infer::Matrix { rows: rows.clone(), cols: cols.clone() })
                            }
                            (Infer::Matrix { rows, cols }, Infer::Vector { extent }) => {
                                if let (Some(c_e), Some(v_e)) = (cols, extent) {
                                    if c_e != v_e {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix-vector multiplication: matrix columns {c_e:?} != vector length {v_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix mul vector", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixMulVector, expr, l, r, Infer::Vector { extent: rows.clone() })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(c1_e), Some(r2_e)) = (c1, r2) {
                                    if c1_e != r2_e {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix multiplication: left columns {c1_e:?} != right rows {r2_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix mul matrix", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixMulMatrix, expr, l, r, Infer::Matrix { rows: r1.clone(), cols: c2.clone() })
                            }
                            _ => {
                                self.record("sema", "multiply → strict f64 multiply", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Mul, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Div => {
                        self.record("sema", "divide → strict f64 divide", expr.source);
                        let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Div, expr, self)?;
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
                        ) && !comparable_numeric(&l_infer, &r_infer)
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
            ExprKind::List(items) => self.lower_list_literal(expr, items),
            ExprKind::Index { value, indices } => self.lower_index(expr, value, indices),
            ExprKind::Binder {
                kind,
                binders,
                body,
            } => self.lower_finite_binder(expr, *kind, binders, body),
            ExprKind::Derivative { .. } => {
                // The parser may produce nested Derivative nodes:
                // `derivative x wrt y` becomes Derivative(Derivative(x)) wrt y.
                // Unwrap to get the inner value and the wrt clause.
                let Some((value, wrt)) = unwrap_derivative(expr) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative could not be unwrapped",
                        expr.source,
                    );
                    return None;
                };
                let Some(vars) = wrt else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative requires `wrt` clause: derivative(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("derivative variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                // Lower the value expression, then inline definition
                // references so the EMIR dual-number evaluator sees the
                // full computation chain.
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "derivative body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Differentiate { body: inlined, var: var_name.clone() },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("derivative wrt {var_name} → forward-mode autodiff"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Solve { value, wrt } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve requires `wrt` clause: solve(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("solve variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "solve body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Solve { body: inlined, var: var_name.clone() },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("solve wrt {var_name} → Newton's method root-finding"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Optimize { value, wrt, maximize } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires `wrt` clause: minimize(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.is_empty() {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires at least one `wrt` variable",
                        expr.source,
                    );
                    return None;
                }
                let mut var_names = Vec::with_capacity(vars.len());
                for var in vars {
                    let Some(segments) = path_segments(var) else {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            "optimization variable must be a plain name",
                            var.source,
                        );
                        return None;
                    };
                    let name = segments[0].clone();
                    if !self.inputs.contains_key(&name) {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            format!("optimization variable `{name}` must be an input"),
                            var.source,
                        );
                        return None;
                    }
                    var_names.push(name);
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "optimization body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let body_with_penalty = self.add_constraint_penalties(inlined, expr.source);
                let id = self.push_expr(
                    ExprNode::Optimize { body: body_with_penalty, vars: var_names.clone(), maximize: *maximize },
                    expr.source,
                );
                let direction = if *maximize { "maximize" } else { "minimize" };
                self.record(
                    "sema",
                    format!("{direction} wrt {} → gradient-descent optimization", var_names.join(", ")),
                    expr.source,
                );
                Some((id, Infer::F64))
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

    fn lower_finite_binder(
        &mut self,
        expr: &Expr,
        kind: BinderKind,
        binders: &[emath_core::tree::Binder],
        body: &Expr,
    ) -> Option<(ExprId, Infer)> {
        if binders.len() != 1 {
            self.error(
                E_UNSUPPORTED_TYPE,
                "only a single binder variable is computed today",
                expr.source,
            );
            return None;
        }
        let binder = &binders[0];
        let Some(domain) = binder.domain.as_ref() else {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("`{kind:?}` needs a finite integer range `name in lo..hi`"),
                binder.source,
            );
            return None;
        };
        let Some((start, end)) = integer_range(domain) else {
            // Variable-bound range: lower as a runtime fold.
            return self.lower_variable_bound_binder(expr, kind, binder, domain, body);
        };
        if end < start {
            self.error(
                "E-DOM-002",
                format!("binder range `{start}..{end}` is inverted"),
                domain.source,
            );
            return None;
        }
        if end - start > 10_000 {
            self.error(
                E_UNSUPPORTED_TYPE,
                "finite binder range is capped at 10000 terms",
                domain.source,
            );
            return None;
        }
        // For bool folds (forall/exists), use the runtime Fold op for
        // correct bool handling in both interp and codegen.
        if matches!(kind, BinderKind::ForAll | BinderKind::Exists | BinderKind::Integral) {
            return self.lower_variable_bound_binder(expr, kind, binder, domain, body);
        }
        let (combine, identity) = match kind {
            BinderKind::Sum => (emath_ir::BinaryOp::StrictFloatAdd, 0.0_f64),
            BinderKind::Product => (emath_ir::BinaryOp::StrictFloatMul, 1.0_f64),
            BinderKind::Integral | BinderKind::ForAll | BinderKind::Exists => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!("`{kind:?}` is not a finite arithmetic fold yet"),
                    expr.source,
                );
                return None;
            }
        };
        let previous = self.index_locals.insert(binder.name.clone(), start);
        let mut acc_id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(identity.to_bits())),
            expr.source,
        );
        let mut acc_infer = Infer::F64;
        for value in start..end {
            self.index_locals.insert(binder.name.clone(), value);
            let (term_id, term_infer) = match self.lower_expr(body) {
                Some(term) => term,
                None => {
                    restore_index_local(&mut self.index_locals, &binder.name, previous);
                    return None;
                }
            };
            if !is_numeric_element(&term_infer) {
                self.error(
                    "E-TYPE-012",
                    format!("`{kind:?}` body must be numeric"),
                    body.source,
                );
                restore_index_local(&mut self.index_locals, &binder.name, previous);
                return None;
            }
            acc_infer = match combine_numeric(
                &acc_infer,
                &term_infer,
                match kind {
                    BinderKind::Sum => NumericCombine::Add,
                    BinderKind::Product => NumericCombine::Mul,
                    BinderKind::Integral | BinderKind::ForAll | BinderKind::Exists => {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            format!("`{kind:?}` is not a finite arithmetic fold yet"),
                            expr.source,
                        );
                        restore_index_local(&mut self.index_locals, &binder.name, previous);
                        return None;
                    }
                },
                expr,
                self,
            ) {
                Some(infer) => infer,
                None => {
                    restore_index_local(&mut self.index_locals, &binder.name, previous);
                    return None;
                }
            };
            acc_id = self.push_expr(
                ExprNode::Binary {
                    operation: combine,
                    left: acc_id,
                    right: term_id,
                },
                expr.source,
            );
        }
        restore_index_local(&mut self.index_locals, &binder.name, previous);
        self.record(
            "sema",
            format!(
                "{kind:?} `{name}` in {start}..{end} → {count} terms",
                name = binder.name,
                count = end - start
            ),
            expr.source,
        );
        Some((acc_id, acc_infer))
    }

    fn lower_variable_bound_binder(
        &mut self,
        expr: &Expr,
        kind: BinderKind,
        binder: &emath_core::tree::Binder,
        domain: &Expr,
        body: &Expr,
    ) -> Option<(ExprId, Infer)> {
        let sir_kind = match kind {
            BinderKind::Sum => emath_ir::BinderKind::Sum,
            BinderKind::Product => emath_ir::BinderKind::Product,
            BinderKind::ForAll => emath_ir::BinderKind::ForAll,
            BinderKind::Exists => emath_ir::BinderKind::Exists,
            BinderKind::Integral => emath_ir::BinderKind::Integral,
        };
        let ExprKind::Range { start, end, inclusive } = &domain.kind else {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("{kind:?} range must be a known integer interval such as `0..n`"),
                domain.source,
            );
            return None;
        };
        let Some(start_expr) = start.as_ref() else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "range needs a start bound",
                domain.source,
            );
            return None;
        };
        let Some(end_expr) = end.as_ref() else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "range needs an end bound",
                domain.source,
            );
            return None;
        };
        let (start_id, _) = self.lower_expr(start_expr)?;
        let (end_id, _) = self.lower_expr(end_expr)?;
        // For inclusive range (`..`=`), the end becomes end+1.
        let end_id = if *inclusive {
            let one = self.push_expr(
                ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())),
                domain.source,
            );
            self.push_expr(
                ExprNode::Binary {
                    operation: emath_ir::BinaryOp::StrictFloatAdd,
                    left: end_id,
                    right: one,
                },
                domain.source,
            )
        } else {
            end_id
        };
        // Encode domain as Vector([start, end]) for the EMIR emitter.
        let domain_id =
            self.push_expr(ExprNode::Vector(vec![start_id, end_id]), domain.source);
        // Temporarily add the loop variable to inputs so the body can
        // reference it as a Variable (resolved to LoadInput by the EMIR
        // emitter's Binder handler).
        let prev = self.inputs.insert(binder.name.clone(), Infer::Nat);
        let (body_id, body_infer) = match self.lower_expr(body) {
            Some(result) => result,
            None => {
                restore_input(&mut self.inputs, &binder.name, prev);
                return None;
            }
        };
        restore_input(&mut self.inputs, &binder.name, prev);
        let is_bool_fold = matches!(kind, BinderKind::ForAll | BinderKind::Exists);
        if is_bool_fold {
            if !matches!(body_infer, Infer::Bool) {
                self.error(
                    "E-TYPE-012",
                    format!("{kind:?} body must be boolean"),
                    body.source,
                );
                return None;
            }
        } else if !is_numeric_element(&body_infer) {
            self.error(
                "E-TYPE-012",
                format!("{kind:?} body must be numeric"),
                body.source,
            );
            return None;
        }
        let binder_id = self.push_expr(
            ExprNode::Binder {
                kind: sir_kind,
                variables: vec![emath_ir::BinderVariable {
                    name: binder.name.clone(),
                    domain: domain_id,
                }],
                body: body_id,
            },
            expr.source,
        );
        self.record(
            "sema",
            format!(
                "{kind:?} `{name}` in <runtime range> → fold",
                name = binder.name
            ),
            expr.source,
        );
        let return_infer = if is_bool_fold { Infer::Bool } else { Infer::F64 };
        Some((binder_id, return_infer))
    }

    fn lower_reduction(
        &mut self,
        expr: &Expr,
        name: &str,
        arg: &Expr,
    ) -> Option<(ExprId, Infer)> {
        let (arg_id, arg_infer) = self.lower_expr(arg)?;
        let (combine, identity): (emath_ir::BinaryOp, f64) = match name {
            "sum" => (emath_ir::BinaryOp::StrictFloatAdd, 0.0),
            "product" => (emath_ir::BinaryOp::StrictFloatMul, 1.0),
            _ => {
                self.error(
                    E_UNKNOWN_FUNCTION,
                    format!("`{name}` is not a finite reduction"),
                    expr.source,
                );
                return None;
            }
        };
        let Some(coords) = reduction_coords(&arg_infer) else {
            if is_numeric_element(&arg_infer) {
                return Some((arg_id, Infer::F64));
            }
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("`{name}` needs a vector, matrix, or tensor with a known size"),
                arg.source,
            );
            return None;
        };
        if coords.len() > 10_000 {
            self.error(
                E_UNSUPPORTED_TYPE,
                "finite reduction is capped at 10000 terms",
                arg.source,
            );
            return None;
        }
        let mut acc_id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(identity.to_bits())),
            expr.source,
        );
        for coord in &coords {
            let indices = coord
                .iter()
                .map(|axis| {
                    self.push_expr(
                        ExprNode::Literal(Literal::Integer(axis.to_string())),
                        expr.source,
                    )
                })
                .collect();
            let term_id = self.push_expr(
                ExprNode::Index {
                    value: arg_id,
                    indices,
                },
                expr.source,
            );
            acc_id = self.push_expr(
                ExprNode::Binary {
                    operation: combine,
                    left: acc_id,
                    right: term_id,
                },
                expr.source,
            );
        }
        self.record(
            "sema",
            format!("`{name}` → {count} terms", count = coords.len()),
            expr.source,
        );
        Some((acc_id, Infer::F64))
    }

    fn lower_list_literal(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        if items.is_empty() {
            self.error(
                "E-SHAPE-004",
                "empty vector literal is not allowed",
                expr.source,
            );
            return None;
        }
        if items.iter().all(|item| matches!(&item.kind, ExprKind::List(_))) {
            let Some(first) = items.first().and_then(|item| match &item.kind {
                ExprKind::List(row) => Some(row.as_slice()),
                _ => None,
            }) else {
                self.error("E-SHAPE-004", "matrix literal row must be a list", expr.source);
                return None;
            };
            let nested_tensor = first
                .iter()
                .any(|cell| matches!(&cell.kind, ExprKind::List(_)));
            if nested_tensor {
                return self.lower_tensor_literal(expr, items);
            }
            return self.lower_matrix_literal(expr, items);
        }
        let count = items.len();
        let mut lowered = Vec::with_capacity(count);
        for item in items {
            let (id, infer) = self.lower_expr(item)?;
            if !is_numeric_element(&infer) {
                self.error("E-TYPE-012", "vector element must be numeric", item.source);
                return None;
            }
            lowered.push(id);
        }
        self.record("sema", "vector literal", expr.source);
        let id = self.push_expr(ExprNode::Vector(lowered), expr.source);
        Some((
            id,
            Infer::Vector {
                extent: Some(Extent::Fixed(count)),
            },
        ))
    }

    fn lower_matrix_literal(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        let num_rows = items.len();
        let mut matrix_rows = Vec::with_capacity(num_rows);
        let mut expected_cols = None;
        for row_item in items {
            let ExprKind::List(row_elements) = &row_item.kind else {
                self.error(
                    "E-SHAPE-004",
                    "matrix row must be a list literal",
                    row_item.source,
                );
                return None;
            };
            if row_elements.is_empty() {
                self.error("E-SHAPE-004", "empty matrix row is not allowed", row_item.source);
                return None;
            }
            if let Some(cols) = expected_cols {
                if row_elements.len() != cols {
                    self.error(
                        "E-SHAPE-005",
                        format!(
                            "matrix rows must have uniform column counts: expected {cols}, found {}",
                            row_elements.len()
                        ),
                        row_item.source,
                    );
                    return None;
                }
            } else {
                expected_cols = Some(row_elements.len());
            }
            let mut lowered_row = Vec::with_capacity(row_elements.len());
            for elem in row_elements {
                let (id, infer) = self.lower_expr(elem)?;
                if !is_numeric_element(&infer) {
                    self.error("E-TYPE-012", "matrix element must be numeric", elem.source);
                    return None;
                }
                lowered_row.push(id);
            }
            matrix_rows.push(lowered_row);
        }
        self.record("sema", "matrix literal", expr.source);
        let id = self.push_expr(ExprNode::Matrix(matrix_rows), expr.source);
        Some((
            id,
            Infer::Matrix {
                rows: Some(Extent::Fixed(num_rows)),
                cols: expected_cols.map(Extent::Fixed),
            },
        ))
    }

    fn lower_tensor_literal(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        let mut elements = Vec::new();
        let mut shape = Vec::new();
        collect_tensor_literal(self, items, 0, &mut shape, &mut elements)?;
        if shape.len() < 3 {
            self.error(
                "E-SHAPE-004",
                "tensor literals must have rank >= 3; use Vector or Matrix for rank 1/2",
                expr.source,
            );
            return None;
        }
        self.record("sema", "tensor literal", expr.source);
        let id = self.push_expr(
            ExprNode::Tensor {
                shape: shape.clone(),
                elements,
            },
            expr.source,
        );
        Some((
            id,
            Infer::Tensor {
                shape: shape.into_iter().map(Extent::Fixed).collect(),
            },
        ))
    }

    fn lower_index(
        &mut self,
        expr: &Expr,
        value: &Expr,
        indices: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        let (target_id, target_infer) = self.lower_expr(value)?;
        let axes = match &target_infer {
            Infer::Vector { extent } => vec![extent.clone()],
            Infer::Matrix { rows, cols } => vec![rows.clone(), cols.clone()],
            Infer::Tensor { shape } => shape.iter().cloned().map(Some).collect(),
            _ => {
                self.error(
                    "E-TYPE-012",
                    "indexing is only supported on Vector, Matrix, and Tensor values",
                    value.source,
                );
                return None;
            }
        };
        if indices.len() != axes.len() {
            self.error(
                "E-SHAPE-006",
                format!(
                    "index requires {} subscript(s), found {}",
                    axes.len(),
                    indices.len()
                ),
                expr.source,
            );
            return None;
        }
        let mut out_shape = Vec::new();
        let mut slice_axes = Vec::new();
        let mut index_ids = Vec::new();
        let mut saw_slice = false;
        for (axis, (index, extent)) in indices.iter().zip(axes.into_iter()).enumerate() {
            match lower_index_axis(self, index, extent.as_ref(), axis)? {
                IndexAxis::Point(id) => {
                    index_ids.push(id);
                    slice_axes.push(emath_ir::SliceAxis::Point(id));
                }
                IndexAxis::Slice { start, end, extent } => {
                    saw_slice = true;
                    slice_axes.push(emath_ir::SliceAxis::Range { start, end });
                    out_shape.push(extent);
                }
            }
        }
        if !saw_slice {
            self.record("sema", "scalar index", expr.source);
            let id = self.push_expr(
                ExprNode::Index {
                    value: target_id,
                    indices: index_ids,
                },
                expr.source,
            );
            return Some((id, Infer::F64));
        }
        self.record("sema", "slice index", expr.source);
        let id = self.push_expr(
            ExprNode::Slice {
                value: target_id,
                axes: slice_axes,
            },
            expr.source,
        );
        Some((id, infer_from_shape(out_shape)))
    }
}

pub(super) fn expr_number(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => parse_float_constant(text),
        ExprKind::Unary {
            op: SynUnOp::Neg,
            value,
        } => expr_number(value).map(|value| -value),
        _ => None,
    }
}

