//! Helper methods for expression lowering, extracted from lowering.rs.
//! Mechanical move — no logic changes.

use emath_core::tree::{BinderKind, Expr, ExprKind};
use emath_ir::{ExprId, ExprNode, Extent, Literal};

use super::super::expr_helpers::*;
use super::super::infer::*;
use super::super::sections::*;
use super::super::{E_UNKNOWN_FUNCTION, E_UNSUPPORTED_TYPE};

impl super::super::Admitter {
    pub(super) fn lower_finite_binder(
        &mut self,
        expr: &Expr,
        kind: BinderKind,
        binders: &[emath_core::tree::Binder],
        body: &Expr,
        guard: Option<&Expr>,
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
            return self.lower_variable_bound_binder(expr, kind, binder, domain, body, guard);
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
            return self.lower_variable_bound_binder(expr, kind, binder, domain, body, guard);
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
            let (term_id, term_infer) = if let Some(guard_expr) = guard {
                // B02: filtered fold — if guard is true, use body; else identity.
                let (guard_id, guard_infer) = match self.lower_expr(guard_expr) {
                    Some(result) => result,
                    None => {
                        restore_index_local(&mut self.index_locals, &binder.name, previous);
                        return None;
                    }
                };
                if !matches!(guard_infer, Infer::Bool) {
                    self.error(
                        "E-TYPE-012",
                        "binder guard (`if`) must be a Boolean expression",
                        guard_expr.source,
                    );
                    restore_index_local(&mut self.index_locals, &binder.name, previous);
                    return None;
                }
                let (body_id, body_infer) = match self.lower_expr(body) {
                    Some(result) => result,
                    None => {
                        restore_index_local(&mut self.index_locals, &binder.name, previous);
                        return None;
                    }
                };
                let identity_id = self.push_expr(
                    ExprNode::Literal(Literal::FloatBits(identity.to_bits())),
                    expr.source,
                );
                let select_id = self.push_expr(
                    ExprNode::If {
                        condition: guard_id,
                        then_value: body_id,
                        else_value: identity_id,
                    },
                    expr.source,
                );
                (select_id, body_infer)
            } else {
                match self.lower_expr(body) {
                    Some(term) => term,
                    None => {
                        restore_index_local(&mut self.index_locals, &binder.name, previous);
                        return None;
                    }
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

    pub(super) fn lower_variable_bound_binder(
        &mut self,
        expr: &Expr,
        kind: BinderKind,
        binder: &emath_core::tree::Binder,
        domain: &Expr,
        body: &Expr,
        guard: Option<&Expr>,
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
        // B02: if a guard is present, wrap the body in a conditional:
        // if guard then body else identity.
        let body_id = if let Some(guard_expr) = guard {
            let (guard_id, guard_infer) = match self.lower_expr(guard_expr) {
                Some(result) => result,
                None => {
                    restore_input(&mut self.inputs, &binder.name, prev);
                    return None;
                }
            };
            if !matches!(guard_infer, Infer::Bool) {
                self.error(
                    "E-TYPE-012",
                    "binder guard (`if`) must be a Boolean expression",
                    guard_expr.source,
                );
                restore_input(&mut self.inputs, &binder.name, prev);
                return None;
            }
            let identity_literal = match kind {
                BinderKind::Sum | BinderKind::Integral => Literal::FloatBits(0.0f64.to_bits()),
                BinderKind::Product => Literal::FloatBits(1.0f64.to_bits()),
                BinderKind::ForAll => Literal::Bool(true),
                BinderKind::Exists => Literal::Bool(false),
            };
            let identity_id = self.push_expr(ExprNode::Literal(identity_literal), expr.source);
            self.push_expr(
                ExprNode::If {
                    condition: guard_id,
                    then_value: body_id,
                    else_value: identity_id,
                },
                expr.source,
            )
        } else {
            body_id
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

    pub(super) fn lower_reduction(
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

    pub(super) fn lower_list_literal(
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

    pub(super) fn lower_matrix_literal(
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

    pub(super) fn lower_tensor_literal(
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

    pub(super) fn lower_index(
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
