//! Residual rate rewriting and state/name helpers for equation admission.

use super::*;

impl Admitter {
    /// Replace `der(state)` inside residual expressions with the placeholder
    /// `__rate_<state>`, collecting rate unknowns; refuses rates that already
    /// have an explicit equation.
    pub(super) fn rewrite_residual_rates(
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
            | ExprKind::Rational { .. }
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

pub(in crate::admit) fn residual_span(admitter: &Admitter, expr: ExprId) -> Span {
    admitter
        .exprs
        .get(expr.0 as usize)
        .map(|(_, span)| *span)
        .unwrap_or_default()
}

/// Collect variable names referenced by a lowered expression tree, threading
/// the expression arena through the walk.
pub(in crate::admit) fn collect_node_names(
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

pub(in crate::admit) fn is_infer_marker(ty: &TypeExpr) -> bool {
    matches!(
        &ty.kind,
        SynTypeKind::Path {
            segments,
            generic_args,
        } if generic_args.is_empty() && segments.last().map(String::as_str) == Some("Infer")
    )
}
