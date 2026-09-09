//! Sample-limit, limit, optimize, solve, and derivative lowering arms.

use super::*;

pub(super) const CAPABILITY_SIMPSON: &str = "std.capability.calculus.simpson-integral";
pub(super) const CAPABILITY_NEWTON: &str = "std.capability.calculus.scalar-solve";
pub(super) const KERNEL_FORWARD: &str = "program-forward-difference";
pub(super) const CAPABILITY_OPTIMIZE: &str = "std.capability.program.optimize";
pub(super) const CAPABILITY_SAMPLE_LIMIT: &str = "std.capability.program.sample-limit";

const DEFAULT_SOLVE_TOLERANCE: f64 = 1e-10;
const DEFAULT_SOLVE_MAX_ITER: i64 = 64;
const DEFAULT_OPT_LEARNING_RATE: f64 = 0.0;
const DEFAULT_OPT_TOLERANCE: f64 = 1e-8;
const DEFAULT_OPT_MAX_ITER: i64 = 64;
pub(super) const DEFAULT_INTEGRAL_STEPS: i64 = 128;

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
        if !is_numeric_element(&body_infer) {
            if let Some(p) = prev {
                self.inputs.insert(var.clone(), p);
            } else {
                self.inputs.remove(var);
            }
            self.error(
                "E-TYPE-012",
                "sample_limit body must be numeric",
                body.source,
            );
            return None;
        }
        let program_inputs = self.program_input_names();
        let Some(slot) = Self::slot_index_in(&program_inputs, var) else {
            if let Some(p) = prev {
                self.inputs.insert(var.clone(), p);
            } else {
                self.inputs.remove(var);
            }
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("sample_limit variable `{var}` is not a program input"),
                expr.source,
            );
            return None;
        };
        if let Some(p) = prev {
            self.inputs.insert(var.clone(), p);
        } else {
            self.inputs.remove(var);
        }
        let extra = vec![self.push_i64(slot, expr.source), target_id, dir_id];
        let Some((id, infer)) = self.apply_program_capability(
            CAPABILITY_SAMPLE_LIMIT,
            body_id,
            program_inputs,
            extra,
            expr.source,
        ) else {
            return None;
        };
        self.record(
            "sema",
            format!("sample_limit {var} → program-carrier geometric sampler"),
            expr.source,
        );
        Some((id, infer))
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
        let program_inputs = self.program_input_names();
        let mut slot_ids = Vec::with_capacity(var_names.len());
        for name in &var_names {
            let Some(slot) = Self::slot_index_in(&program_inputs, name) else {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!("optimization variable `{name}` is not a program input"),
                    expr.source,
                );
                return None;
            };
            slot_ids.push(self.push_f64(slot as f64, expr.source));
        }
        let indices = self.push_expr(ExprNode::Vector(slot_ids), expr.source);
        let extra = vec![
            indices,
            self.push_bool(*maximize, expr.source),
            self.push_f64(DEFAULT_OPT_LEARNING_RATE, expr.source),
            self.push_f64(DEFAULT_OPT_TOLERANCE, expr.source),
            self.push_i64(DEFAULT_OPT_MAX_ITER, expr.source),
        ];
        let Some((id, infer)) = self.apply_program_capability(
            CAPABILITY_OPTIMIZE,
            body_with_penalty,
            program_inputs,
            extra,
            expr.source,
        ) else {
            return None;
        };
        let direction = if *maximize { "maximize" } else { "minimize" };
        self.record(
            "sema",
            format!(
                "{direction} wrt {} → program-carrier Newton stationarity",
                var_names.join(", ")
            ),
            expr.source,
        );
        Some((id, infer))
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
        let program_inputs = self.program_input_names();
        let Some(slot) = Self::slot_index_in(&program_inputs, &var_name) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("solve variable `{var_name}` is not a program input"),
                expr.source,
            );
            return None;
        };
        let extra = vec![
            self.push_i64(slot, expr.source),
            self.push_f64(DEFAULT_SOLVE_TOLERANCE, expr.source),
            self.push_i64(DEFAULT_SOLVE_MAX_ITER, expr.source),
        ];
        let Some((id, infer)) =
            self.apply_program_capability(CAPABILITY_NEWTON, inlined, program_inputs, extra, expr.source)
        else {
            return None;
        };
        self.record(
            "sema",
            format!("solve wrt {var_name} → program-carrier Newton root"),
            expr.source,
        );
        Some((id, infer))
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
        let program_inputs = self.program_input_names();
        let Some(slot) = Self::slot_index_in(&program_inputs, &var_name) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("derivative variable `{var_name}` is not a program input"),
                expr.source,
            );
            return None;
        };
        let extra = vec![self.push_i64(slot, expr.source)];
        let Some((id, infer)) =
            self.apply_program_kernel(KERNEL_FORWARD, inlined, program_inputs, extra, expr.source)
        else {
            return None;
        };
        self.record(
            "sema",
            format!("derivative wrt {var_name} → program-carrier forward difference"),
            expr.source,
        );
        Some((id, infer))
    }

    pub(super) fn capability_by_kernel(
        &self,
        kernel: &str,
    ) -> Option<super::super::CapabilityCallBinding> {
        self.capability_cells
            .iter()
            .find(|binding| binding.kernel.as_deref() == Some(kernel))
            .cloned()
    }

    pub(super) fn push_i64(&mut self, value: i64, span: emath_core::Span) -> ExprId {
        self.push_expr(ExprNode::Literal(Literal::Integer(value.to_string())), span)
    }

    pub(super) fn push_f64(&mut self, value: f64, span: emath_core::Span) -> ExprId {
        self.push_expr(ExprNode::Literal(Literal::FloatBits(value.to_bits())), span)
    }

    fn push_bool(&mut self, value: bool, span: emath_core::Span) -> ExprId {
        self.push_expr(ExprNode::Literal(Literal::Bool(value)), span)
    }

    pub(super) fn program_input_names(&self) -> Vec<String> {
        let mut names = self.inputs.keys().cloned().collect::<Vec<_>>();
        for name in self.states.keys() {
            let ir_name = format!("state.{name}");
            if !names
                .iter()
                .any(|existing| existing == name || existing == &ir_name)
            {
                names.push(ir_name);
            }
        }
        names
    }

    fn push_environment_for(
        &mut self,
        program_inputs: &[String],
        span: emath_core::Span,
    ) -> ExprId {
        let elements = program_inputs
            .iter()
            .map(|name| {
                let bare = name.strip_prefix("state.").unwrap_or(name);
                if self.inputs.contains_key(name)
                    || self.inputs.contains_key(bare)
                    || self.states.contains_key(bare)
                {
                    self.push_expr(
                        ExprNode::Variable(emath_core::QualifiedName(name.clone())),
                        span,
                    )
                } else {
                    self.push_f64(0.0, span)
                }
            })
            .collect();
        self.push_expr(ExprNode::Vector(elements), span)
    }

    pub(super) fn apply_program_kernel(
        &mut self,
        kernel: &str,
        body: ExprId,
        program_inputs: Vec<String>,
        extra: Vec<ExprId>,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let Some(binding) = self.capability_by_kernel(kernel) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "program-carrier capability is not executable in the loaded Language Image",
                span,
            );
            return None;
        };
        self.apply_program_capability(&binding.key, body, program_inputs, extra, span)
    }

    pub(super) fn apply_program_capability(
        &mut self,
        capability: &str,
        body: ExprId,
        program_inputs: Vec<String>,
        extra: Vec<ExprId>,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let program = self.push_expr(
            ExprNode::Program { body, inputs: program_inputs.clone() },
            span,
        );
        let environment = self.push_environment_for(&program_inputs, span);
        let mut arguments = Vec::with_capacity(2 + extra.len());
        arguments.push(program);
        arguments.push(environment);
        arguments.extend(extra);
        self.apply_capability(capability, arguments, Infer::F64, span)
    }

    pub(in crate::admit) fn apply_capability(
        &mut self,
        capability: &str,
        arguments: Vec<ExprId>,
        result: Infer,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let Some(capability) = self.capability_cells.iter()
            .find(|binding| binding.key == capability)
            .map(|binding| emath_ir::CapabilityId(binding.capability))
        else {
            self.error(E_UNSUPPORTED_TYPE, "capability is not executable in the loaded Language Image", span);
            return None;
        };
        let id = self.push_expr(
            ExprNode::Apply {
                capability,
                arguments,
            },
            span,
        );
        Some((id, result))
    }

    pub(super) fn apply_kernel(
        &mut self,
        kernel: &str,
        arguments: Vec<ExprId>,
        result: Infer,
        span: emath_core::Span,
    ) -> Option<(ExprId, Infer)> {
        let Some(binding) = self.capability_by_kernel(kernel) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "capability kernel is not executable in the loaded Language Image",
                span,
            );
            return None;
        };
        let id = self.push_expr(
            ExprNode::Apply {
                capability: emath_ir::CapabilityId(binding.capability),
                arguments,
            },
            span,
        );
        Some((id, result))
    }

    pub(super) fn slot_index_in(program_inputs: &[String], name: &str) -> Option<i64> {
        program_inputs
            .iter()
            .position(|input| input == name)
            .map(|index| index as i64)
    }
}
