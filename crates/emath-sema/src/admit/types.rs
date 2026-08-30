//! Type mapping functions: surface `TypeExpr` → semantic IR `TypeNode`,
//! unit annotations, and type display.

use emath_core::tree::{ExprKind, GenericArg, TypeExpr, TypeKind as SynTypeKind, TypeProductOp};
use emath_core::{Diagnostics, QualifiedName, SchemaId};
use emath_ir::{TypeNode, Unit, lookup_unit, per_unit};
use std::collections::BTreeSet;

use super::E_UNSUPPORTED_TYPE;

/// Largest admitted prime-field modulus: i32::MAX. Field values are exact
/// i64 and the interpreter's modular kernels (extended gcd) run on i64;
/// capping the TYPE-LEVEL prime at i32::MAX keeps the field in the exact
/// i64 square (p² < 2^62) while staying representable in i32-lane kernels
/// (emath-option-result-graph-field-aj8d).
const FIELD_PRIME_MAX: i64 = i32::MAX as i64;

/// Trial division: whether `p` is a prime (p ≥ 2). Within the admitted
/// i32::MAX cap, `d * d` never overflows i64.
fn is_prime(p: i64) -> bool {
    if p < 2 {
        return false;
    }
    if p % 2 == 0 {
        return p == 2;
    }
    let mut d = 3i64;
    while d * d <= p {
        if p % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

/// How a `Field<p>`/`GF<p>` generic argument reads, so the refusal names
/// the exact constraint (emath-option-result-graph-field-aj8d pass 8):
/// a plain integer literal carries a candidate modulus; an integral
/// literal too large for i64 is refused for range (never mis-typed as
/// "not a literal"); a type arg (`GF<Int>`) or a computed value arg
/// (`GF<n>`, `GF<7 + 1>`) is refused for the type-level-literal rule.
enum FieldModulusArg {
    /// An integral literal that fits i64 and names a candidate modulus.
    Literal(i64),
    /// An integral literal too large to fit i64 — cannot satisfy the
    /// `2 ≤ p ≤ i32::MAX` bound no matter its value.
    LiteralOverflow,
    /// A type argument or a non-integer value argument.
    NotLiteral,
}

/// Read a `Field<p>` generic argument; never panics, always classifies.
fn field_modulus_arg(arg: &GenericArg) -> FieldModulusArg {
    let GenericArg::Value(expr) = arg else {
        return FieldModulusArg::NotLiteral;
    };
    if let ExprKind::Int(text) = &expr.kind {
        return parse_int_literal(text, false);
    }
    FieldModulusArg::NotLiteral
}

fn parse_int_literal(text: &str, _neg: bool) -> FieldModulusArg {
    match text.parse::<i64>() {
        Ok(value) => FieldModulusArg::Literal(value),
        Err(_) => FieldModulusArg::LiteralOverflow,
    }
}

/// Text of a numeric literal for a domain predicate string; falls back to
/// `expr_text` for complex expressions.
fn expr_literal_text(expr: &emath_core::tree::Expr) -> String {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => text.clone(),
        ExprKind::Unary {
            op: emath_core::tree::UnaryOp::Neg,
            value,
        } => match &value.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => format!("-{text}"),
            _ => super::super::recognition::expr_text(expr),
        },
        _ => super::super::recognition::expr_text(expr),
    }
}

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
    // U5: Domain annotation `Float64 in [lo, hi]` - map to a refinement
    // that carries the bounds as a predicate string.
    if let SynTypeKind::Domain { base, lo, hi } = &ty.kind {
        let base_node = map_type(base, diagnostics, host_types)?;
        if !matches!(
            base_node,
            TypeNode::Float64 | TypeNode::Nat | TypeNode::Int | TypeNode::Refinement { .. }
        ) {
            diagnostics.error(
                "E-TYPE-001",
                format!(
                    "domain annotation applies to a scalar numeric type, not `{}`",
                    type_display(base)
                ),
                ty.source,
            );
            return None;
        }
        // Extract numeric literal bounds for the predicate.
        let lo_text = expr_literal_text(lo);
        let hi_text = expr_literal_text(hi);
        let predicate = format!("domain[{lo_text},{hi_text}]");
        return Some(TypeNode::Refinement {
            base: Box::new(base_node),
            predicate,
        });
    }
    if matches!(
        &ty.kind,
        SynTypeKind::Product { .. } | SynTypeKind::Pow { .. }
    ) {
        return map_unit_annotation(ty, diagnostics);
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
        "Float64" => Some(TypeNode::Float64),
        "Bool" => Some(TypeNode::Bool),
        "Nat" => Some(TypeNode::Nat),
        "Int" => Some(TypeNode::Int),
        "Complex" => Some(TypeNode::Complex(Box::new(TypeNode::Float64))),
        "Self" => Some(TypeNode::Other(QualifiedName("Self".into()))),
        // Field<p> / GF<p> — prime fields (emath-option-result-graph-field-aj8d).
        // The PRIME is a TYPE-LEVEL constant: the declared modulus
        // distinguishes the type (GF<7> ≠ Int ≠ GF<5>), fixing the
        // earlier silent `"GF" => TypeNode::Int` collapse that dropped
        // the prime (RED: aj8d_gf_prime_is_distinct_type). Values are
        // exact i64; the type admits exactly ONE PRIME INTEGER LITERAL
        // argument and refuses anything else with an E-TYPE-010 message
        // naming the spelling and the constraint.
        "Field" | "GF" => {
            if generic_args.len() != 1 {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`{leaf}<p>` requires exactly one prime integer type argument; got {}",
                        generic_args.len()
                    ),
                    ty.source,
                );
                return None;
            }
            let modulus = match field_modulus_arg(&generic_args[0]) {
                FieldModulusArg::Literal(modulus) => modulus,
                FieldModulusArg::LiteralOverflow => {
                    let spelling = match &generic_args[0] {
                        GenericArg::Value(expr) => expr_literal_text(expr),
                        _ => type_display(ty),
                    };
                    diagnostics.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "`{leaf}<p>` prime integer LITERAL `{spelling}` exceeds the \
                             maximum supported field modulus (i32::MAX = {FIELD_PRIME_MAX}); \
                             the field prime is a type-level constant"
                        ),
                        ty.source,
                    );
                    return None;
                }
                FieldModulusArg::NotLiteral => {
                    diagnostics.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "`{leaf}<p>` requires a prime integer LITERAL modulus (the field \
                             prime is a type-level constant); got `{}`",
                            type_display(ty)
                        ),
                        ty.source,
                    );
                    return None;
                }
            };
            if modulus < 2 || modulus > FIELD_PRIME_MAX || !is_prime(modulus) {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`{leaf}<{modulus}>` requires a prime modulus 2 ≤ p ≤ {FIELD_PRIME_MAX}; \
                         got {modulus}"
                    ),
                    ty.source,
                );
                return None;
            }
            Some(TypeNode::FieldPrime { modulus })
        }
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
                Ok(unit) => Some(unit_ref_node(unit)),
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
        "Set" => {
            let inner = generic_args
                .first()
                .and_then(|arg| generic_arg_as_type(arg, diagnostics))
                .and_then(|ty| map_type(ty, diagnostics, host_types))
                .unwrap_or(TypeNode::Float64);
            Some(TypeNode::Set(Box::new(inner)))
        }
        "Vector" | "Matrix" | "Tensor" => {
            map_shape_type(leaf, generic_args, ty, diagnostics, host_types)
        }
        "Series" => {
            // 04 §5.4 (emath-r3-timeseries-1nsa): `Series<T in tunit,
            // U in vunit>` — exactly two unit-annotated numeric type
            // arguments. The pair literal + declared policy is the
            // VALUE (admitted in lowering); pure CSV-text projection
            // and series evaluation use the same identity-bearing value.
            if generic_args.len() != 2 {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`Series<...>` requires exactly two type arguments (`Series<T in time_unit, U in value_unit>`); got {}",
                        generic_args.len()
                    ),
                    ty.source,
                );
                return None;
            }
            let mut mapped: Option<(TypeNode, TypeNode)> =
                Some((TypeNode::Float64, TypeNode::Float64));
            for (label, index) in [("time", 0_usize), ("value", 1_usize)] {
                let Some(arg_ty) = generic_arg_as_type(&generic_args[index], diagnostics) else {
                    mapped = None;
                    break;
                };
                let SynTypeKind::In { base, .. } = &arg_ty.kind else {
                    diagnostics.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "`Series<...>` {label} argument must carry a unit annotation (`Real in s`); a bare type names no measured dimension"
                        ),
                        arg_ty.source,
                    );
                    mapped = None;
                    break;
                };
                let base_node = map_type(base, diagnostics, host_types);
                match (base_node, mapped.as_mut()) {
                    (Some(node), Some(slot)) => {
                        if index == 0 {
                            slot.0 = node;
                        } else {
                            slot.1 = node;
                        }
                    }
                    _ => {
                        mapped = None;
                        break;
                    }
                }
            }
            mapped.map(|(time, value)| TypeNode::Series {
                time: Box::new(time),
                value: Box::new(value),
            })
        }
        "Option" => {
            if generic_args.len() != 1 {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`Option<T>` requires exactly one type argument; got {}",
                        generic_args.len()
                    ),
                    ty.source,
                );
                return None;
            }
            let inner = generic_arg_as_type(&generic_args[0], diagnostics)
                .and_then(|arg| map_type(arg, diagnostics, host_types))?;
            Some(TypeNode::OptionType(Box::new(inner)))
        }
        "Result" => {
            if generic_args.len() != 2 {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`Result<T, E>` requires exactly two type arguments; got {}",
                        generic_args.len()
                    ),
                    ty.source,
                );
                return None;
            }
            let ok = generic_arg_as_type(&generic_args[0], diagnostics)
                .and_then(|arg| map_type(arg, diagnostics, host_types))?;
            let error = generic_arg_as_type(&generic_args[1], diagnostics)
                .and_then(|arg| map_type(arg, diagnostics, host_types))?;
            Some(TypeNode::Result {
                ok: Box::new(ok),
                error: Box::new(error),
            })
        }
        "Graph" => {
            // Graph is an ALIAS for the dense `Matrix<Float64>` adjacency
            // carrier (emath-option-result-graph-field-aj8d, decision b).
            // The graph ops check SHAPES (ParamShape::Matrix), not the
            // TypeNode, so mapping the spelling onto the existing matrix
            // surface makes graph-typed declarations compute with zero
            // downstream changes. Bare `Graph` only; any generic count is
            // a typed arity refusal (pass 8 contract).
            if !generic_args.is_empty() {
                diagnostics.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "`Graph` admits no type arguments (dense `Matrix<Float64>` adjacency carrier); got {}",
                        generic_args.len()
                    ),
                    ty.source,
                );
                None
            } else {
                Some(TypeNode::Matrix {
                    element: Box::new(TypeNode::Float64),
                    rows: None,
                    cols: None,
                })
            }
        }
        // Total refusal matrix (pass 5, emath-rat-real-types-p5cj): bare
        // `Real` at a type site is NEVER silently mapped to `f64`. The one
        // deterministic E-NUM-004 names the three sanctioned spellings, so
        // bare input, generic arguments, and Vector/Matrix element positions
        // all refuse identically (no shape-dependent behavior).
        "Real" => {
            diagnostics.error(
                "E-NUM-004",
                "bare `Real` at a type site requires profile evidence; write \
                 `Float64` (strict-f64), `Interval<Float64>` (certified interval), \
                 or a `representation Real => Float64` directive",
                ty.source,
            );
            None
        }
        // Pass 2 (emath-rat-real-types-p5cj): `Rat`/`Rational` map onto the
        // existing `TypeNode::Rational` (exact i128 num/den) instead of the
        // Phase 1 refusal.
        "Rat" | "Rational" => Some(TypeNode::Rational),
        "Sequence"
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
        | "Witness" => {
            diagnostics.error(
                E_UNSUPPORTED_TYPE,
                format!("type `{leaf}` is outside the Phase 1 subset"),
                ty.source,
            );
            None
        }
        other => match lookup_unit(other) {
            Ok(unit) => Some(unit_ref_node(unit)),
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

fn unit_ref_node(unit: Unit) -> TypeNode {
    TypeNode::UnitRef {
        name: unit.name,
        dims: unit.dims,
        family: unit.family,
    }
}

pub(super) fn map_unit_annotation(
    unit: &TypeExpr,
    diagnostics: &mut Diagnostics,
) -> Option<TypeNode> {
    match lookup_unit_type(unit) {
        Ok(looked_up) => Some(unit_ref_node(looked_up)),
        Err(error) => {
            diagnostics.error(error.code, error.message, unit.source);
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
        SynTypeKind::Product { left, op, right } => {
            let left_unit = lookup_unit_type(left)?;
            let right_unit = lookup_unit_type(right)?;
            match op {
                TypeProductOp::Mul => left_unit.mul(&right_unit),
                TypeProductOp::Div => left_unit.div(&right_unit),
            }
        }
        SynTypeKind::Pow { base, exponent } => lookup_unit_type(base)?.pow(*exponent),
        SynTypeKind::Tuple(items) if items.len() == 1 => lookup_unit_type(&items[0]),
        _ => Err(emath_ir::UnitError {
            code: "E-UNIT-105",
            message: format!("unit `{}` is not well-formed", type_display(ty)),
        }),
    }
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
            | "Nat"
            | "Int"
            | "GF"
            | "Self"
            | "NonNegative"
            | "Positive"
            | "Probability"
            | "Per"
            | "Interval"
            | "Set"
            | "Complex"
            | "Vector"
            | "Matrix"
            | "Tensor"
            | "Option"
            | "Result"
            | "Graph"
            | "Field"
            | "Rat"
            | "Rational"
    ) || lookup_unit(leaf).is_ok()
}

/// Constructor return `Result<Self, E>` is error-type sugar, not a compute
/// type. Compute-site `Result` is refused by `map_type`.
pub(super) fn map_constructor_return(
    ty: &TypeExpr,
    diagnostics: &mut Diagnostics,
    host_types: &BTreeSet<String>,
) -> Option<TypeNode> {
    if let SynTypeKind::Path {
        segments,
        generic_args,
    } = &ty.kind
    {
        if segments.last().map(String::as_str) == Some("Result") {
            let error_name = generic_args
                .get(1)
                .and_then(|arg| generic_arg_as_type(arg, diagnostics))
                .map_or_else(|| "ConfigError".to_string(), type_display);
            return Some(TypeNode::Other(QualifiedName(error_name)));
        }
    }
    map_type(ty, diagnostics, host_types)
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

fn extent_from_expr(
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
                super::super::recognition::expr_text(lo),
                super::super::recognition::expr_text(hi)
            )
        }
    }
}
