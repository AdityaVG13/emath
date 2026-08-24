//! Expression lowering helpers: indexing, tensor literals, broadcasting,
//! and diagnostics extracted from `admit.rs` isomorphically.

use emath_core::tree::{Expr, ExprKind};
use emath_ir::{ExprId, ExprNode, Extent, Literal};

use super::infer::{is_index_type, is_numeric_element, Infer};
use super::{expr_number, Admitter};

pub(super) enum IndexAxis {
    Point(ExprId),
    Slice {
        start: ExprId,
        end: ExprId,
        extent: Extent,
    },
}

pub(super) fn reduction_coords(infer: &Infer) -> Option<Vec<Vec<usize>>> {
    match infer {
        Infer::Vector {
            extent: Some(Extent::Fixed(len)),
        } => Some((0..*len).map(|index| vec![index]).collect()),
        Infer::Matrix {
            rows: Some(Extent::Fixed(rows)),
            cols: Some(Extent::Fixed(cols)),
        } => {
            let mut coords = Vec::with_capacity(rows * cols);
            for row in 0..*rows {
                for col in 0..*cols {
                    coords.push(vec![row, col]);
                }
            }
            Some(coords)
        }
        Infer::Tensor { shape } => {
            let mut dims = Vec::with_capacity(shape.len());
            for extent in shape {
                match extent {
                    Extent::Fixed(len) => dims.push(*len),
                    Extent::Symbolic(_) => return None,
                }
            }
            Some(cartesian_coords(&dims))
        }
        _ => None,
    }
}

pub(super) fn cartesian_coords(dims: &[usize]) -> Vec<Vec<usize>> {
    if dims.is_empty() {
        return vec![Vec::new()];
    }
    let mut coords = vec![Vec::new()];
    for &dim in dims {
        let mut next = Vec::with_capacity(coords.len() * dim);
        for prefix in coords {
            for index in 0..dim {
                let mut coord = prefix.clone();
                coord.push(index);
                next.push(coord);
            }
        }
        coords = next;
    }
    coords
}

pub(super) fn collect_tensor_literal(
    admitter: &mut Admitter,
    items: &[Expr],
    depth: usize,
    shape: &mut Vec<usize>,
    elements: &mut Vec<ExprId>,
) -> Option<()> {
    if items.is_empty() {
        admitter.error(
            "E-SHAPE-004",
            "empty tensor axis is not allowed",
            items
                .first()
                .map(|item| item.source)
                .unwrap_or_default(),
        );
        return None;
    }
    if shape.len() == depth {
        shape.push(items.len());
    } else if shape[depth] != items.len() {
        admitter.error(
            "E-SHAPE-005",
            format!(
                "tensor axis {depth} must have uniform extent {}, found {}",
                shape[depth],
                items.len()
            ),
            items[0].source,
        );
        return None;
    }
    let nested = items.iter().all(|item| matches!(&item.kind, ExprKind::List(_)));
    if nested {
        for item in items {
            let ExprKind::List(inner) = &item.kind else {
                admitter.error(
                    "E-SHAPE-004",
                    "tensor axis entry must be a list literal",
                    item.source,
                );
                return None;
            };
            collect_tensor_literal(admitter, inner, depth + 1, shape, elements)?;
        }
        return Some(());
    }
    for item in items {
        if matches!(&item.kind, ExprKind::List(_)) {
            admitter.error(
                "E-SHAPE-005",
                "ragged tensor literal is not allowed",
                item.source,
            );
            return None;
        }
        let (id, infer) = admitter.lower_expr(item)?;
        if !is_numeric_element(&infer) {
            admitter.error("E-TYPE-012", "tensor element must be numeric", item.source);
            return None;
        }
        elements.push(id);
    }
    Some(())
}

pub(super) fn lower_index_axis(
    admitter: &mut Admitter,
    index: &Expr,
    extent: Option<&Extent>,
    axis: usize,
) -> Option<IndexAxis> {
    if let ExprKind::Slice { start, end } = &index.kind {
        let start_id = match start {
            Some(start) => {
                refuse_negative_constant_index(admitter, start)?;
                let (id, infer) = admitter.lower_expr(start)?;
                if !is_index_type(&infer) {
                    admitter.error(
                        "E-SHAPE-006",
                        "slice start must be a Nat, non-negative Int, or Float64 whole number",
                        start.source,
                    );
                    return None;
                }
                id
            }
            None => admitter.push_expr(
                ExprNode::Literal(Literal::Integer("0".into())),
                index.source,
            ),
        };
        let end_id = match end {
            Some(end) => {
                refuse_negative_constant_index(admitter, end)?;
                let (id, infer) = admitter.lower_expr(end)?;
                if !is_index_type(&infer) {
                    admitter.error(
                        "E-SHAPE-006",
                        "slice end must be a Nat, non-negative Int, or Float64 whole number",
                        end.source,
                    );
                    return None;
                }
                id
            }
            None => match extent {
                Some(Extent::Fixed(size)) => admitter.push_expr(
                    ExprNode::Literal(Literal::Integer(size.to_string())),
                    index.source,
                ),
                _ => {
                    admitter.error(
                        "E-SHAPE-006",
                        format!("open slice on axis {axis} needs a fixed extent"),
                        index.source,
                    );
                    return None;
                }
            },
        };
        let slice_extent = match (
            start
                .as_ref()
                .and_then(|expr| expr_number(expr))
                .or(start.is_none().then_some(0.0)),
            end.as_ref().and_then(|expr| expr_number(expr)).or_else(|| {
                end.is_none()
                    .then(|| match extent {
                        Some(Extent::Fixed(size)) => Some(*size as f64),
                        _ => None,
                    })
                    .flatten()
            }),
        ) {
            (Some(start), Some(end)) if start.is_finite() && end.is_finite() && end >= start => {
                Extent::Fixed((end - start) as usize)
            }
            _ => Extent::Symbolic(format!("slice{axis}")),
        };
        return Some(IndexAxis::Slice {
            start: start_id,
            end: end_id,
            extent: slice_extent,
        });
    }
    refuse_negative_constant_index(admitter, index)?;
    let (id, infer) = admitter.lower_expr(index)?;
    if !is_index_type(&infer) {
        admitter.error(
            "E-SHAPE-006",
            "index must be a Nat, non-negative Int, or Float64 whole number",
            index.source,
        );
        return None;
    }
    Some(IndexAxis::Point(id))
}

pub(super) fn broadcast_tensor_shapes(
    admitter: &mut Admitter,
    left: &[Extent],
    right: &[Extent],
    expr: &Expr,
) -> Option<Vec<Extent>> {
    if left.len() != right.len() {
        admitter.error(
            "E-SHAPE-005",
            format!(
                "tensor rank mismatch in elementwise op: {} vs {}",
                left.len(),
                right.len()
            ),
            expr.source,
        );
        return None;
    }
    let mut out = Vec::with_capacity(left.len());
    for (lhs, rhs) in left.iter().zip(right) {
        match (lhs, rhs) {
            (a, b) if a == b => out.push(a.clone()),
            (Extent::Fixed(1), other) | (other, Extent::Fixed(1)) => out.push(other.clone()),
            _ => {
                admitter.error(
                    "E-SHAPE-005",
                    format!("tensor broadcast mismatch: {lhs} vs {rhs}"),
                    expr.source,
                );
                return None;
            }
        }
    }
    Some(out)
}

pub(super) fn refuse_negative_constant_index(admitter: &mut Admitter, expr: &Expr) -> Option<()> {
    if let Some(value) = expr_number(expr) {
        if value < 0.0 {
            admitter.error(
                "E-SHAPE-006",
                "constant index must be non-negative",
                expr.source,
            );
            return None;
        }
    }
    Some(())
}

pub(super) fn parse_float_constant(text: &str) -> Option<f64> {
    // strip float suffix (`1e-12f32` → `1e-12`)
    let mut cleaned = text.to_string();
    for suffix in ["bf16", "f16", "f32", "f64", "f128"] {
        if let Some(stripped) = cleaned.strip_suffix(suffix) {
            cleaned = stripped.to_string();
            break;
        }
    }
    cleaned.replace('_', "").parse().ok()
}

pub(super) fn expr_form_name(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Int(_) => "integer",
        ExprKind::Float(_) => "float",
        ExprKind::Str(_) => "string",
        ExprKind::Bool(_) => "bool",
        ExprKind::Quantity { .. } => "quantity",
        ExprKind::Path { .. } => "path",
        ExprKind::Call { .. } => "call",
        ExprKind::Index { .. } => "index",
        ExprKind::Slice { .. } => "slice",
        ExprKind::Unary { .. } => "unary",
        ExprKind::Binary { .. } => "binary",
        ExprKind::If { .. } => "if",
        ExprKind::List(_) => "list",
        ExprKind::Tuple(_) => "tuple",
        ExprKind::Range { .. } => "range",
        ExprKind::Binder { .. } => "binder",
        ExprKind::Derivative { .. } => "derivative",
        ExprKind::Solve { .. } => "solve",
        ExprKind::Optimize { .. } => "optimize",
        ExprKind::At { .. } => "at",
        ExprKind::On { .. } => "on",
        ExprKind::Conditioned { .. } => "conditioned",
        ExprKind::UnitQuery { .. } => "unit-query",
        ExprKind::Limit { .. } => "limit",
        ExprKind::SampleLimit { .. } => "sample-limit",
        ExprKind::Cases { .. } => "cases",
    }
}
