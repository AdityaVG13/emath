//! Sample-limit, limit, optimize, solve, and derivative lowering arms.

use super::*;

impl super::super::Admitter {
    pub(super) fn lower_sample_limit_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } = &expr.kind
        else {
            unreachable!()
        };
        // Lower as a SampleLimit node: the body is compiled as a
        // sub-program with the limit variable as an input.
        let dir_bits = match direction {
            emath_core::tree::LimitDirection::TwoSided => 0.0_f64,
            emath_core::tree::LimitDirection::FromAbove => 1.0_f64,
            emath_core::tree::LimitDirection::FromBelow => -1.0_f64,
        };
        let (target_id, _) = self.lower_expr(target)?;
        let dir_id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(dir_bits.to_bits())),
            expr.source,
        );
        // Register the limit variable as a temporary input so the
        // body can reference it.
        let prev = self.inputs.insert(var.clone(), Infer::F64);
        let (body_id, body_infer) = self.lower_expr(body)?;
        if let Some(p) = prev {
            self.inputs.insert(var.clone(), p);
        } else {
            self.inputs.remove(var);
        }
        if !is_numeric_element(&body_infer) {
            self.error(
                "E-TYPE-012",
                "sample_limit body must be numeric",
                body.source,
            );
            return None;
        }
        let id = self.push_expr(
            ExprNode::SampleLimit {
                body: body_id,
                var: var.clone(),
                target: target_id,
                direction: dir_id,
            },
            expr.source,
        );
        self.record(
            "sema",
            format!("sample_limit {var} → numerical limit approximation"),
            expr.source,
        );
        Some((id, Infer::F64))
    }

    pub(super) fn lower_limit_expr_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } = &expr.kind
        else {
            unreachable!()
        };
        if self.in_claim_context {
            // Admit as a stated claim: Bool(true), not verified.
            self.record(
                "sema",
                format!("limit {var} -> claim admitted (not computationally verified)"),
                expr.source,
            );
            let _ = (target, direction, body);
            let id = self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
            return Some((id, Infer::Bool));
        }
        let dir = match direction {
            emath_core::tree::LimitDirection::TwoSided => "",
            emath_core::tree::LimitDirection::FromAbove => "+",
            emath_core::tree::LimitDirection::FromBelow => "-",
        };
        self.error(
            E_UNSUPPORTED_TYPE,
            format!(
                "`limit {var} -> {dir}` is a claim, not a computation; \
                         use `sample_limit` for numerical evaluation or place in `require`/`invariant`"
            ),
            expr.source,
        );
        let _ = (target, body);
        None
    }

    pub(super) fn lower_optimize_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Optimize {
            value,
            wrt,
            maximize,
        } = &expr.kind
        else {
            unreachable!()
        };
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
            ExprNode::Optimize {
                body: body_with_penalty,
                vars: var_names.clone(),
                maximize: *maximize,
            },
            expr.source,
        );
        let direction = if *maximize { "maximize" } else { "minimize" };
        self.record(
            "sema",
            format!(
                "{direction} wrt {} → Newton stationarity (∇f = 0)",
                var_names.join(", ")
            ),
            expr.source,
        );
        Some((id, Infer::F64))
    }

    pub(super) fn lower_solve_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Solve { value, wrt } = &expr.kind else {
            unreachable!()
        };
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
            self.error("E-TYPE-012", "solve body must be numeric", value.source);
            return None;
        }
        let inlined = self.inline_defs(body_id);
        let id = self.push_expr(
            ExprNode::Solve {
                body: inlined,
                var: var_name.clone(),
            },
            expr.source,
        );
        self.record(
            "sema",
            format!("solve wrt {var_name} → Newton's method root-finding"),
            expr.source,
        );
        Some((id, Infer::F64))
    }

    pub(super) fn lower_derivative_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Derivative { kind, holding, .. } = &expr.kind else {
            unreachable!()
        };
        // Partial without `holding` is a MeaningHole: autodiff wrt
        // one input would silently hold every other input fixed.
        if *kind == DerivativeKind::Partial {
            if holding.is_empty() {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    "partial derivative requires an explicit `holding` set \
                             (e.g. `partial(H) wrt T holding p`); the compiler will not \
                             guess which variables are held fixed",
                    expr.source,
                );
                return None;
            }
            for held in holding {
                let Some(segments) = path_segments(held) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "holding variable must be a plain name",
                        held.source,
                    );
                    return None;
                };
                let held_name = &segments[0];
                if self.lookup(held_name).is_none() {
                    self.error(
                        E_UNKNOWN_VARIABLE,
                        format!("unknown holding variable `{held_name}`"),
                        held.source,
                    );
                    return None;
                }
            }
        }
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
            ExprNode::Differentiate {
                body: inlined,
                var: var_name.clone(),
            },
            expr.source,
        );
        self.record(
            "sema",
            format!("derivative wrt {var_name} → forward-mode autodiff"),
            expr.source,
        );
        Some((id, Infer::F64))
    }
}
