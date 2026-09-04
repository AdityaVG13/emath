//! Shape/extent type mapping and type display.

use super::*;

pub(super) fn map_shape_type(
    leaf: &str,
    generic_args: &[GenericArg],
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
    // `Vector[3]` / `Matrix[2, 2]` treat all args as extents (element defaults
    // to Float64). `Vector[Float64, 3]` / `Matrix[Real, m, n]` name the element
    // first, then the extents.
    //
    // C10: Extents can arrive as either GenericArg::Type (old-style: integers
    // and identifiers parsed as type paths) or GenericArg::Value (new-style:
    // integer literals and expressions). Both are valid extents.
    let (element, extent_args) = match generic_args.first() {
        Some(GenericArg::Type(first)) if is_element_type_arg(first, host_types) => {
            let element = map_type(first, diagnostics, host_types)?;
            (element, generic_args.get(1..).unwrap_or(&[]))
        }
        _ => (TypeNode::Float64, generic_args),
    };
    let mut extents = Vec::new();
    for arg in extent_args {
        match arg {
            GenericArg::Type(ty) => match &ty.kind {
                SynTypeKind::List(items) if items.is_empty() => {
                    diagnostics.error(
                        "E-SHAPE-004",
                        "declared tensor/vector shape must have rank >= 1",
                        ty.source,
                    );
                    return None;
                }
                SynTypeKind::List(items) => {
                    for item in items {
                        extents.push(extent_from_type(item, diagnostics)?);
                    }
                }
                SynTypeKind::Path { segments, .. } => {
                    let name = segments.last().map_or("", String::as_str);
                    extents.push(emath_ir::Extent::from_surface(name));
                }
                _ => {
                    diagnostics.error(
                        "E-SHAPE-004",
                        format!("shape extent `{}` is not well-formed", type_display(ty)),
                        ty.source,
                    );
                    return None;
                }
            },
            GenericArg::Value(expr) => match &expr.kind {
                ExprKind::Int(value) => {
                    extents.push(emath_ir::Extent::from_surface(value));
                }
                ExprKind::Path { segments, .. } => {
                    let name = segments.last().map_or("", String::as_str);
                    extents.push(emath_ir::Extent::from_surface(name));
                }
                ExprKind::List(items) if items.is_empty() => {
                    diagnostics.error(
                        "E-SHAPE-004",
                        "declared tensor/vector shape must have rank >= 1",
                        expr.source,
                    );
                    return None;
                }
                ExprKind::List(items) => {
                    for item in items {
                        extents.push(extent_from_expr(item, diagnostics)?);
                    }
                }
                _ => {
                    diagnostics.error(
                        "E-SHAPE-004",
                        format!(
                            "shape extent `{}` is not a literal or identifier",
                            crate::recognition::expr_text(expr)
                        ),
                        expr.source,
                    );
                    return None;
                }
            },
            GenericArg::Named { .. } => {
                diagnostics.error(
                    "E-SHAPE-004",
                    "named generic arguments are not valid as shape extents",
                    ty.source,
                );
                return None;
            }
        }
    }
    if leaf == "Tensor"
        && extents.is_empty()
        && extent_args.iter().any(
            |arg| matches!(arg, GenericArg::Type(ty) if matches!(ty.kind, SynTypeKind::List(_))),
        )
    {
        return None;
    }
    if !extents.is_empty() {
        if let Err(error) = emath_ir::Shape::declare(extents.clone()) {
            diagnostics.error(error.code, error.message, ty.source);
            return None;
        }
    }
    match leaf {
        "Vector" if extents.len() > 1 => {
            diagnostics.error(
                "E-SHAPE-004",
                format!("`Vector` takes at most one extent, found {}", extents.len()),
                ty.source,
            );
            None
        }
        "Matrix" if !extents.is_empty() && extents.len() != 2 => {
            diagnostics.error(
                "E-SHAPE-004",
                format!(
                    "`Matrix` takes two extents (rows, cols), found {}",
                    extents.len()
                ),
                ty.source,
            );
            None
        }
        "Vector" => Some(TypeNode::Vector {
            element: Box::new(element),
            extent: extents.first().cloned(),
        }),
        "Matrix" => Some(TypeNode::Matrix {
            element: Box::new(element),
            rows: extents.first().cloned(),
            cols: extents.get(1).cloned(),
        }),
        _ => Some(TypeNode::Tensor {
            element: Box::new(element),
            shape: extents,
        }),
    }
}

pub(super) fn extent_from_type(
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
) -> Option<emath_ir::Extent> {
    match &ty.kind {
        SynTypeKind::Path { segments, .. } => {
            let name = segments.last().map_or("", String::as_str);
            Some(emath_ir::Extent::from_surface(name))
        }
        _ => {
            diagnostics.error(
                "E-SHAPE-004",
                format!("shape extent `{}` is not well-formed", type_display(ty)),
                ty.source,
            );
            None
        }
    }
}

pub(super) fn extent_from_expr(
    expr: &emath_core::tree::Expr,
    diagnostics: &mut Diagnostics,
) -> Option<emath_ir::Extent> {
    match &expr.kind {
        ExprKind::Int(value) => Some(emath_ir::Extent::from_surface(value)),
        ExprKind::Path { segments, .. } => {
            let name = segments.last().map_or("", String::as_str);
            Some(emath_ir::Extent::from_surface(name))
        }
        _ => {
            diagnostics.error(
                "E-SHAPE-004",
                format!(
                    "shape extent `{}` is not a literal or identifier",
                    crate::recognition::expr_text(expr)
                ),
                expr.source,
            );
            None
        }
    }
}

pub(in crate::admit) fn type_display(expr: &TypeExpr) -> String {
    match &expr.kind {
        SynTypeKind::Path { segments, .. } => segments.join("::"),
        SynTypeKind::List(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(type_display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        SynTypeKind::Tuple(items) => {
            format!(
                "({})",
                items
                    .iter()
                    .map(type_display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        SynTypeKind::Ref(inner) => format!("&{}", type_display(inner)),
        SynTypeKind::Product { left, op, right } => {
            format!(
                "{}{}{}",
                type_display(left),
                op.as_str(),
                type_display(right)
            )
        }
        SynTypeKind::Pow { base, exponent } => {
            if matches!(base.kind, SynTypeKind::Product { .. }) {
                format!("({})^{exponent}", type_display(base))
            } else {
                format!("{}^{exponent}", type_display(base))
            }
        }
        SynTypeKind::In { base, unit } => {
            format!("{} in {}", type_display(base), type_display(unit))
        }
        SynTypeKind::Domain { base, lo, hi } => {
            format!(
                "{} in [{}, {}]",
                type_display(base),
                crate::recognition::expr_text(lo),
                crate::recognition::expr_text(hi)
            )
        }
    }
}
