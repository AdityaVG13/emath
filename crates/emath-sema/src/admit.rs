//! The Phase 1 admission pass: syntax → typed neutral SIR with stable
//! diagnostics and a source-to-SIR trace.

use emath_core::tree::{Expr, ExprKind, UnaryOp as SynUnOp};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::{
    BinaryOp, ExprId, ExprNode,
    Literal, ModelResidual, TypeId, TypeNode,
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
mod sections;
mod lowering;
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

