//! Provider-free semantic type nodes (SIR types).

use crate::shapes::Extent;
use crate::units::{UnitDim, UnitFamily};
use emath_core::QualifiedName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeNode {
    Bool,
    Nat,
    Int,
    Rational,
    /// Real under the selected numeric profile (default `strict-f64`).
    /// Not a claim about real-number arithmetic.
    Float64,
    /// A refinement type `NonNegative<Real>` style; predicate resolved by sema.
    Refinement {
        base: Box<TypeNode>,
        predicate: String,
    },
    Interval(Box<TypeNode>),
    Complex(Box<TypeNode>),
    Set(Box<TypeNode>),
    Vector {
        element: Box<TypeNode>,
        extent: Option<Extent>,
    },
    Matrix {
        element: Box<TypeNode>,
        rows: Option<Extent>,
        cols: Option<Extent>,
    },
    Tensor {
        element: Box<TypeNode>,
        shape: Vec<Extent>,
    },
    Record(QualifiedName),
    Variant(QualifiedName),
    Result {
        ok: Box<TypeNode>,
        error: Box<TypeNode>,
    },
    OptionType(Box<TypeNode>),
    /// A prime field `Field<p>` / `GF<p>`: the integers modulo a fixed
    /// prime `modulus` (emath-option-result-graph-field-aj8d). The prime
    /// is a TYPE-LEVEL constant carried by the type — `GF<7>` is a
    /// distinct node from plain `Int` and from `GF<5>` — so field-typed
    /// declarations keep their modulus instead of collapsing to
    /// `TypeNode::Int`. Values are exact i64 integers; modular reduction
    /// and inversion remain operational concerns of the builtins
    /// (`mod_inv`, `congruence`, `poly_eval_mod`).
    FieldPrime { modulus: i64 },
    Opaque {
        name: QualifiedName,
        provider_contract: Option<emath_core::SchemaId>,
    },
    UnitRef {
        name: String,
        dims: UnitDim,
        family: UnitFamily,
    },
    Other(QualifiedName),
    /// Time-series value type (04 §5.4, emath-r3-timeseries-1nsa):
    /// `Series<Real in s, Real in V>` — the sampled time axis and the
    /// value axis, each carrying their declared unit dimensions. The
    /// interpretation policy rides the VALUE (identity-bearing there),
    /// not the type.
    Series {
        time: Box<TypeNode>,
        value: Box<TypeNode>,
    },
}

fn extent_label(extent: Option<&Extent>) -> String {
    extent.map(ToString::to_string).unwrap_or_default()
}

impl TypeNode {
    /// Human-readable name used in diagnostics and exports.
    #[must_use]
    pub fn display_name(&self) -> String {
        match self {
            Self::Bool => "Bool".to_string(),
            Self::Nat => "Nat".to_string(),
            Self::Int => "Int".to_string(),
            Self::Rational => "Rational".to_string(),
            Self::Float64 => "Float64".to_string(),
            Self::Refinement { base, predicate } => {
                format!("<{} {}>", predicate, base.display_name())
            }
            Self::Interval(inner) => format!("Interval<{}>", inner.display_name()),
            Self::Complex(inner) => format!("Complex<{}>", inner.display_name()),
            Self::Set(inner) => format!("Set<{}>", inner.display_name()),
            Self::Vector { element, extent } => {
                format!(
                    "Vector<{}, {}>",
                    element.display_name(),
                    extent_label(extent.as_ref())
                )
            }
            Self::Matrix {
                element,
                rows,
                cols,
            } => format!(
                "Matrix<{}, {}, {}>",
                element.display_name(),
                extent_label(rows.as_ref()),
                extent_label(cols.as_ref())
            ),
            Self::Tensor { element, shape } => {
                format!(
                    "Tensor<{}, [{}]>",
                    element.display_name(),
                    shape
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::Record(name)
            | Self::Variant(name)
            | Self::Opaque { name, .. }
            | Self::Other(name) => name.0.clone(),
            Self::Series { time, value } => {
                format!(
                    "Series<{}, {}>",
                    time.display_name(),
                    value.display_name()
                )
            }
            Self::Result { ok, error } => {
                format!("Result<{}, {}>", ok.display_name(), error.display_name())
            }
            Self::OptionType(inner) => format!("Option<{}>", inner.display_name()),
            Self::FieldPrime { modulus } => format!("Field<{modulus}>"),
            Self::UnitRef { name, dims, .. } => {
                if name.is_empty() {
                    dims.render()
                } else {
                    name.clone()
                }
            }
        }
    }

    #[must_use]
    pub fn is_scalar_real(&self) -> bool {
        matches!(self, Self::Float64)
    }
}
