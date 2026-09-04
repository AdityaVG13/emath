//! Equation and residual admission: derivative detection, equation
//! causalization, implicit residual rewriting, and related helpers
//! extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind, Section, StmtKind, TypeExpr, TypeKind as SynTypeKind,
};
use emath_core::{QualifiedName, Span};
use emath_ir::{BinaryOp, ExprId, ExprNode, Extent, ModelResidual, TypeNode, UnitDim};
use std::collections::{BTreeMap, BTreeSet};

use super::infer::{Infer, NumericCombine, combine_numeric, is_numeric_element};
use super::{Admitter, E_DUPLICATE_FIELD, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE};

mod state;

pub(super) use state::*;

pub(super) fn ty_display(node: &TypeNode) -> String {
    node.display_name()
}

pub(super) fn state_variable_name(admitter: &Admitter, segments: &[String], name: &str) -> String {
    if name.starts_with("state.") {
        return name.to_string();
    }
    if segments.len() == 1
        && admitter.states.contains_key(&segments[0])
        && !admitter.inputs.contains_key(&segments[0])
        && !admitter.params.contains_key(&segments[0])
        && !admitter.definitions.contains_key(&segments[0])
    {
        return format!("state.{}", segments[0]);
    }
    name.to_string()
}

pub(super) fn path_segments(expr: &Expr) -> Option<&[String]> {
    match &expr.kind {
        ExprKind::Path { segments, .. } => Some(segments),
        _ => None,
    }
}

pub(super) fn is_time_name(name: &str) -> bool {
    matches!(name, "t" | "time")
}

pub(super) fn is_der_call(function: &Expr) -> bool {
    path_segments(function).is_some_and(|segments| {
        segments.len() == 1 && matches!(segments[0].as_str(), "der" | "derivative")
    })
}

/// Explicit `derivative(state)` / `der(state)` / `derivative state wrt t`.
pub(super) fn unwrap_derivative(expr: &Expr) -> Option<(&Expr, Option<&[Expr]>)> {
    match &expr.kind {
        ExprKind::Derivative { value, wrt, .. } => {
            let wrt = wrt.as_deref();
            if let ExprKind::Derivative {
                value: inner,
                wrt: None,
                ..
            } = &value.kind
            {
                Some((inner, wrt))
            } else {
                Some((value.as_ref(), wrt))
            }
        }
        ExprKind::Call { function, args } if args.len() == 1 && is_der_call(function) => {
            Some((&args[0], None))
        }
        _ => None,
    }
}

pub(super) fn derivative_state_name(expr: &Expr) -> Result<Option<String>, (&'static str, String)> {
    let Some((value, wrt)) = unwrap_derivative(expr) else {
        return Ok(None);
    };
    if let Some(wrt) = wrt {
        if wrt.len() != 1 {
            return Err((
                "E-TYPE-010",
                "only a single independent variable `t`/`time` is admitted on `derivative`".into(),
            ));
        }
        let Some(segments) = path_segments(&wrt[0]) else {
            return Err((
                "E-TYPE-010",
                "derivative independent variable must be `t` or `time`".into(),
            ));
        };
        if segments.len() != 1 || !is_time_name(&segments[0]) {
            return Err((
                "E-TYPE-010",
                "derivative independent variable must be `t` or `time`".into(),
            ));
        }
    }
    let Some(segments) = path_segments(value) else {
        return Err((
            "E-TYPE-010",
            "only `derivative(state)` of a named state field is admitted".into(),
        ));
    };
    let name = if segments.first().map(String::as_str) == Some("state") {
        segments.get(1)
    } else {
        segments.first()
    };
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return Err((
            "E-TYPE-010",
            "only `derivative(state)` of a named state field is admitted".into(),
        ));
    };
    Ok(Some(name.clone()))
}

/// `der(x)` or `m * der(x)` / `m * derivative(x)` with a named scalar mass.
pub(super) fn split_mass_times_derivative(
    expr: &Expr,
) -> Result<(String, Option<String>), (&'static str, String)> {
    if let Some(name) = derivative_state_name(expr)? {
        return Ok((name, None));
    }
    let ExprKind::Binary {
        op: SynBinOp::Mul,
        left,
        right,
    } = &expr.kind
    else {
        return Err((
            "E-TYPE-010",
            "only explicit `derivative(state) = rhs` or scalar `m * derivative(state) = rhs` equations are admitted".into(),
        ));
    };
    let (mass, der) = if unwrap_derivative(right).is_some() {
        (left.as_ref(), right.as_ref())
    } else if unwrap_derivative(left).is_some() {
        (right.as_ref(), left.as_ref())
    } else {
        return Err((
            "E-TYPE-010",
            "only explicit `derivative(state) = rhs` or scalar `m * derivative(state) = rhs` equations are admitted".into(),
        ));
    };
    let Some(segments) = path_segments(mass) else {
        return Err((
            "E-TYPE-010",
            "mass-matrix factor must be a named scalar input".into(),
        ));
    };
    if segments.len() != 1 {
        return Err((
            "E-TYPE-010",
            "mass-matrix factor must be a named scalar input".into(),
        ));
    }
    let name = derivative_state_name(der)?.ok_or((
        "E-TYPE-010",
        "only `m * derivative(state)` of a named state field is admitted".into(),
    ))?;
    Ok((name, Some(segments[0].clone())))
}

pub(super) fn rate_unit_mismatch(
    state: Option<&Infer>,
    rate: &Infer,
) -> Option<(&'static str, String)> {
    let Some(Infer::Unit { dims, family, .. }) = state else {
        return None;
    };
    let time = UnitDim::base(0, 0, 1, 0, 0, 0, 0);
    let expected = dims.div(time);
    match rate {
        Infer::Unit {
            dims: rate_dims,
            family: rate_family,
            ..
        } if rate_family == family && *rate_dims == expected => None,
        Infer::Unit {
            dims: rate_dims, ..
        } => Some((
            "E-UNIT-101",
            format!(
                "rate dimensions {} do not match state/time {}",
                rate_dims.render(),
                expected.render()
            ),
        )),
        Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred => Some((
            "E-UNIT-101",
            "dimension mismatch: cannot use a dimensionless rate for a quantity state".into(),
        )),
        _ => None,
    }
}

pub(super) fn admit_equations(
    admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    definitions: &mut BTreeMap<String, ExprId>,
    is_model: bool,
) {
    for section_name in ["equations", "equation"] {
        let Some(section) = by_name.get(section_name) else {
            continue;
        };
        if !is_model {
            admitter.error(
                "E-KIND-010",
                "`equations:` (with `derivative(state) = rhs` rows) describes continuous dynamics and is only admitted on `emath model` declarations; a stateless formula uses `definitions:` on `emath function`, a stateful object uses `emath policy`",
                section.source,
            );
            continue;
        }
        for stmt in &section.suite.statements {
            // Accept Equation, Assign, and bare-expression statements:
            // - Equation: `derivative(state) = rhs`, `m * derivative(state) = rhs`,
            //   or any other `lhs = rhs` (implicit residual, causalized)
            // - Assign: `algebraic_var = expr` (semi-explicit DAE)
            // - Expr: `lhs == rhs` comparisons or bare `expr` (implicit residual)
            // `eq_style` records whether the statement used `=`/assignment
            // spelling; only those may become algebraic definitions. A
            // `==` comparison is always a residual, even when the left
            // side is a bare name (`a == 5` constrains `a`, it does not
            // define it).
            let (left, right, eq_style) = match &stmt.kind {
                StmtKind::Equation { left, right } => (left.clone(), right.clone(), true),
                StmtKind::Expr(expr) => match &expr.kind {
                    // `V - R * I - q / C == 0`: split the comparison so the
                    // residual is `left - right`.
                    ExprKind::Binary {
                        op: SynBinOp::Eq,
                        left,
                        right,
                    } => ((**left).clone(), (**right).clone(), false),
                    // Bare `expr` means `expr == 0`.
                    _ => {
                        let zero = Expr {
                            kind: ExprKind::Int("0".into()),
                            source: expr.source,
                        };
                        (expr.clone(), zero, false)
                    }
                },
                StmtKind::Assign { target, value } => {
                    if !target.indices.is_empty() {
                        admitter.error(
                            "E-SYN-101",
                            "indexed assignments are not allowed in `equations:`",
                            stmt.source,
                        );
                        continue;
                    }
                    let left = Expr {
                        kind: ExprKind::Path {
                            segments: target.segments.clone(),
                            generics: None,
                        },
                        source: target.source,
                    };
                    (left, value.clone(), true)
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "only `derivative(state) = rhs`, `name = expr`, or `lhs == rhs` / bare residual expressions are allowed in `equations:`",
                        stmt.source,
                    );
                    continue;
                }
            };
            let (left, right) = (&left, &right);
            let (state_name, mass) = match split_mass_times_derivative(left) {
                Ok(split) => split,
                Err((code, message)) => {
                    // Check if this is an algebraic definition: `name = expr`
                    // (semi-explicit DAE support).  The left side must be a
                    // plain identifier that is not a state field (state
                    // fields use `der(state) = rhs`). Only `=`-spelled
                    // statements (eq_style) may become definitions — `==`
                    // comparisons are always residuals.
                    if eq_style {
                        if let Some(segments) = path_segments(left) {
                            if segments.len() == 1 {
                                let name = segments[0].clone();
                                if admitter.states.contains_key(&name) {
                                    admitter.error(
                                    "E-TYPE-010",
                                    format!("state field `{name}` must use `derivative({name}) = rhs`, not `{name} = rhs`"),
                                    left.source,
                                );
                                    continue;
                                }
                                if definitions.contains_key(&name) {
                                    admitter.error(
                                        E_DUPLICATE_FIELD,
                                        format!("duplicate definition `{name}`"),
                                        left.source,
                                    );
                                    continue;
                                }
                                let Some((id, infer)) = admitter.lower_expr(right) else {
                                    continue;
                                };
                                if !is_numeric_element(&infer)
                                    && !matches!(
                                        infer,
                                        Infer::Vector { .. }
                                            | Infer::Matrix { .. }
                                            | Infer::Tensor { .. }
                                    )
                                {
                                    admitter.error(
                                        "E-TYPE-012",
                                        format!("algebraic definition `{name}` must be numeric"),
                                        right.source,
                                    );
                                    continue;
                                }
                                admitter.record(
                                    "sema",
                                    format!("algebraic definition `{name}` in equations"),
                                    left.source,
                                );
                                definitions.insert(name.clone(), id);
                                admitter.definitions.insert(name, (id, infer));
                                continue;
                            }
                        }
                    }
                    // Not an explicit rate and not an algebraic definition:
                    // causalize it as an implicit residual `left - right`,
                    // solved for the declaration's unknowns at each step.
                    if admit_residual(admitter, left, right, definitions).is_some() {
                        continue;
                    }
                    admitter.error(code, message, left.source);
                    continue;
                }
            };
            // A named mass factor that is a vector/matrix/tensor cannot be
            // rewritten to `der(x) = rhs / mass`; causalize the equation as
            // an implicit residual over the rate unknown instead.
            if let Some(mass_name) = &mass {
                let scalar_mass = admitter.lookup(mass_name).is_some_and(|infer| {
                    !matches!(
                        infer,
                        Infer::Vector { .. } | Infer::Matrix { .. } | Infer::Tensor { .. }
                    )
                });
                if !scalar_mass {
                    if admit_residual(admitter, left, right, definitions).is_some() {
                        continue;
                    }
                    admitter.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "non-scalar mass factor `{mass_name}` requires the implicit residual form; cannot rewrite to `der(...) = rhs / {mass_name}`"
                        ),
                        left.source,
                    );
                    continue;
                }
            }
            if !admitter.states.contains_key(&state_name) {
                admitter.error(
                    E_UNKNOWN_VARIABLE,
                    format!("unknown state field `{state_name}` in derivative"),
                    left.source,
                );
                continue;
            }
            let rate_name = format!("der_{state_name}");
            if definitions.contains_key(&rate_name) {
                admitter.error(
                    E_DUPLICATE_FIELD,
                    format!("duplicate rate `{rate_name}`"),
                    left.source,
                );
                continue;
            }
            let Some((mut id, mut infer)) = admitter.lower_expr(right) else {
                continue;
            };
            if let Some(mass_name) = mass {
                if !admitter.inputs.contains_key(&mass_name)
                    && !admitter.params.contains_key(&mass_name)
                    && !admitter.definitions.contains_key(&mass_name)
                {
                    admitter.error(
                        "E-TYPE-010",
                        format!(
                            "mass-matrix factor `{mass_name}` must be a scalar input, parameter, or definition"
                        ),
                        left.source,
                    );
                    continue;
                }
                let Some(mass_infer) = admitter.lookup(&mass_name) else {
                    continue;
                };
                if !matches!(
                    mass_infer,
                    Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred
                ) {
                    admitter.error(
                        "E-TYPE-010",
                        format!("mass-matrix factor `{mass_name}` must be a scalar"),
                        left.source,
                    );
                    continue;
                }
                let mass_id = admitter.push_expr(
                    ExprNode::Variable(QualifiedName(mass_name.clone())),
                    left.source,
                );
                id = admitter.push_expr(
                    ExprNode::Binary {
                        operation: emath_ir::BinaryOp::StrictFloatDiv,
                        left: id,
                        right: mass_id,
                    },
                    left.source,
                );
                infer = match combine_numeric(
                    &infer,
                    &mass_infer,
                    NumericCombine::Div,
                    right,
                    admitter,
                ) {
                    Some(combined) => combined,
                    None => continue,
                };
                admitter.record(
                    "sema",
                    format!("mass-matrix rewrite `{mass_name} * der({state_name})` → `der_{state_name} = rhs / {mass_name}`"),
                    left.source,
                );
            }
            match infer {
                infer @ (Infer::F64
                | Infer::Nat
                | Infer::Int
                | Infer::Complex
                | Infer::Unit { .. }
                | Infer::HostDeferred
                | Infer::Vector { .. }
                | Infer::Matrix { .. }
                | Infer::Tensor { .. }
                | Infer::OptionCarrier
                | Infer::ResultCarrier) => {
                    if let Some((code, message)) =
                        rate_unit_mismatch(admitter.states.get(&state_name), &infer)
                    {
                        admitter.error(code, message, right.source);
                    }
                    admitter.record("sema", format!("rate `{rate_name}` typed"), right.source);
                    definitions.insert(rate_name.clone(), id);
                    admitter.definitions.insert(rate_name, (id, infer));
                }
                Infer::Bool
                | Infer::Text
                | Infer::Rat
                // Stage-2 (emath-t63iz): rates feed f64-backed state
                // integration; exact big field elements are not rates.
                | Infer::BigInt
                | Infer::Set(_)
                | Infer::Record(_)
                | Infer::Opaque
                | Infer::Series
                | Infer::Sequence => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("rate `der_{state_name}` must be numeric or tensor"),
                        right.source,
                    );
                }
            }
        }
    }
}

/// Causalize `left == right` into an implicit residual: `der(state)` becomes
/// `__rate_<state>`, and unknowns are the `algebraic:` variables. `None` when
/// it cannot be typed as a scalar or fixed-length vector.
pub(super) fn admit_residual(
    admitter: &mut Admitter,
    left: &Expr,
    right: &Expr,
    definitions: &BTreeMap<String, ExprId>,
) -> Option<ModelResidual> {
    let mut rates: Vec<String> = Vec::new();
    let left = admitter.rewrite_residual_rates(left, definitions, &mut rates)?;
    let right = admitter.rewrite_residual_rates(right, definitions, &mut rates)?;
    let (left_id, l_infer) = admitter.lower_expr(&left)?;
    let (right_id, r_infer) = admitter.lower_expr(&right)?;
    let (operation, components) = residual_difference(admitter, &l_infer, &r_infer, left.source)?;
    let expr = admitter.push_expr(
        ExprNode::Binary {
            operation,
            left: left_id,
            right: right_id,
        },
        left.source,
    );
    let expr = admitter.inline_defs(expr);
    rates.dedup();
    admitter.record(
        "sema",
        format!(
            "implicit residual: {components} component(s), rate unknowns [{}]",
            rates.join(", ")
        ),
        left.source,
    );
    let residual = ModelResidual {
        expr,
        components: u16::try_from(components).unwrap_or(u16::MAX),
        algebraic: Vec::new(),
        rates,
    };
    admitter.residuals.push(residual.clone());
    Some(residual)
}

/// Pick the subtraction operation and component count for a residual
/// `left - right`. Scalar and fixed-extent vector operands are admitted.
pub(super) fn residual_difference(
    admitter: &mut Admitter,
    l: &Infer,
    r: &Infer,
    span: Span,
) -> Option<(emath_ir::BinaryOp, usize)> {
    use Infer::Vector;
    fn scalar(infer: &Infer) -> bool {
        matches!(infer, Infer::F64 | Infer::Nat | Infer::Int)
    }
    match (l, r) {
        (l, r) if scalar(l) && scalar(r) => Some((BinaryOp::StrictFloatSub, 1)),
        (
            Vector {
                extent: Some(Extent::Fixed(le)),
                ..
            },
            Vector {
                extent: Some(Extent::Fixed(re)),
                ..
            },
        ) => {
            if le != re {
                admitter.error(
                    "E-SHAPE-005",
                    format!("dimension mismatch in residual subtraction: {le} vs {re}"),
                    span,
                );
                return None;
            }
            Some((BinaryOp::VectorSub, *le))
        }
        _ => {
            admitter.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "implicit residual must subtract scalar from scalar or vector from vector, found {l:?} - {r:?}"
                ),
                span,
            );
            None
        }
    }
}
