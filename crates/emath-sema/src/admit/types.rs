//! Type mapping functions: surface `TypeExpr` → semantic IR `TypeNode`,
//! unit annotations, and type display.

use emath_core::tree::{ExprKind, GenericArg, TypeExpr, TypeKind as SynTypeKind};
use emath_core::{Diagnostics, QualifiedName, SchemaId};
use emath_ir::{TypeNode, Unit, lookup_unit, per_unit};
use std::collections::BTreeSet;

use super::E_UNSUPPORTED_TYPE;

/// Extract a `TypeExpr` from a `GenericArg::Type`, refusing value/named args
/// in the Phase 1 strict-f64 subset.
fn generic_arg_as_type<'a>(
    arg: &'a GenericArg,
    diagnostics: &mut Diagnostics,
) -> Option<&'a TypeExpr> {
    match arg {
        GenericArg::Type(ty) => Some(ty),
        GenericArg::Value(_) | GenericArg::Named { .. } => {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                "value-level generic arguments are not yet admitted in the Phase 1 strict-f64 subset",
                // GenericArg doesn't carry its own span; use a default.
                emath_core::Span::default(),
            );
            None
        }
    }
}

/// Map a surface type to a neutral type node (Phase 1 subset).
pub(super) fn map_type(
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
    if let SynTypeKind::In { base, unit } = &ty.kind {
        let base_node = map_type(base, diagnostics, host_types)?;
        if !matches!(
            base_node,
            TypeNode::Float64 | TypeNode::Nat | TypeNode::Int | TypeNode::Refinement { .. }
        ) {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "unit annotation applies to a scalar numeric type, not `{}`",
                    type_display(base)
                ),
                ty.source,
            );
            return None;
        }
        return map_unit_annotation(unit, diagnostics);
    }
    if let SynTypeKind::Product(items) = &ty.kind {
        return map_unit_product(items, diagnostics);
    }
    let SynTypeKind::Path {
        segments,
        generic_args,
    } = &ty.kind
    else {
        diagnostics.error(
            E_UNSUPPORTED_TYPE,
            format!(
                "type `{}` is outside the Phase 1 subset (scalar Float64/Real/Bool only)",
                type_display(ty)
            ),
            ty.source,
        );
        return None;
    };
    let leaf = segments.last().map_or("", String::as_str);
    if host_types.contains(leaf) {
        return Some(TypeNode::Opaque {
            name: QualifiedName(leaf.to_string()),
            provider_contract: Some(SchemaId("emath.host.deferred".into())),
        });
    }
    match leaf {
        "Real" | "Float64" | "float64" | "f64" => Some(TypeNode::Float64),
        "Bool" => Some(TypeNode::Bool),
        "Nat" => Some(TypeNode::Nat),
        "Int" => Some(TypeNode::Int),
        "Complex" => Some(TypeNode::Complex(Box::new(TypeNode::Float64))),
        "Self" => Some(TypeNode::Other(QualifiedName("Self".into()))),
        // Mod<p> and GF<p> — integers modulo a prime. Values are exact i64
        // integers; modular reduction is an operational concern in the builtins,
        // not a type-system concern. B15/B29/B40.
        "Mod" | "GF" => Some(TypeNode::Int),
        "NonNegative" | "Positive" | "Probability" => {
            let inner = generic_args
                .first()
                .and_then(|arg| generic_arg_as_type(arg, diagnostics))
                .and_then(|ty| map_type(ty, diagnostics, host_types));
            let base = inner.unwrap_or(TypeNode::Float64);
            Some(TypeNode::Refinement {
                base: Box::new(base),
                predicate: leaf.to_string(),
            })
        }
        "Per" => {
            if generic_args.len() != 1 {
                diagnostics.error(
                    "E-UNIT-105",
                    "`Per<U>` requires exactly one inner unit",
                    ty.source,
                );
                return None;
            }
            let inner_ty = generic_arg_as_type(&generic_args[0], diagnostics);
            let inner_name = match inner_ty {
                Some(ty) => {
                    if let SynTypeKind::Path { segments, .. } = &ty.kind {
                        segments.last().map_or("", String::as_str)
                    } else {
                        diagnostics.error(
                            "E-UNIT-105",
                            "`Per<U>` inner argument must be a unit type",
                            ty.source,
                        );
                        return None;
                    }
                }
                None => return None,
            };
            match per_unit(inner_name) {
                Ok(unit) => Some(TypeNode::UnitRef { name: unit.name }),
                Err(error) => {
                    diagnostics.error(error.code, error.message, ty.source);
                    None
                }
            }
        }
        "Interval" => {
            let inner = generic_args
                .first()
                .and_then(|arg| generic_arg_as_type(arg, diagnostics))
                .and_then(|ty| map_type(ty, diagnostics, host_types))
                .unwrap_or(TypeNode::Float64);
            Some(TypeNode::Interval(Box::new(inner)))
        }
        "Vector" | "Matrix" | "Tensor" => {
            map_shape_type(leaf, generic_args, ty, diagnostics, host_types)
        }
        "Result" => {
            let error_name = generic_args
                .get(1)
                .and_then(|arg| generic_arg_as_type(arg, diagnostics))
                .map_or_else(|| "ConfigError".to_string(), type_display);
            Some(TypeNode::Other(QualifiedName(error_name)))
        }
        "Option"
        | "Sequence"
        | "Set"
        | "Array"
        | "Field"
        | "DirectedGraph"
        | "SearchResult"
        | "RecoveryCertificate"
        | "RequestProfile"
        | "ArtifactId"
        | "NodeId"
        | "CacheCandidate"
        | "Route"
        | "Witness"
        | "Rational" => {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                format!("type `{leaf}` is outside the Phase 1 subset"),
                ty.source,
            );
            None
        }
        other => match lookup_unit(other) {
            Ok(unit) => Some(TypeNode::UnitRef { name: unit.name }),
            Err(error) if error.code == "E-UNIT-104" => {
                diagnostics.error("E-TYPE-001", format!("unknown type `{other}`"), ty.source);
                None
            }
            Err(error) => {
                diagnostics.error(error.code, error.message, ty.source);
                None
            }
        },
    }
}

pub(super) fn map_unit_annotation(unit: &TypeExpr, diagnostics: &mut Diagnostics) -> Option<TypeNode> {
    match lookup_unit_type(unit) {
        Ok(looked_up) => Some(TypeNode::UnitRef {
            name: looked_up.name,
        }),
        Err(error) => {
            diagnostics.error(error.code, error.message, unit.source);
            None
        }
    }
}

pub(super) fn map_unit_product(
    items: &[TypeExpr],
    diagnostics: &mut Diagnostics,
) -> Option<TypeNode> {
    match lookup_unit_product(items) {
        Ok(unit) => Some(TypeNode::UnitRef { name: unit.name }),
        Err(error) => {
            diagnostics.error(
                error.code,
                error.message,
                items.first().map(|item| item.source).unwrap_or_default(),
            );
            None
        }
    }
}

pub(super) fn lookup_unit_type(ty: &TypeExpr) -> Result<Unit, emath_ir::UnitError> {
    match &ty.kind {
        SynTypeKind::Path { segments, .. } => {
            let name = segments.last().map_or("", String::as_str);
            lookup_unit(name)
        }
        SynTypeKind::Product(items) => lookup_unit_product(items),
        _ => Err(emath_ir::UnitError {
            code: "E-UNIT-105",
            message: format!("unit `{}` is not well-formed", type_display(ty)),
        }),
    }
}

pub(super) fn lookup_unit_product(items: &[TypeExpr]) -> Result<Unit, emath_ir::UnitError> {
    if items.len() < 2 {
        return Err(emath_ir::UnitError {
            code: "E-UNIT-105",
            message: "unit product needs at least two factors".into(),
        });
    }
    let mut acc = lookup_unit_type(&items[0])?;
    for item in &items[1..] {
        let next = lookup_unit_type(item)?;
        acc = acc.div(&next)?;
    }
    Ok(acc)
}

pub(super) fn is_element_type_arg(arg: &TypeExpr, host_types: &BTreeSet<String>) -> bool {
    let SynTypeKind::Path { segments, .. } = &arg.kind else {
        return false;
    };
    let leaf = segments.last().map_or("", String::as_str);
    if host_types.contains(leaf) {
        return true;
    }
    matches!(
        leaf,
        "Real"
            | "Float64"
            | "float64"
            | "f64"
            | "Bool"
            | "Self"
            | "NonNegative"
            | "Positive"
            | "Probability"
            | "Per"
            | "Interval"
            | "Complex"
            | "Vector"
            | "Matrix"
            | "Tensor"
    ) || lookup_unit(leaf).is_ok()
}

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
                _ => {
                    diagnostics.error(
                        "E-SHAPE-004",
                        format!("shape extent `{}` is not a literal or identifier", crate::recognition::expr_text(expr)),
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
    if leaf == "Tensor" && extents.is_empty() && extent_args.iter().any(|arg| {
        matches!(arg, GenericArg::Type(ty) if matches!(ty.kind, SynTypeKind::List(_)))
    }) {
        return None;
    }
    if !extents.is_empty() {
        if let Err(error) = emath_ir::Shape::declare(extents.clone()) {
            diagnostics.error(error.code, error.message, ty.source);
            return None;
        }
    }
    match leaf {
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

pub(super) fn type_display(expr: &TypeExpr) -> String {
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
        SynTypeKind::Product(items) => format!(
            "({})",
            items
                .iter()
                .map(type_display)
                .collect::<Vec<_>>()
                .join(" * ")
        ),
        SynTypeKind::In { base, unit } => {
            format!("{} in {}", type_display(base), type_display(unit))
        }
    }
}
