//! Expression lowering: lowers parsed `.emath` expressions into typed
//! EMIR expression nodes with stable inference.

use emath_core::QualifiedName;
use emath_core::tree::{
    BinaryOp as SynBinOp, BinderKind, DerivativeKind, Expr, ExprKind, UnaryOp as SynUnOp,
};
use emath_ir::{
    BinaryOp, DistributionKind, ExprId, ExprNode, Extent, Literal, TypeNode, UnitDim, UnitFamily,
    lookup_unit,
};

mod call;
mod exprs;
mod goals;
mod helpers;
mod series;
pub(super) mod sibling_calls;
mod terms;

use super::Admitter;
use super::equations::*;
use super::expr_helpers::*;
use super::infer::*;
use super::sections::{integer_range, restore_index_local};
use super::{E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE};
use crate::recognition::expr_text;

/// Typed refusal: a tolerance-less `≈` edge. An approximation without a
/// declared tolerance is never admitted as if it were exact (04 §6.4).
const E_APPROX_TOL: &str = "E-APPROX-TOL";

fn capability_input_admits(input: &str, infer: &Infer) -> bool {
    match input.trim() {
        "Float64" | "F64" => matches!(infer, Infer::F64),
        "Bool" => matches!(infer, Infer::Bool),
        // Naturals are integers: admitting a Nat argument where `Int` is
        // declared preserves exactness, and value-level kernel guards
        // (checked arithmetic, positive-domain refusals) still fire at
        // evaluation. Literal `1` infers as Nat, so refusing Nat here
        // would make every integer-literal capsule call unusable.
        "Int" => matches!(infer, Infer::Int | Infer::Nat),
        "Nat" => matches!(infer, Infer::Nat),
        "ExactInt" | "PositiveExactInt" | "PrimeModulus" => {
            matches!(infer, Infer::Int | Infer::Nat | Infer::BigInt)
        }
        "Rat" | "Rational" => matches!(infer, Infer::Rat),
        "BigInt" => matches!(infer, Infer::BigInt),
        text if text.starts_with("Vector") => matches!(infer, Infer::Vector { .. }),
        text if text.starts_with("Matrix") => matches!(infer, Infer::Matrix { .. }),
        _ => false,
    }
}

/// The declared-output type text of a capability cell → the call's
/// inferred type. The mapping reads the cell declaration's OWN contract
/// (the small closed set of Phase-1 type spellings); anything outside
/// it is opaque — never a silently-assumed scalar.
fn capability_result_infer(output: Option<&str>) -> Infer {
    match output.map(str::trim) {
        Some("Float64") | Some("F64") => Infer::F64,
        Some("Bool") => Infer::Bool,
        Some("Int") | Some("ExactInt") => Infer::Int,
        Some("Nat") => Infer::Nat,
        Some("Rat") | Some("Rational") => Infer::Rat,
        Some("BigInt") => Infer::BigInt,
        Some(text) if text.starts_with("Vector") => Infer::Vector {
            extent: None,
            element: None,
        },
        Some(text) if text.starts_with("Matrix") => Infer::Matrix {
            rows: None,
            cols: None,
        },
        _ => Infer::Opaque,
    }
}

fn interpolation_paths(template: &str) -> Vec<&str> {
    let mut paths = Vec::new();
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'{' {
            if bytes.get(index + 1) == Some(&b'{') {
                index += 2;
                continue;
            }
            if let Some(relative_end) = template[index + 1..].find('}') {
                let end = index + 1 + relative_end;
                let field = template[index + 1..end]
                    .split_once(':')
                    .map_or(&template[index + 1..end], |(path, _)| path);
                paths.push(field);
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    paths
}

fn graph_tuple_parts(items: &[Expr]) -> Option<(&[Expr], &[Expr])> {
    if items.len() != 2 {
        return None;
    }
    let (ExprKind::List(nodes), ExprKind::List(edges)) = (&items[0].kind, &items[1].kind) else {
        return None;
    };
    if edges
        .iter()
        .all(|edge| matches!(&edge.kind, ExprKind::List(parts) if parts.len() == 4))
    {
        Some((nodes, edges))
    } else {
        None
    }
}

/// A numeric literal with an optional unary sign — `1`, `-1.0`, `+2`,
/// and nested sign chains (`--1.0` folds to `1.0`). The recursive fold
/// over `Unary { Neg | Pos }` of `Int`/`Float` spellings admits signed
/// graph edge weights (`0 -[-1.0]-> 1`) and signed node labels through
/// the SAME literal helper, without graph-specific parser branches.
/// Non-literal forms (paths, calls, arithmetic, measured quotes) still
/// yield `None` and refuse `E-TYPE-012` at the call site.
fn signed_numeric_literal(expr: &Expr) -> Option<f64> {
    fn raw(expr: &Expr) -> Option<(f64, bool)> {
        match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => {
                parse_float_constant(text).map(|value| (value, false))
            }
            ExprKind::Unary {
                op: SynUnOp::Neg,
                value,
            } => raw(value).map(|(value, neg)| (value, !neg)),
            ExprKind::Unary {
                op: SynUnOp::Pos,
                value,
            } => raw(value),
            _ => None,
        }
    }
    raw(expr).and_then(|(value, neg)| {
        let value = if neg { -value } else { value };
        value.is_finite().then_some(value)
    })
}

/// Unit families must agree inside one composed unit spelling
/// (`kg*m^2/s^2` is fine; `m*ft` is a typed refusal).
fn combine_unit_families(
    admitter: &mut Admitter,
    left: UnitFamily,
    right: UnitFamily,
    expr: &Expr,
) -> Option<UnitFamily> {
    if left == right {
        Some(left)
    } else {
        admitter.error(
            "E-UNIT-101",
            format!("unit family mismatch in unit comparison: {left:?} vs {right:?}"),
            expr.source,
        );
        None
    }
}

impl super::Admitter {
    /// Compile-time unit comparison:
    /// `unit of E == spelling`, `dimension of E == spelling`, query-to-query
    /// forms, and their `!=` negations. Both sides resolve to a static
    /// (dimension vector, family); the equality is computed at admission.
    /// A held comparison admits as a constant Bool with a receipt naming
    /// the computed units; a failed comparison is the typed refusal
    /// `E-UNIT-101` (or `E-UNIT-104` for an unresolvable spelling), never
    /// a silently-true claim. A bare `unit of E` outside a comparison is
    /// unchanged: still a named refuse (`E-TYPE-010`) — a unit is not a
    /// Phase-1 value.
    fn lower_unit_query_comparison(
        &mut self,
        op: SynBinOp,
        left: &Expr,
        right: &Expr,
    ) -> Option<(ExprId, Infer)> {
        let (left_dims, left_family, left_label) = self.static_unit_of(left)?;
        let (right_dims, right_family, right_label) = self.static_unit_of(right)?;
        let equal = left_dims == right_dims && left_family == right_family;
        if equal != matches!(op, SynBinOp::Eq) {
            self.error(
                "E-UNIT-101",
                format!(
                    "unit query computed false: `{}` has {} but `{}` has {}; \
                     the comparison does not hold",
                    expr_text(left),
                    left_label,
                    expr_text(right),
                    right_label,
                ),
                left.source.cover(right.source),
            );
            return None;
        }
        self.record(
            "sema",
            format!(
                "unit query computed: `unit of` comparison {} {} {} ({} vs {}); \
                 units are compile-time data, admitted",
                expr_text(left),
                if matches!(op, SynBinOp::Eq) {
                    "=="
                } else {
                    "!="
                },
                expr_text(right),
                left_label,
                right_label,
            ),
            left.source.cover(right.source),
        );
        let id = self.push_expr(
            ExprNode::Literal(Literal::Bool(true)),
            left.source.cover(right.source),
        );
        Some((id, Infer::Bool))
    }

    /// Static unit of an expression in a unit comparison: a `unit of` /
    /// `dimension of` query (the inner expression's inferred unit), a unit
    /// spelling (`m`, `m^2`, `kg*m^2/s^2`), a quantity literal, or an
    /// arithmetic composition of spellings.
    fn static_unit_of(&mut self, expr: &Expr) -> Option<(UnitDim, UnitFamily, String)> {
        match &expr.kind {
            ExprKind::UnitQuery { expr, .. } => {
                let (_, infer) = self.lower_expr(expr)?;
                match infer {
                    Infer::Unit { dims, family, .. } => Some((dims, family, expr_text(expr))),
                    // Dimensionless numeric: a bare Float64/Int input or a
                    // fully cancelled expression.
                    Infer::F64 | Infer::Nat | Infer::Int => {
                        Some((UnitDim::one(), UnitFamily::Si, expr_text(expr)))
                    }
                    other => {
                        self.error(
                            "E-TYPE-010",
                            format!(
                                "`unit of` requires a unit-carrying operand, found {:?}",
                                other
                            ),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Path { segments, .. } if segments.len() == 1 => {
                let name = &segments[0];
                match lookup_unit(name) {
                    Ok(unit) => Some((unit.dims, unit.family, name.clone())),
                    Err(_) => {
                        self.error(
                            "E-UNIT-104",
                            format!("unknown unit `{name}` in unit comparison"),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Binary {
                op: SynBinOp::Mul,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let (rd, rf, rl) = self.static_unit_of(right)?;
                Some((
                    ld.mul(rd),
                    combine_unit_families(self, lf, rf, expr)?,
                    format!("{ll}*{rl}"),
                ))
            }
            ExprKind::Binary {
                op: SynBinOp::Div,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let (rd, rf, rl) = self.static_unit_of(right)?;
                Some((
                    ld.div(rd),
                    combine_unit_families(self, lf, rf, expr)?,
                    format!("{ll}/{rl}"),
                ))
            }
            ExprKind::Binary {
                op: SynBinOp::Pow,
                left,
                right,
            } => {
                let (ld, lf, ll) = self.static_unit_of(left)?;
                let exponent = match &right.kind {
                    ExprKind::Int(text) | ExprKind::Float(text) => text.parse::<i32>().ok(),
                    _ => None,
                };
                let Some(exponent) = exponent else {
                    self.error(
                        "E-TYPE-010",
                        "unit power must be an integer literal",
                        right.source,
                    );
                    return None;
                };
                Some((ld.pow(exponent), lf, format!("{ll}^{exponent}")))
            }
            ExprKind::Quantity { unit, .. } => {
                // A quantity literal on one side: compare against its unit.
                let mut dims = UnitDim::one();
                let mut family = UnitFamily::Si;
                let mut label = String::new();
                for (name, power) in unit.flatten() {
                    let Ok(looked_up) = lookup_unit(&name) else {
                        self.error(
                            "E-UNIT-104",
                            format!("unknown unit `{name}` in unit comparison"),
                            expr.source,
                        );
                        return None;
                    };
                    let factor_dims = looked_up.dims.pow(power);
                    dims = if power >= 0 {
                        dims.mul(factor_dims)
                    } else {
                        dims.div(factor_dims)
                    };
                    family = looked_up.family;
                    if label.is_empty() {
                        label = name;
                    }
                }
                Some((dims, family, label))
            }
            other => {
                self.error(
                    "E-TYPE-010",
                    format!(
                        "unit comparison requires a unit spelling or unit query on each side, found {}",
                        expr_form_name(other)
                    ),
                    expr.source,
                );
                None
            }
        }
    }

    pub(super) fn lower_requirement(&mut self, expr: &Expr) -> Option<ExprId> {
        // Claim expressions (limit, series, asymp) are admitted as stated
        // claims in require/invariant. They produce Bool(true) — the claim
        // is recorded but not computationally verified in Phase 1.
        let prev_claim = self.in_claim_context;
        self.in_claim_context = true;
        let result = self.lower_expr(expr);
        self.in_claim_context = prev_claim;
        let (id, infer) = result?;
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
