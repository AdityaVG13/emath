//! Series-pair scalar extraction and duplicate-refusal suppression.

use super::*;

impl super::super::Admitter {
    /// True when an `E-NAME-023` refusal for exactly this declared
    /// output is already on record (message `output `<name>` has no
    /// definition`). The consequent `E-TYPE-002` "unknown variable" at
    /// later use sites is suppressed for such names (emath-2bwk):
    /// the empty `definitions:` block is the single root refusal, and
    /// repeating the same missing name at every use site buries it in
    /// noise. The message pair is pinned by the regression
    /// `empty_definitions_refuses_once_and_preserves_independent_errors`
    /// in tests/emath-sema/tests/session.rs, so a message edit that
    /// breaks the pairing fails that test.
    pub(super) fn undefined_output_already_refused(&self, name: &str) -> bool {
        let expected = format!("output `{name}` has no definition");
        self.diagnostics
            .errors()
            .any(|d| d.code == "E-NAME-023" && d.message == expected)
    }

    /// 04 §5.4: resolve one series pair element to its SI scalar. Only
    /// data literals admit: a quantity (`2.5 mg/L`) scales to SI; a bare
    /// numeric is dimensionless. Anything else (variables, calls,
    /// expressions) is not a datum — the series is not executable in
    /// this slice, so it must not smuggle in computation either.
    pub(super) fn series_pair_scalar(&mut self, expr: &Expr) -> Option<f64> {
        match &expr.kind {
            ExprKind::Quantity { value, unit } => {
                let Some(magnitude) = parse_quantity_magnitude(value) else {
                    self.error(
                        "E-UNIT-105",
                        "series pair elements must be numeric quantity literals",
                        expr.source,
                    );
                    return None;
                };
                let factors = unit.flatten();
                let mut scale = 1.0_f64;
                let mut offset = 0.0_f64;
                for (name, power) in &factors {
                    match lookup_unit(name) {
                        Ok(looked_up) => {
                            if looked_up.is_affine() && (*power != 1 || factors.len() != 1) {
                                self.error(
                                    "E-UNIT-102",
                                    format!(
                                        "affine unit misuse: `{}` cannot appear in a compound or powered series element",
                                        looked_up.name
                                    ),
                                    expr.source,
                                );
                                return None;
                            }
                            scale *= looked_up.scale.powi(*power);
                            if *power == 1 {
                                offset = looked_up.offset;
                            }
                        }
                        Err(error) => {
                            self.error(error.code, error.message, expr.source);
                            return None;
                        }
                    }
                }
                let si = (magnitude + offset) * scale;
                if si.is_finite() {
                    Some(si)
                } else {
                    self.error(
                        "E-TYPE-011",
                        "non-finite series element refused under the selected numeric model",
                        expr.source,
                    );
                    None
                }
            }
            ExprKind::Float(text) => match text.parse::<f64>() {
                Ok(value) if value.is_finite() => Some(value),
                _ => {
                    self.error(
                        "E-TYPE-011",
                        "non-finite series element refused under the selected numeric model",
                        expr.source,
                    );
                    None
                }
            },
            _ => {
                self.error(
                    "E-SYN-101",
                    "series pair elements are data literals (`2.5 mg/L`, `0.1 s`) — no expressions or references inside the series",
                    expr.source,
                );
                None
            }
        }
    }

    pub(in crate::admit) fn lower_expr(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        match &expr.kind {
            ExprKind::Int(text) => {
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Integer(text.clone())),
                    expr.source,
                );
                // Stage-2 band (emath-t63iz): a non-negative literal
                // beyond i64::MAX infers BigInt when it fits the
                // |F| < 2^256 bound (the emitter enforces the bound and
                // lowers to ConstBigInt). Digit-length pre-check keeps
                // this O(1); the 19-digit edge compares against
                // i64::MAX's decimal form.
                const I64_MAX: &str = "9223372036854775807";
                let stripped = text.replace('_', "");
                let infer = if stripped.starts_with('-') {
                    Infer::Int
                } else if stripped.len() > I64_MAX.len()
                    || (stripped.len() == I64_MAX.len() && stripped.as_str() > I64_MAX)
                {
                    Infer::BigInt
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
            ExprKind::Str(_) => self.lower_str_literal_arm(expr),
            ExprKind::Measured { .. } => self.lower_measured_arm(expr),
            ExprKind::WithSeriesPolicy { .. } => self.lower_with_series_policy_arm(expr),
            ExprKind::Quantity { .. } => self.lower_quantity_arm(expr),
            ExprKind::Path { .. } => self.lower_path_expr_arm(expr),
            ExprKind::Call { .. } => self.lower_call_expr_arm(expr),
            ExprKind::Unary { .. } => self.lower_unary_expr_arm(expr),
            ExprKind::Binary { .. } => self.lower_binary_expr_arm(expr),
            ExprKind::Approx { .. } => self.lower_approx_arm(expr),
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
            ExprKind::Cases {
                subject: _,
                arms,
                else_arm,
            } => {
                // U1: Lower `cases: | c1 => e1 | c2 => e2 | else => e3`
                // to nested `If { c1, e1, If { c2, e2, e3 } }`.
                // The subject is for readability only (arm conditions
                // are full expressions, not pattern matches).
                let (mut current_else, result_infer) = self.lower_expr(else_arm)?;
                for (cond, value) in arms.iter().rev() {
                    let (cond_id, cond_infer) = self.lower_expr(cond)?;
                    if !matches!(cond_infer, Infer::Bool) {
                        self.error(
                            "E-TYPE-012",
                            "cases arm condition must be Boolean",
                            cond.source,
                        );
                        return None;
                    }
                    let (val_id, val_infer) = self.lower_expr(value)?;
                    if val_infer != result_infer {
                        self.error(
                            "E-TYPE-012",
                            "cases arms must have the same type",
                            expr.source,
                        );
                        return None;
                    }
                    current_else = self.push_expr(
                        ExprNode::If {
                            condition: cond_id,
                            then_value: val_id,
                            else_value: current_else,
                        },
                        expr.source,
                    );
                }
                Some((current_else, result_infer))
            }
            ExprKind::List(items) => self.lower_list_literal(expr, items),
            ExprKind::Table { headers, rows } => self.lower_table_literal(expr, headers, rows),
            ExprKind::Tuple(items) if graph_tuple_parts(items).is_some() => {
                self.lower_graph_tuple(expr, items)
            }
            ExprKind::Set(items) => {
                let mut elements = Vec::with_capacity(items.len());
                let mut element_infer = None;
                for item in items {
                    let (id, infer) = self.lower_expr(item)?;
                    if let Some(expected) = &element_infer {
                        if expected != &infer {
                            self.error(
                                "E-TYPE-012",
                                "set literal elements must have one type",
                                item.source,
                            );
                            return None;
                        }
                    } else {
                        element_infer = Some(infer);
                    }
                    elements.push(id);
                }
                let element_infer = element_infer.unwrap_or(Infer::F64);
                let id = self.push_expr(
                    ExprNode::Set {
                        guards: vec![None; elements.len()],
                        elements,
                    },
                    expr.source,
                );
                Some((id, Infer::Set(Box::new(element_infer))))
            }
            ExprKind::SetComprehension { .. } => self.lower_set_comprehension_arm(expr),
            ExprKind::Record { type_path, fields } => {
                let mut lowered = std::collections::BTreeMap::new();
                for (name, value) in fields {
                    let (id, _) = self.lower_expr(value)?;
                    lowered.insert(name.clone(), id);
                }
                let name = QualifiedName(type_path.join("::"));
                let ty = self.type_id(TypeNode::Record(name.clone()));
                let id = self.push_expr(
                    ExprNode::Record {
                        ty,
                        fields: lowered,
                    },
                    expr.source,
                );
                Some((id, Infer::Record(name.0)))
            }
            ExprKind::Index { value, indices } => self.lower_index(expr, value, indices),
            ExprKind::Binder {
                kind,
                binders,
                body,
                guard,
            } => {
                // Series in claim context: admit as Bool(true).
                if *kind == BinderKind::Series && self.in_claim_context {
                    self.record(
                        "sema",
                        "series convergence claim admitted (not computationally verified)",
                        expr.source,
                    );
                    let id = self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
                    return Some((id, Infer::Bool));
                }
                self.lower_finite_binder(expr, *kind, binders, body, guard.as_deref())
            }
            ExprKind::Derivative { .. } => self.lower_derivative_arm(expr),
            ExprKind::Solve { .. } => self.lower_solve_arm(expr),
            ExprKind::Optimize { .. } => self.lower_optimize_arm(expr),
            ExprKind::Limit { .. } => self.lower_limit_expr_arm(expr),
            ExprKind::SampleLimit { .. } => self.lower_sample_limit_arm(expr),
            ExprKind::UnitQuery { kind, .. } => {
                let query = match kind {
                    emath_core::tree::UnitQueryKind::Unit => "unit of",
                    emath_core::tree::UnitQueryKind::Dimension => "dimension of",
                };
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`{query}` is a compile-time query: it parses, but Phase 1 does not evaluate it"
                    ),
                    expr.source,
                );
                None
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
}
