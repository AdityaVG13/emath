//! The Infer type system — type inference enum, helpers, and numeric
//! combination logic extracted from the admission pass.

use emath_core::tree::Expr;
use emath_ir::{Extent, TypeNode, Unit, UnitDim, UnitFamily, check_compatible, lookup_unit};
use super::Admitter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Infer {
    F64,
    Bool,
    Nat,
    Int,
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
    Unit { dims: UnitDim, family: UnitFamily },
    /// Whole host-imported record. Not a scalar.
    Opaque,
    /// Host-deferred field access; numeric use is admitted without fabricating a field type.
    HostDeferred,
}

impl Infer {
    pub(super) fn from_unit(unit: &Unit) -> Self {
        if unit.dims == UnitDim::one() {
            Self::F64
        } else {
            Self::Unit {
                dims: unit.dims,
                family: unit.family,
            }
        }
    }

}

pub(super) fn is_numeric_element(infer: &Infer) -> bool {
    matches!(
        infer,
        Infer::F64
            | Infer::Nat
            | Infer::Int
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
    Mul,
    Div,
}

pub(super) fn infer_from_node(node: &TypeNode) -> Infer {
    match node {
        TypeNode::Bool => Infer::Bool,
        TypeNode::Nat => Infer::Nat,
        TypeNode::Int => Infer::Int,
        TypeNode::Vector { extent, .. } => Infer::Vector { extent: extent.clone() },
        TypeNode::Matrix { rows, cols, .. } => Infer::Matrix { rows: rows.clone(), cols: cols.clone() },
        TypeNode::Tensor { shape, .. } => Infer::Tensor { shape: shape.clone() },
        TypeNode::UnitRef { name } => unit_infer_from_name(name),
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
        (Infer::Unit { dims: got, .. }, Infer::Unit { dims: declared, .. }) => got == declared,
        (Infer::F64, Infer::F64)
        | (Infer::Bool, Infer::Bool)
        | (Infer::Nat, Infer::Nat)
        | (Infer::Int, Infer::Int)
        | (Infer::Opaque, Infer::Opaque) => true,
        (Infer::Nat | Infer::Int, Infer::F64) | (Infer::F64, Infer::Nat | Infer::Int) => true,
        _ => false,
    }
}

pub(super) fn unit_infer_from_name(name: &str) -> Infer {
    if let Ok(unit) = lookup_unit(name) {
        return Infer::from_unit(&unit);
    }
    if let Some(inner) = name.strip_prefix("1/") {
        if let Ok(unit) = lookup_unit(inner) {
            return Infer::Unit {
                dims: UnitDim::one().div(unit.dims),
                family: unit.family,
            };
        }
    }
    if name.contains('/') {
        let mut acc: Option<Unit> = None;
        for factor in name.split('/') {
            let Ok(next) = lookup_unit(factor) else {
                return Infer::F64;
            };
            acc = Some(match acc {
                None => next,
                Some(prev) => match prev.div(&next) {
                    Ok(unit) => unit,
                    Err(_) => return Infer::F64,
                },
            });
        }
        if let Some(unit) = acc {
            return Infer::from_unit(&unit);
        }
    }
    Infer::F64
}

pub(super) fn comparable_numeric(left: &Infer, right: &Infer) -> bool {
    match (left, right) {
        (Infer::F64 | Infer::Nat | Infer::Int, Infer::F64 | Infer::Nat | Infer::Int) => true,
        (Infer::HostDeferred, Infer::F64)
        | (Infer::F64, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::HostDeferred)
        | (Infer::HostDeferred, Infer::Unit { .. })
        | (Infer::Unit { .. }, Infer::HostDeferred) => true,
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
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
        (Infer::HostDeferred, Infer::HostDeferred, _) => Some(Infer::F64),
        (Infer::HostDeferred, Infer::F64, _) | (Infer::F64, Infer::HostDeferred, _) => {
            Some(Infer::F64)
        }
        (Infer::F64 | Infer::Nat | Infer::Int, Infer::F64 | Infer::Nat | Infer::Int, _) => {
            Some(Infer::F64)
        }
        (Infer::HostDeferred, Infer::Unit { dims, family }, NumericCombine::Add)
        | (Infer::Unit { dims, family }, Infer::HostDeferred, NumericCombine::Add) => {
            Some(Infer::Unit {
                dims: *dims,
                family: *family,
            })
        }
        (Infer::HostDeferred, Infer::Unit { .. }, NumericCombine::Mul | NumericCombine::Div)
        | (Infer::Unit { .. }, Infer::HostDeferred, NumericCombine::Mul | NumericCombine::Div) => {
            Some(Infer::F64)
        }
        (Infer::Unit { .. }, Infer::F64, NumericCombine::Add)
        | (Infer::F64, Infer::Unit { .. }, NumericCombine::Add) => {
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
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Add,
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right = Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            match check_compatible(&dummy_left, &dummy_right) {
                Ok(()) => Some(left.clone()),
                Err(error) => {
                    admitter.error(error.code, error.message, expr.source);
                    None
                }
            }
        }
        (Infer::Unit { dims, family }, Infer::F64, NumericCombine::Mul | NumericCombine::Div)
        | (Infer::F64, Infer::Unit { dims, family }, NumericCombine::Mul) => {
            Some(Infer::Unit {
                dims: *dims,
                family: *family,
            })
        }
        (Infer::F64, Infer::Unit { dims, family }, NumericCombine::Div) => Some(Infer::Unit {
            dims: UnitDim::one().div(*dims),
            family: *family,
        }),
        (
            Infer::Unit {
                dims: left_dims,
                family: left_family,
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Mul,
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right = Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            match dummy_left.mul(&dummy_right) {
                Ok(product) => Some(Infer::from_unit(&product)),
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
            },
            Infer::Unit {
                dims: right_dims,
                family: right_family,
            },
            NumericCombine::Div,
        ) => {
            let dummy_left = Unit::with_family("left".into(), *left_dims, 1.0, 0.0, *left_family);
            let dummy_right = Unit::with_family("right".into(), *right_dims, 1.0, 0.0, *right_family);
            match dummy_left.div(&dummy_right) {
                Ok(quotient) => Some(Infer::from_unit(&quotient)),
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
