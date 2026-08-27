//! The Infer type system — type inference enum, helpers, and numeric
//! combination logic extracted from the admission pass.

use super::Admitter;
use emath_core::tree::Expr;
use emath_ir::{Extent, TypeNode, Unit, UnitDim, UnitFamily, check_compatible};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Infer {
    F64,
    Bool,
    Nat,
    Int,
    Complex,
    Vector {
        extent: Option<Extent>,
    },
    Matrix {
        rows: Option<Extent>,
        cols: Option<Extent>,
    },
    Tensor {
        shape: Vec<Extent>,
    },
    Unit {
        dims: UnitDim,
        family: UnitFamily,
        /// True for an affine *point* (e.g. `degC`). Not part of type
        /// identity: Kelvin and Celsius share temperature dimensions.
        affine: bool,
    },
    /// Whole host-imported record. Not a scalar.
    Opaque,
    /// Host-deferred field access; numeric use is admitted without fabricating a field type.
    HostDeferred,
}

impl Infer {
    fn quantity(dims: UnitDim, family: UnitFamily, affine: bool) -> Self {
        if dims == UnitDim::one() && family == UnitFamily::Si && !affine {
            Self::F64
        } else {
            Self::Unit {
                dims,
                family,
                affine,
            }
        }
    }

    pub(super) fn from_unit(unit: &Unit) -> Self {
        // Information quantities share a dimensionless SI vector but are
        // never SI numbers. Collapse only true SI dimensionless units.
        if unit.is_dimensionless() {
            Self::F64
        } else {
            Self::quantity(unit.dims, unit.family, unit.is_affine())
        }
    }

    /// Result of mul/div: cancelled dimensions are a pure number, even
    /// when both operands were information quantities (`1 MiB / 1 B`).
    pub(super) fn from_derived_unit(unit: &Unit) -> Self {
        if unit.dims == UnitDim::one() {
            Self::F64
        } else {
            Self::quantity(unit.dims, unit.family, false)
        }
    }

    pub(super) fn from_dims(dims: UnitDim, family: UnitFamily) -> Self {
        Self::quantity(dims, family, false)
    }

    pub(super) fn from_dims_affine(dims: UnitDim, family: UnitFamily, affine: bool) -> Self {
        Self::quantity(dims, family, affine)
    }

    fn describe(&self) -> String {
        match self {
            Self::F64 => "Float64".into(),
            Self::Bool => "Bool".into(),
            Self::Nat => "Nat".into(),
            Self::Int => "Int".into(),
            Self::Complex => "Complex".into(),
            Self::Vector { extent } => match extent {
                Some(extent) => format!("Vector[{extent}]"),
                None => "Vector".into(),
            },
            Self::Matrix { rows, cols } => format!(
                "Matrix[{}, {}]",
                rows.as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "?".into()),
                cols.as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "?".into())
            ),
            Self::Tensor { shape } => format!(
                "Tensor[{}]",
                shape
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Unit { dims, family, .. } => {
                if *family == UnitFamily::Information {
                    format!("information ({})", dims.render())
                } else if let Some(kind) = dims.kind_name() {
                    format!("{kind} ({})", dims.render())
                } else {
                    format!("quantity {}", dims.render())
                }
            }
            Self::Opaque => "opaque host value".into(),
            Self::HostDeferred => "host-deferred field".into(),
        }
    }
}

impl std::fmt::Display for Infer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

pub(super) fn is_numeric_element(infer: &Infer) -> bool {
    matches!(
        infer,
        Infer::F64
            | Infer::Nat
            | Infer::Int
            | Infer::Complex
            | Infer::HostDeferred
            | Infer::Unit { .. }
    )
}

pub(super) fn is_index_type(infer: &Infer) -> bool {
    matches!(
        infer,
        Infer::Nat | Infer::Int | Infer::F64 | Infer::HostDeferred
    )
}

pub(super) fn infer_from_shape(shape: Vec<Extent>) -> Infer {
    match shape.len() {
        1 => Infer::Vector {
            extent: shape.into_iter().next(),
        },
        2 => {
            let mut iter = shape.into_iter();
            Infer::Matrix {
                rows: iter.next(),
                cols: iter.next(),
            }
        }
        _ => Infer::Tensor { shape },
    }
}

#[derive(Clone, Copy)]
pub(super) enum NumericCombine {
    Add,
    Sub,
    Mul,
    Div,
}

pub(super) fn infer_from_node(node: &TypeNode) -> Infer {
    match node {
        TypeNode::Bool => Infer::Bool,
        TypeNode::Nat => Infer::Nat,
        TypeNode::Int => Infer::Int,
        TypeNode::Complex(_) => Infer::Complex,
        TypeNode::Vector { extent, .. } => Infer::Vector {
            extent: extent.clone(),
        },
        TypeNode::Matrix { rows, cols, .. } => Infer::Matrix {
            rows: rows.clone(),
            cols: cols.clone(),
        },
        TypeNode::Tensor { shape, .. } => Infer::Tensor {
            shape: shape.clone(),
        },
        TypeNode::UnitRef { dims, family, .. } => Infer::from_dims(*dims, *family),
        TypeNode::Refinement { base, .. } | TypeNode::Interval(base) => infer_from_node(base),
        TypeNode::Opaque { .. } => Infer::Opaque,
        _ => Infer::F64,
    }
}

pub(super) fn extents_compatible(got: Option<&Extent>, declared: Option<&Extent>) -> bool {
    match (got, declared) {
        (Some(got), Some(declared)) => got == declared,
        _ => true,
    }
}

pub(super) fn infer_conforms(got: &Infer, declared: &Infer) -> bool {
    match (got, declared) {
        (Infer::HostDeferred, _) | (_, Infer::HostDeferred) => true,
        (Infer::Vector { extent: got }, Infer::Vector { extent: declared }) => {
            extents_compatible(got.as_ref(), declared.as_ref())
        }
        (
            Infer::Matrix {
                rows: got_rows,
                cols: got_cols,
            },
            Infer::Matrix {
                rows: declared_rows,
                cols: declared_cols,
            },
        ) => {
            extents_compatible(got_rows.as_ref(), declared_rows.as_ref())
                && extents_compatible(got_cols.as_ref(), declared_cols.as_ref())
        }
        (Infer::Tensor { shape: got }, Infer::Tensor { shape: declared }) => got == declared,
        (
            Infer::Unit {
                dims: got,
                family: got_family,
                ..
            },
            Infer::Unit {
                dims: declared,
                family: declared_family,
                ..
            },
        ) => got == declared && got_family == declared_family,
        (Infer::F64, Infer::F64)
        | (Infer::Bool, Infer::Bool)
        | (Infer::Nat, Infer::Nat)
        | (Infer::Int, Infer::Int)
        | (Infer::Complex, Infer::Complex)
        | (Infer::Opaque, Infer::Opaque) => true,
        (Infer::Nat | Infer::Int, Infer::F64) | (Infer::F64, Infer::Nat | Infer::Int) => true,
        (Infer::F64 | Infer::Nat | Infer::Int, Infer::Complex) => true,
        _ => false,
    }
}

pub(super) fn comparable_numeric(left: &Infer, right: &Infer) -> bool {
    match (left, right) {
        (Infer::F64 | Infer::Nat | Infer::Int, Infer::F64 | Infer::Nat | Infer::Int) => true,
        (Infer::Complex, Infer::Complex | Infer::F64 | Infer::Nat | Infer::Int)
        | (Infer::F64 | Infer::Nat | Infer::Int, Infer::Complex) => true,
        (Infer::HostDeferred, Infer::F64)
        | (Infer::F64, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::Unit { .. })
        | (Infer::Unit { .. }, Infer::HostDeferred) => true,
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
                ..
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
                ..
            },
        ) => left_family == right_family && left_dims == right_dims,
        _ => false,
    }
}

pub(super) fn combine_numeric(
    left: &Infer,
    right: &Infer,
    combine: NumericCombine,
    expr: &Expr,
    admitter: &mut Admitter,
) -> Option<Infer> {
    match (left, right, combine) {
        (Infer::Opaque, _, _) | (_, Infer::Opaque, _) => {
            admitter.error(
                "E-TYPE-012",
                "opaque host value is not a scalar; access a field",
                expr.source,
            );
            None
        }
        (Infer::Unit { affine: true, .. }, _, NumericCombine::Mul | NumericCombine::Div)
        | (_, Infer::Unit { affine: true, .. }, NumericCombine::Mul | NumericCombine::Div) => {
            admitter.error(
                "E-UNIT-102",
                "affine unit misuse: cannot multiply or divide an affine quantity",
                expr.source,
            );
            None
        }
        (Infer::HostDeferred, Infer::HostDeferred, _) => Some(Infer::F64),
        (Infer::HostDeferred, Infer::F64, _) | (Infer::F64, Infer::HostDeferred, _) => {
            Some(Infer::F64)
        }
        (Infer::F64 | Infer::Nat | Infer::Int, Infer::F64 | Infer::Nat | Infer::Int, _) => {
            Some(Infer::F64)
        }
        (Infer::Complex, Infer::Complex | Infer::F64 | Infer::Nat | Infer::Int, _)
        | (Infer::F64 | Infer::Nat | Infer::Int, Infer::Complex, _) => Some(Infer::Complex),
        (
            Infer::HostDeferred,
            Infer::Unit {
                dims,
                family,
                affine,
            },
            NumericCombine::Add | NumericCombine::Sub,
        )
        | (
            Infer::Unit {
                dims,
                family,
                affine,
            },
            Infer::HostDeferred,
            NumericCombine::Add | NumericCombine::Sub,
        ) => Some(Infer::Unit {
            dims: *dims,
            family: *family,
            affine: *affine,
        }),
        (Infer::HostDeferred, Infer::Unit { .. }, NumericCombine::Mul | NumericCombine::Div)
        | (Infer::Unit { .. }, Infer::HostDeferred, NumericCombine::Mul | NumericCombine::Div) => {
            Some(Infer::F64)
        }
        (
            Infer::Unit { .. },
            Infer::F64 | Infer::Nat | Infer::Int,
            NumericCombine::Add | NumericCombine::Sub,
        )
        | (
            Infer::F64 | Infer::Nat | Infer::Int,
            Infer::Unit { .. },
            NumericCombine::Add | NumericCombine::Sub,
        ) => {
            admitter.error(
                "E-UNIT-101",
                "dimension mismatch: cannot add a quantity to a dimensionless value",
                expr.source,
            );
            None
        }
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
                affine: left_affine,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
                affine: right_affine,
            },
            combine @ (NumericCombine::Add | NumericCombine::Sub),
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right =
                Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            if let Err(error) = check_compatible(&dummy_left, &dummy_right) {
                admitter.error(error.code, error.message, expr.source);
                return None;
            }
            let result_affine = match (combine, *left_affine, *right_affine) {
                (NumericCombine::Add, true, true) => {
                    admitter.error(
                        "E-UNIT-102",
                        "affine unit misuse: cannot add two affine quantities",
                        expr.source,
                    );
                    return None;
                }
                (NumericCombine::Sub, false, true) => {
                    admitter.error(
                        "E-UNIT-102",
                        "affine unit misuse: cannot subtract an affine point from a linear interval",
                        expr.source,
                    );
                    return None;
                }
                (NumericCombine::Add, left_a, right_a) => left_a || right_a,
                (NumericCombine::Sub, true, true) => false,
                (NumericCombine::Sub, true, false) => true,
                (NumericCombine::Sub, false, false) => false,
                _ => false,
            };
            Some(Infer::quantity(*left_dims, *left_family, result_affine))
        }
        (
            Infer::Unit {
                dims,
                family,
                affine: false,
            },
            Infer::F64 | Infer::Nat | Infer::Int,
            NumericCombine::Mul | NumericCombine::Div,
        )
        | (
            Infer::F64 | Infer::Nat | Infer::Int,
            Infer::Unit {
                dims,
                family,
                affine: false,
            },
            NumericCombine::Mul,
        ) => Some(Infer::quantity(*dims, *family, false)),
        (
            Infer::F64 | Infer::Nat | Infer::Int,
            Infer::Unit {
                dims,
                family,
                affine: false,
            },
            NumericCombine::Div,
        ) => Some(Infer::quantity(UnitDim::one().div(*dims), *family, false)),
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
                affine: false,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
                affine: false,
            },
            NumericCombine::Mul,
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right =
                Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            match dummy_left.mul(&dummy_right) {
                Ok(product) => Some(Infer::from_derived_unit(&product)),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
                affine: false,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
                affine: false,
            },
            NumericCombine::Div,
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right =
                Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            match dummy_left.div(&dummy_right) {
                Ok(quotient) => Some(Infer::from_derived_unit(&quotient)),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        _ => {
            admitter.error(
                "E-TYPE-012",
                "operator requires numeric operands",
                expr.source,
            );
            None
        }
    }
}
