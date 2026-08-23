//! Type mapping functions: surface `TypeExpr` → semantic IR `TypeNode`,
//! unit annotations, and type display.

use emath_core::tree::{TypeExpr, TypeKind as SynTypeKind};
use emath_core::{Diagnostics, QualifiedName, SchemaId};
use emath_ir::{TypeNode, Unit, lookup_unit, per_unit};
use std::collections::BTreeSet;

use super::E_UNSUPPORTED_TYPE;

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
        "Self" => Some(TypeNode::Other(QualifiedName("Self".into()))),
        "NonNegative" | "Positive" | "Probability" => {
            let inner = generic_args
                .first()
                .and_then(|arg| map_type(arg, diagnostics, host_types));
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
            let inner_name = match &generic_args[0].kind {
                SynTypeKind::Path { segments, .. } => segments.last().map_or("", String::as_str),
                _ => {
                    diagnostics.error(
                        "E-UNIT-105",
                        "`Per<U>` inner argument must be a unit type",
                        generic_args[0].source,
                    );
                    return None;
                }
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
                .and_then(|arg| map_type(arg, diagnostics, host_types))
                .unwrap_or(TypeNode::Float64);
            Some(TypeNode::Interval(Box::new(inner)))
        }
        "Vector" | "Matrix" | "Tensor" => {
            map_shape_type(leaf, generic_args, ty, diagnostics, host_types)
        }
        "Result" => {
            let error_name = generic_args
                .get(1)
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
            | "Vector"
            | "Matrix"
            | "Tensor"
    ) || lookup_unit(leaf).is_ok()
}

pub(super) fn map_shape_type(
    leaf: &str,
    generic_args: &[TypeExpr],
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
    // `Vector[3]` / `Matrix[2, 2]` treat all args as extents (element defaults
    // to Float64). `Vector[Float64, 3]` / `Matrix[Real, m, n]` name the element
    // first, then the extents.
    let (element, extent_args) = match generic_args.first() {
        Some(first) if is_element_type_arg(first, host_types) => {
            let element = map_type(first, diagnostics, host_types)?;
            (element, generic_args.get(1..).unwrap_or(&[]))
        }
        _ => (TypeNode::Float64, generic_args),
    };
    let mut extents = Vec::new();
    for arg in extent_args {
        match &arg.kind {
            SynTypeKind::List(items) if items.is_empty() => {
                diagnostics.error(
                    "E-SHAPE-004",
                    "declared tensor/vector shape must have rank >= 1",
                    arg.source,
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
                    format!("shape extent `{}` is not well-formed", type_display(arg)),
                    arg.source,
                );
                return None;
            }
        }
    }
    if leaf == "Tensor" && extents.is_empty() && extent_args.iter().any(|arg| {
        matches!(arg.kind, SynTypeKind::List(_))
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
