//! Helper methods for expression lowering, extracted from lowering.rs.
//! Mechanical move — no logic changes.

use emath_core::tree::{BinderKind, Expr, ExprKind};
use emath_ir::{ExprId, ExprNode, Extent, Literal};

use super::super::E_UNSUPPORTED_TYPE;
use super::super::expr_helpers::*;
use super::super::infer::*;
use super::super::sections::*;

impl super::super::Admitter {
    pub(super) fn lower_finite_binder(
        &mut self,
        expr: &Expr,
        kind: BinderKind,
        binders: &[emath_core::tree::Binder],
        body: &Expr,
        guard: Option<&Expr>,
    ) -> Option<(ExprId, Infer)> {
        if binders.is_empty() {
            self.error(
                E_UNSUPPORTED_TYPE,
                format!("`{kind:?}` needs at least one binder variable"),
                expr.source,
            );
            return None;
        }
        if binders.len() > 1 {
            // Multi-binder desugar (emath-6kk1b): leftmost binder =
            // outermost, rightmost = innermost; the optional guard binds
            // to the innermost binder. Each nesting level is the SAME
            // kind (sum∘sum ≡ sum, product likewise, forall/exists
            // compose associatively, integral nesting is the standard
            // iterated-integral convention). The synthesized tree
            // re-enters THIS path one binder at a time, so range checks,
            // caps, guards, and inference rules apply per level with
            // zero new lowering code.
            let mut nested = Expr {
                kind: ExprKind::Binder {
                    kind,
                    binders: vec![binders[binders.len() - 1].clone()],
                    body: Box::new(body.clone()),
                    guard: guard.map(|g| Box::new(g.clone())),
                },
                source: expr.source,
            };
            for binder in binders[..binders.len() - 1].iter().rev() {
                nested = Expr {
                    kind: ExprKind::Binder {
                        kind,
                        binders: vec![binder.clone()],
                        body: Box::new(nested),
                        guard: None,
                    },
                    source: expr.source,
                };
            }
            self.record(
                "sema",
                format!(
                    "multi-binder {kind:?} over {} variables → nested single-binder folds (rightmost innermost)",
                    binders.len()
                ),
                expr.source,
            );
            return self.lower_expr(&nested);
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
        // Every fold lowers to the runtime Fold op, for every binder
        // kind. Literal ranges previously unrolled here into per-term
        // Add nodes, which overflowed the EMIR emitter's recursion at a
        // few hundred terms; the runtime fold is flat, exact, and
        // already cap-checked above.
        self.lower_variable_bound_binder(expr, kind, binder, domain, body, guard)
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
            BinderKind::Series => emath_ir::BinderKind::Series,
        };
        let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &domain.kind
        else {
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
                ExprNode::Literal(Literal::Integer("1".into())),
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
        let domain_id = self.push_expr(ExprNode::Vector(vec![start_id, end_id]), domain.source);
        // Temporarily add the loop variable to inputs so the body can
        // reference it as a Variable (resolved to LoadInput by the EMIR
        // emitter's Binder handler). Hide any unrolled `index_locals`
        // of the same name so an inner runtime fold actually shadows a
        // constant-range outer binder instead of baking the outer value.
        let prev_index = self.index_locals.remove(&binder.name);
        let prev = self.inputs.insert(binder.name.clone(), Infer::Nat);
        let (body_id, body_infer) = match self.lower_expr(body) {
            Some(result) => result,
            None => {
                restore_index_local(&mut self.index_locals, &binder.name, prev_index);
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
                    restore_index_local(&mut self.index_locals, &binder.name, prev_index);
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
                restore_index_local(&mut self.index_locals, &binder.name, prev_index);
                restore_input(&mut self.inputs, &binder.name, prev);
                return None;
            }
            let identity_literal = match kind {
                BinderKind::Sum => Literal::Integer("0".into()),
                BinderKind::Integral => Literal::FloatBits(0.0f64.to_bits()),
                BinderKind::Product => Literal::Integer("1".into()),
                BinderKind::ForAll => Literal::Bool(true),
                BinderKind::Exists => Literal::Bool(false),
                BinderKind::Series => Literal::Bool(true),
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
        restore_index_local(&mut self.index_locals, &binder.name, prev_index);
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
        let return_infer = if is_bool_fold {
            Infer::Bool
        } else {
            Infer::F64
        };
        Some((binder_id, return_infer))
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
        if items
            .iter()
            .all(|item| matches!(&item.kind, ExprKind::List(_)))
        {
            let Some(first) = items.first().and_then(|item| match &item.kind {
                ExprKind::List(row) => Some(row.as_slice()),
                _ => None,
            }) else {
                self.error(
                    "E-SHAPE-004",
                    "matrix literal row must be a list",
                    expr.source,
                );
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
                element: None,
            },
        ))
    }

    /// Table literal (U9): headers live only in the receipt; cells lower
    /// through the matrix element path (numeric gate, uniform extents).
    pub(super) fn lower_table_literal(
        &mut self,
        expr: &Expr,
        headers: &[String],
        rows: &[Vec<Expr>],
    ) -> Option<(ExprId, Infer)> {
        let wrapped: Vec<Expr> = rows
            .iter()
            .map(|row| Expr {
                kind: ExprKind::List(row.clone()),
                source: expr.source,
            })
            .collect();
        self.record(
            "sema",
            format!(
                "table literal `{}` ({} columns x {} rows)",
                headers.join(" "),
                headers.len(),
                rows.len()
            ),
            expr.source,
        );
        self.lower_matrix_literal(expr, &wrapped)
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
                self.error(
                    "E-SHAPE-004",
                    "empty matrix row is not allowed",
                    row_item.source,
                );
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
        // Chain flattening: `m[0][1]` parses as nested single-index
        // expressions, but the runtime value model has no nested-vector
        // value (Vector-of-Vector is row-major like Matrix), so the chain
        // must reach the emitter as ONE multi-index node (2 indices ->
        // MatrixIndex over the row-major store). Only all-point chains
        // flatten; any range slice keeps the literal chain shape.
        let (value, indices): (&Expr, Vec<Expr>) = {
            let mut indices: Vec<Expr> = indices.to_vec();
            let mut cursor: &Expr = value;
            loop {
                let next: Option<&Expr> = match &cursor.kind {
                    ExprKind::Index {
                        value: inner,
                        indices: inner_indices,
                    } => {
                        if indices.len() >= 3 || inner_indices.len() != 1 {
                            None
                        } else {
                            let index = &inner_indices[0];
                            let index_is_range = matches!(&index.kind, ExprKind::Range { .. });
                            if index_is_range {
                                None
                            } else {
                                indices.insert(0, index.clone());
                                Some(&**inner)
                            }
                        }
                    }
                    _ => None,
                };
                match next {
                    Some(next_expr) => cursor = next_expr,
                    None => break,
                }
            }
            (cursor, indices)
        };
        let (target_id, target_infer) = self.lower_expr(value)?;
        let axes = match &target_infer {
            Infer::Vector { extent, element } => match element.as_deref() {
                // Vector-of-vectors indexes like a matrix: one index per
                // nesting level (the runtime value is the row-major
                // matrix store).
                Some(inner @ Infer::Vector { .. }) => {
                    let inner_extent = match inner {
                        Infer::Vector { extent, .. } => extent.clone(),
                        _ => None,
                    };
                    vec![extent.clone(), inner_extent]
                }
                _ => vec![extent.clone()],
            },
            Infer::Sequence => vec![None],
            Infer::Matrix { rows, cols } => vec![rows.clone(), cols.clone()],
            Infer::Tensor { shape } => shape.iter().cloned().map(Some).collect(),
            _ => {
                self.error(
                    "E-TYPE-012",
                    "indexing is only supported on Sequence, Vector, Matrix, and Tensor values",
                    value.source,
                );
                return None;
            }
        };
        if indices.len() != axes.len() {
            // A single index on a vector-of-vectors asks for the row as a
            // VALUE; the Phase 1 runtime has no row value (rows exist only
            // inside the matrix store), so require the full index chain.
            // Depth of the Vector-element chain (2 = Vector<Vector<..>>).
            let mut nest_depth = 0usize;
            if let Infer::Vector {
                element: Some(first),
                ..
            } = &target_infer
            {
                let mut cursor: &Infer = first;
                loop {
                    match cursor {
                        Infer::Vector { element, .. } => {
                            nest_depth += 1;
                            match element {
                                Some(next) => cursor = next,
                                None => break,
                            }
                        }
                        _ => break,
                    }
                }
                nest_depth += 1;
            }
            if nest_depth >= 3 {
                self.error(
                    "E-TYPE-012",
                    "nested vectors deeper than two levels have no Phase 1 runtime value; use Tensor<Float64> with one index per axis",
                    expr.source,
                );
            } else if nest_depth == 2 {
                self.error(
                    "E-SHAPE-006",
                    "indexing a nested vector needs every level at once (`m[i, j]`); row extraction is not a Phase 1 value",
                    expr.source,
                );
            } else {
                self.error(
                    "E-SHAPE-006",
                    format!(
                        "index requires {} subscript(s), found {}",
                        axes.len(),
                        indices.len()
                    ),
                    expr.source,
                );
            }
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
            // Element type of the result: walk the element chain once per
            // point index (Vector<Vector<Float64>> indexed twice yields
            // the Float64 element). Element-blind vectors keep the
            // historical Float64 result.
            let mut result = match &target_infer {
                Infer::Vector { element, .. } => element.clone(),
                _ => None,
            };
            for _ in 1..index_ids.len() {
                result = match result.as_deref() {
                    Some(Infer::Vector { element, .. }) => element.clone(),
                    _ => None,
                };
            }
            let result_infer = result.map_or(Infer::F64, |boxed| *boxed);
            self.record("sema", "scalar index", expr.source);
            let id = self.push_expr(
                ExprNode::Index {
                    value: target_id,
                    indices: index_ids,
                },
                expr.source,
            );
            return Some((id, result_infer));
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
