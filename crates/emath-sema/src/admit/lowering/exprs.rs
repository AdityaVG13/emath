//! Set-comprehension, approx, and binary-operator lowering arms.

use super::*;

impl super::super::Admitter {
    pub(super) fn lower_set_comprehension_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::SetComprehension {
            element,
            var,
            domain,
            guard,
        } = &expr.kind
        else {
            unreachable!()
        };
        let Some((start, end)) = integer_range(domain) else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "set comprehensions require a finite literal integer range",
                domain.source,
            );
            return None;
        };
        if end < start || end - start > 10_000 {
            self.error(
                "E-DOM-002",
                "set comprehension range must be ordered and at most 10000 elements",
                domain.source,
            );
            return None;
        }
        let previous = self.index_locals.insert(var.clone(), start);
        let mut elements = Vec::with_capacity((end - start) as usize);
        let mut guards = Vec::with_capacity((end - start) as usize);
        let mut element_infer = None;
        for value in start..end {
            self.index_locals.insert(var.clone(), value);
            let Some((element_id, infer)) = self.lower_expr(element) else {
                restore_index_local(&mut self.index_locals, var, previous);
                return None;
            };
            if let Some(expected) = &element_infer {
                if expected != &infer {
                    self.error(
                        "E-TYPE-012",
                        "set comprehension elements must have one type",
                        element.source,
                    );
                    restore_index_local(&mut self.index_locals, var, previous);
                    return None;
                }
            } else {
                element_infer = Some(infer);
            }
            let guard_id = if let Some(guard) = guard {
                let Some((id, infer)) = self.lower_expr(guard) else {
                    restore_index_local(&mut self.index_locals, var, previous);
                    return None;
                };
                if infer != Infer::Bool {
                    self.error(
                        "E-TYPE-012",
                        "set comprehension guard must be Boolean",
                        guard.source,
                    );
                    restore_index_local(&mut self.index_locals, var, previous);
                    return None;
                }
                Some(id)
            } else {
                None
            };
            elements.push(element_id);
            guards.push(guard_id);
        }
        restore_index_local(&mut self.index_locals, var, previous);
        let id = self.push_expr(ExprNode::Set { elements, guards }, expr.source);
        Some((
            id,
            Infer::Set(Box::new(element_infer.unwrap_or(Infer::F64))),
        ))
    }

    pub(super) fn lower_approx_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Approx {
            left,
            right,
            tolerance,
        } = &expr.kind
        else {
            unreachable!()
        };
        let (left_id, left_infer) = self.lower_expr(left)?;
        let (right_id, right_infer) = self.lower_expr(right)?;
        if !self.in_claim_context {
            self.error(
                E_UNSUPPORTED_TYPE,
                "approximation (`≈`) is a claim, not a computation; \
                         use it in `require` or `invariant`",
                expr.source,
            );
            return None;
        }
        let Some(tolerance) = tolerance else {
            self.error(
                E_APPROX_TOL,
                "bare `≈` has no declared tolerance; an approximation without a \
                         tolerance is never admitted as exact — declare one \
                         (`within rtol=…, atol=…`)",
                expr.source,
            );
            return None;
        };
        let mut declared: Vec<String> = Vec::new();
        if tolerance.rtol.is_some() {
            declared.push("rtol".to_string());
        }
        if tolerance.atol.is_some() {
            declared.push("atol".to_string());
        }
        self.record(
            "sema",
            format!(
                "≈ approximation claim admitted with declared tolerance ({}) — \
                         authority degraded through the ≈ edge, never recovered upward, \
                         not computationally exact",
                declared.join(", ")
            ),
            expr.source,
        );
        combine_numeric(&left_infer, &right_infer, NumericCombine::Sub, expr, self)?;
        let zero = |admitter: &mut Self| {
            admitter.push_expr(
                ExprNode::Literal(Literal::FloatBits(0.0_f64.to_bits())),
                expr.source,
            )
        };
        let lower_tolerance =
            |admitter: &mut Self, value: Option<&Expr>, label: &str| -> Option<ExprId> {
                let Some(value) = value else {
                    return Some(zero(admitter));
                };
                let (id, infer) = admitter.lower_expr(value)?;
                if !matches!(
                    infer,
                    Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                ) {
                    admitter.error(
                        "E-TYPE-012",
                        format!("approximation {label} must be dimensionless numeric"),
                        value.source,
                    );
                    return None;
                }
                Some(id)
            };
        let rtol = lower_tolerance(self, tolerance.rtol.as_ref(), "rtol")?;
        let atol = lower_tolerance(self, tolerance.atol.as_ref(), "atol")?;
        let difference = self.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatSub,
                left: left_id,
                right: right_id,
            },
            expr.source,
        );
        let absolute_difference = self.push_expr(
            ExprNode::Call {
                function: QualifiedName::single("abs"),
                arguments: vec![difference],
            },
            expr.source,
        );
        let absolute_reference = self.push_expr(
            ExprNode::Call {
                function: QualifiedName::single("abs"),
                arguments: vec![right_id],
            },
            expr.source,
        );
        let relative = self.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: rtol,
                right: absolute_reference,
            },
            expr.source,
        );
        let threshold = self.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatAdd,
                left: atol,
                right: relative,
            },
            expr.source,
        );
        let id = self.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::LessEqual,
                left: absolute_difference,
                right: threshold,
            },
            expr.source,
        );
        Some((id, Infer::Bool))
    }

    pub(super) fn lower_binary_expr_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Binary { op, left, right } = &expr.kind else {
            unreachable!()
        };
        // Unit queries compute (
        // 04 §1.4): `unit of E == spelling`, `unit of a == unit of b`
        // and the `!=` forms are compile-time comparisons over the
        // static unit layer — evaluated at admission, never pushed
        // as runtime arithmetic.
        if matches!(op, SynBinOp::Eq | SynBinOp::Ne)
            && (matches!(left.kind, ExprKind::UnitQuery { .. })
                || matches!(right.kind, ExprKind::UnitQuery { .. }))
        {
            return self.lower_unit_query_comparison(*op, left, right);
        }
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
            SynBinOp::Add => match (&l_infer, &r_infer) {
                (Infer::Vector { extent: ext_l, .. }, Infer::Vector { extent: ext_r, .. }) => {
                    if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                        if l_e != r_e {
                            self.error(
                                "E-SHAPE-005",
                                format!(
                                    "dimension mismatch in vector addition: {l_e:?} vs {r_e:?}"
                                ),
                                expr.source,
                            );
                            return None;
                        }
                    }
                    let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                    self.record("sema", "vector add", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::VectorAdd,
                        expr,
                        l,
                        r,
                        Infer::Vector {
                            extent: res_extent,
                            element: None,
                        },
                    )
                }
                (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                    if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                        if r1_e != r2_e {
                            self.error(
                                "E-SHAPE-005",
                                "matrix row dimension mismatch in addition",
                                expr.source,
                            );
                            return None;
                        }
                    }
                    if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                        if c1_e != c2_e {
                            self.error(
                                "E-SHAPE-005",
                                "matrix col dimension mismatch in addition",
                                expr.source,
                            );
                            return None;
                        }
                    }
                    self.record("sema", "matrix add", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixAdd,
                        expr,
                        l,
                        r,
                        Infer::Matrix {
                            rows: r1.clone().or_else(|| r2.clone()),
                            cols: c1.clone().or_else(|| c2.clone()),
                        },
                    )
                }
                (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                    let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                    self.record("sema", "tensor add", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::TensorAdd,
                        expr,
                        l,
                        r,
                        Infer::Tensor { shape },
                    )
                }
                _ => {
                    let result =
                        combine_numeric(&l_infer, &r_infer, NumericCombine::Add, expr, self)?;
                    self.record("sema", "add → strict f64 add", expr.source);
                    arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r, result)
                }
            },
            SynBinOp::Sub => match (&l_infer, &r_infer) {
                (Infer::Vector { extent: ext_l, .. }, Infer::Vector { extent: ext_r, .. }) => {
                    if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                        if l_e != r_e {
                            self.error(
                                "E-SHAPE-005",
                                format!(
                                    "dimension mismatch in vector subtraction: {l_e:?} vs {r_e:?}"
                                ),
                                expr.source,
                            );
                            return None;
                        }
                    }
                    let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                    self.record("sema", "vector subtract", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::VectorSub,
                        expr,
                        l,
                        r,
                        Infer::Vector {
                            extent: res_extent,
                            element: None,
                        },
                    )
                }
                (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                    if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                        if r1_e != r2_e {
                            self.error(
                                "E-SHAPE-005",
                                "matrix row dimension mismatch in subtraction",
                                expr.source,
                            );
                            return None;
                        }
                    }
                    if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                        if c1_e != c2_e {
                            self.error(
                                "E-SHAPE-005",
                                "matrix col dimension mismatch in subtraction",
                                expr.source,
                            );
                            return None;
                        }
                    }
                    self.record("sema", "matrix subtract", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixSub,
                        expr,
                        l,
                        r,
                        Infer::Matrix {
                            rows: r1.clone().or_else(|| r2.clone()),
                            cols: c1.clone().or_else(|| c2.clone()),
                        },
                    )
                }
                (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                    let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                    self.record("sema", "tensor subtract", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::TensorSub,
                        expr,
                        l,
                        r,
                        Infer::Tensor { shape },
                    )
                }
                _ => {
                    self.record("sema", "subtract → strict f64 subtract", expr.source);
                    let result =
                        combine_numeric(&l_infer, &r_infer, NumericCombine::Sub, expr, self)?;
                    arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r, result)
                }
            },
            SynBinOp::Mul => match (&l_infer, &r_infer) {
                (Infer::Vector { extent, .. }, Infer::F64 | Infer::HostDeferred) => {
                    self.record("sema", "vector scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::VectorScale,
                        expr,
                        l,
                        r,
                        Infer::Vector {
                            extent: extent.clone(),
                            element: None,
                        },
                    )
                }
                (Infer::F64 | Infer::HostDeferred, Infer::Vector { extent, .. }) => {
                    self.record("sema", "vector scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::VectorScale,
                        expr,
                        r,
                        l,
                        Infer::Vector {
                            extent: extent.clone(),
                            element: None,
                        },
                    )
                }
                (Infer::Matrix { rows, cols }, Infer::F64 | Infer::HostDeferred) => {
                    self.record("sema", "matrix scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixScale,
                        expr,
                        l,
                        r,
                        Infer::Matrix {
                            rows: rows.clone(),
                            cols: cols.clone(),
                        },
                    )
                }
                (Infer::F64 | Infer::HostDeferred, Infer::Matrix { rows, cols }) => {
                    self.record("sema", "matrix scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixScale,
                        expr,
                        r,
                        l,
                        Infer::Matrix {
                            rows: rows.clone(),
                            cols: cols.clone(),
                        },
                    )
                }
                (Infer::Tensor { shape }, Infer::F64 | Infer::HostDeferred) => {
                    self.record("sema", "tensor scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::TensorScale,
                        expr,
                        l,
                        r,
                        Infer::Tensor {
                            shape: shape.clone(),
                        },
                    )
                }
                (Infer::F64 | Infer::HostDeferred, Infer::Tensor { shape }) => {
                    self.record("sema", "tensor scale", expr.source);
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::TensorScale,
                        expr,
                        r,
                        l,
                        Infer::Tensor {
                            shape: shape.clone(),
                        },
                    )
                }
                (Infer::Matrix { rows, cols }, Infer::Vector { extent, .. }) => {
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
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixMulVector,
                        expr,
                        l,
                        r,
                        Infer::Vector {
                            extent: rows.clone(),
                            element: None,
                        },
                    )
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
                    arithmetic(
                        self,
                        emath_ir::BinaryOp::MatrixMulMatrix,
                        expr,
                        l,
                        r,
                        Infer::Matrix {
                            rows: r1.clone(),
                            cols: c2.clone(),
                        },
                    )
                }
                _ => {
                    self.record("sema", "multiply → strict f64 multiply", expr.source);
                    let result =
                        combine_numeric(&l_infer, &r_infer, NumericCombine::Mul, expr, self)?;
                    arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r, result)
                }
            },
            SynBinOp::Div => {
                self.record("sema", "divide → strict f64 divide", expr.source);
                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Div, expr, self)?;
                arithmetic(self, emath_ir::BinaryOp::StrictFloatDiv, expr, l, r, result)
            }
            SynBinOp::Pow => {
                self.record("sema", "power → strict f64 powf", expr.source);
                if !matches!(
                    (&l_infer, &r_infer),
                    (
                        Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred,
                        Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
                    )
                ) {
                    self.error(
                        "E-TYPE-012",
                        "operator `^` requires dimensionless numeric operands",
                        expr.source,
                    );
                    return None;
                }
                arithmetic(
                    self,
                    emath_ir::BinaryOp::StrictFloatPow,
                    expr,
                    l,
                    r,
                    Infer::F64,
                )
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
            SynBinOp::And | SynBinOp::Or | SynBinOp::Imply | SynBinOp::Iff => {
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
                            operation: match op {
                                SynBinOp::And => emath_ir::BinaryOp::And,
                                SynBinOp::Or => emath_ir::BinaryOp::Or,
                                SynBinOp::Imply => emath_ir::BinaryOp::Imply,
                                SynBinOp::Iff => emath_ir::BinaryOp::Iff,
                                _ => unreachable!(),
                            },
                            left: l,
                            right: r,
                        },
                        expr.source,
                    ),
                    Infer::Bool,
                ))
            }
            SynBinOp::In => {
                let Infer::Set(element) = &r_infer else {
                    self.error(
                        "E-TYPE-012",
                        "membership (`in`) requires a finite set on the right",
                        expr.source,
                    );
                    return None;
                };
                if **element != l_infer {
                    self.error(
                        "E-TYPE-012",
                        format!(
                            "membership element has type {l_infer}, but the set contains {element}"
                        ),
                        expr.source,
                    );
                    return None;
                }
                Some((
                    self.push_expr(
                        ExprNode::Binary {
                            operation: emath_ir::BinaryOp::SetContains,
                            left: l,
                            right: r,
                        },
                        expr.source,
                    ),
                    Infer::Bool,
                ))
            }
            SynBinOp::Asymp => {
                if self.in_claim_context {
                    // Admit as a stated claim: Bool(true).
                    self.record(
                        "sema",
                        "asymptotic equivalence (`~~`) claim admitted (not computationally verified)",
                        expr.source,
                    );
                    let id = self.push_expr(ExprNode::Literal(Literal::Bool(true)), expr.source);
                    return Some((id, Infer::Bool));
                }
                self.error(
                    E_UNSUPPORTED_TYPE,
                    "asymptotic equivalence (`~~`) is a claim, not a computation; \
                             use it in `require` or `invariant`",
                    expr.source,
                );
                return None;
            }
        }
    }
}
