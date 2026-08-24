//! Equation and residual admission: derivative detection, equation
//! causalization, implicit residual rewriting, and related helpers
//! extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind, Section, StmtKind, TypeExpr, TypeKind as SynTypeKind,
};
use emath_core::{QualifiedName, Span};
use emath_ir::{BinaryOp, ExprId, ExprNode, Extent, ModelResidual, TypeNode, UnitDim};
use std::collections::{BTreeMap, BTreeSet};

use super::infer::{combine_numeric, is_numeric_element, Infer, NumericCombine};
use super::{E_DUPLICATE_FIELD, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE, Admitter};

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

pub(super) fn rate_unit_mismatch(state: Option<&Infer>, rate: &Infer) -> Option<(&'static str, String)> {
    let Some(Infer::Unit { dims, family }) = state else {
        return None;
    };
    let time = UnitDim::base(0, 0, 1, 0, 0, 0, 0);
    let expected = dims.div(time);
    match rate {
        Infer::Unit {
            dims: rate_dims,
            family: rate_family,
        } if rate_family == family && *rate_dims == expected => None,
        Infer::Unit { dims: rate_dims, .. } => Some((
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
                "equations are only admitted on `emath model` declarations",
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
                StmtKind::Equation { left, right } => {
                    (left.clone(), right.clone(), true)
                }
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
                                && !matches!(infer, Infer::Vector { .. } | Infer::Matrix { .. } | Infer::Tensor { .. })
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
                | Infer::Unit { .. }
                | Infer::HostDeferred
                | Infer::Vector { .. }
                | Infer::Matrix { .. }
                | Infer::Tensor { .. }) => {
                    if let Some((code, message)) =
                        rate_unit_mismatch(admitter.states.get(&state_name), &infer)
                    {
                        admitter.error(code, message, right.source);
                    }
                    admitter.record(
                        "sema",
                        format!("rate `{rate_name}` typed"),
                        right.source,
                    );
                    definitions.insert(rate_name.clone(), id);
                    admitter.definitions.insert(rate_name, (id, infer));
                }
                Infer::Bool | Infer::Opaque => {
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

/// Causalization: build an implicit residual from `left == right` (or a
/// bare `expr`, meaning `expr == 0`). `der(state)` occurrences without an
/// explicit rate are rewritten to the synthetic input `__rate_<state>`;
/// the remaining unknowns are the declaration's `algebraic:` variables.
/// Returns `None` (after a diagnostic) when the residual cannot be typed
/// as a scalar or fixed-length vector.
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
            },
            Vector {
                extent: Some(Extent::Fixed(re)),
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

impl Admitter {
    /// Replace `der(state)` / `derivative(state)` occurrences inside a
    /// residual expression with the placeholder variable `__rate_<state>`,
    /// collecting rate unknowns. Refuses residuals that reference a rate
    /// that already has an explicit equation, or constructs outside the
    /// Phase 1 subset.
    fn rewrite_residual_rates(
        &mut self,
        expr: &Expr,
        definitions: &BTreeMap<String, ExprId>,
        rates: &mut Vec<String>,
    ) -> Option<Expr> {
        let node = expr.clone();
        match &node.kind {
            ExprKind::Derivative { value, wrt, .. } => {
                if wrt.as_ref().is_some_and(|w| !is_time_wrt(w)) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "inside an implicit residual, `derivative` must be a time rate; only `t`/`time` is admitted as the independent variable",
                        expr.source,
                    );
                    return None;
                }
                self.rate_placeholder_for(value, definitions, rates, expr.source)
            }
            ExprKind::Call { function, args } if args.len() == 1 && is_der_call(function) => {
                self.rate_placeholder_for(&args[0], definitions, rates, expr.source)
            }
            ExprKind::Path { .. }
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_) => Some(node),
            ExprKind::Quantity { value, unit } => {
                let value = self.rewrite_residual_rates(value, definitions, rates)?;
                Some(Expr {
                    kind: ExprKind::Quantity {
                        value: Box::new(value),
                        unit: unit.clone(),
                    },
                    source: expr.source,
                })
            }
            ExprKind::Unary { op, value } => {
                let value = self.rewrite_residual_rates(value, definitions, rates)?;
                Some(Expr {
                    kind: ExprKind::Unary {
                        op: *op,
                        value: Box::new(value),
                    },
                    source: expr.source,
                })
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.rewrite_residual_rates(left, definitions, rates)?;
                let right = self.rewrite_residual_rates(right, definitions, rates)?;
                Some(Expr {
                    kind: ExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    source: expr.source,
                })
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.rewrite_residual_rates(condition, definitions, rates)?;
                let then_value = self.rewrite_residual_rates(then_value, definitions, rates)?;
                let else_value = self.rewrite_residual_rates(else_value, definitions, rates)?;
                Some(Expr {
                    kind: ExprKind::If {
                        condition: Box::new(condition),
                        then_value: Box::new(then_value),
                        else_value: Box::new(else_value),
                    },
                    source: expr.source,
                })
            }
            ExprKind::Call { function, args } => {
                let mut new_args = Vec::with_capacity(args.len());
                for arg in args {
                    new_args.push(self.rewrite_residual_rates(arg, definitions, rates)?);
                }
                Some(Expr {
                    kind: ExprKind::Call {
                        function: function.clone(),
                        args: new_args,
                    },
                    source: expr.source,
                })
            }
            _ => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    "this expression form is not admitted inside an implicit residual in Phase 1 (residuals admit arithmetic, builtin calls, and `der(state)` on state fields)",
                    expr.source,
                );
                None
            }
        }
    }

    /// One `der(x)` occurrence inside a residual: validate, register the
    /// placeholder infer, and return the rewritten synthetic path.
    fn rate_placeholder_for(
        &mut self,
        value: &Expr,
        definitions: &BTreeMap<String, ExprId>,
        rates: &mut Vec<String>,
        span: Span,
    ) -> Option<Expr> {
        let Some(name) = state_field_name(self, value) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "only `der(state.field)` / `derivative(state field)` of a declared state field is admitted inside an implicit residual",
                span,
            );
            return None;
        };
        let rate_name = format!("der_{name}");
        if definitions.contains_key(&rate_name) {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "rate `{rate_name}` already has an explicit equation; an implicit residual must not reference `der({name})` again"
                ),
                span,
            );
            return None;
        }
        let placeholder = format!("__rate_{name}");
        if !self.rate_placeholders.contains_key(&placeholder) {
            if let Some(infer) = self.states.get(&name) {
                self.rate_placeholders
                    .insert(placeholder.clone(), infer.clone());
            }
        }
        if !rates.iter().any(|existing| existing == &name) {
            rates.push(name);
        }
        Some(Expr {
            kind: ExprKind::Path {
                segments: vec![placeholder],
                generics: None,
            },
            source: span,
        })
    }
}

/// The state field name of `derivative(x)` / `derivative(state.x)`.
pub(super) fn state_field_name(admitter: &Admitter, value: &Expr) -> Option<String> {
    let segments = path_segments(value)?;
    let name = if segments.len() == 2 && segments[0] == "state" {
        segments[1].clone()
    } else if segments.len() == 1 {
        segments[0].clone()
    } else {
        return None;
    };
    admitter.states.contains_key(&name).then_some(name)
}

/// Whether a `derivative ... wrt` list is exactly `t` or `time`.
pub(super) fn is_time_wrt(wrt: &[Expr]) -> bool {
    wrt.len() == 1
        && path_segments(&wrt[0])
            .is_some_and(|segments| segments.len() == 1 && is_time_name(&segments[0]))
}

pub(super) fn residual_span(admitter: &Admitter, expr: ExprId) -> Span {
    admitter
        .exprs
        .get(expr.0 as usize)
        .map(|(_, span)| *span)
        .unwrap_or_default()
}

/// Collect variable names referenced by a lowered expression tree
/// (used to verify every `algebraic:` variable appears in a residual).
/// Children are arena references, so the expression arena is threaded
/// through the walk.
pub(super) fn collect_node_names(
    exprs: &[(ExprNode, Span)],
    node: &ExprNode,
    out: &mut BTreeSet<String>,
) {
    match node {
        ExprNode::Variable(name) => {
            let name = name.0.strip_prefix("state.").unwrap_or(&name.0);
            out.insert(name.to_string());
        }
        ExprNode::Unary { value, .. } => {
            if let Some((child, _)) = exprs.get(value.0 as usize) {
                collect_node_names(exprs, child, out);
            }
        }
        ExprNode::Binary { left, right, .. } => {
            if let Some((child, _)) = exprs.get(left.0 as usize) {
                collect_node_names(exprs, child, out);
            }
            if let Some((child, _)) = exprs.get(right.0 as usize) {
                collect_node_names(exprs, child, out);
            }
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            if let Some((child, _)) = exprs.get(condition.0 as usize) {
                collect_node_names(exprs, child, out);
            }
            if let Some((child, _)) = exprs.get(then_value.0 as usize) {
                collect_node_names(exprs, child, out);
            }
            if let Some((child, _)) = exprs.get(else_value.0 as usize) {
                collect_node_names(exprs, child, out);
            }
        }
        ExprNode::Call { arguments, .. } => {
            for argument in arguments {
                if let Some((child, _)) = exprs.get(argument.0 as usize) {
                    collect_node_names(exprs, child, out);
                }
            }
        }
        ExprNode::Index { value, indices } => {
            if let Some((child, _)) = exprs.get(value.0 as usize) {
                collect_node_names(exprs, child, out);
            }
            for index in indices {
                if let Some((child, _)) = exprs.get(index.0 as usize) {
                    collect_node_names(exprs, child, out);
                }
            }
        }
        ExprNode::Vector(elements) | ExprNode::Tensor { elements, .. } => {
            for element in elements {
                if let Some((child, _)) = exprs.get(element.0 as usize) {
                    collect_node_names(exprs, child, out);
                }
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows {
                for element in row {
                    if let Some((child, _)) = exprs.get(element.0 as usize) {
                        collect_node_names(exprs, child, out);
                    }
                }
            }
        }
        _ => {}
    }
}

pub(super) fn is_infer_marker(ty: &TypeExpr) -> bool {
    matches!(
        &ty.kind,
        SynTypeKind::Path {
            segments,
            generic_args,
        } if generic_args.is_empty() && segments.last().map(String::as_str) == Some("Infer")
    )
}
