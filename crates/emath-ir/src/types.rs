//! Provider-free semantic type nodes (SIR types).

use crate::shapes::Extent;
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
    Opaque {
        name: QualifiedName,
        provider_contract: Option<emath_core::SchemaId>,
    },
    UnitRef {
        name: String,
    },
    Other(QualifiedName),
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
            Self::Result { ok, error } => {
                format!("Result<{}, {}>", ok.display_name(), error.display_name())
            }
            Self::OptionType(inner) => format!("Option<{}>", inner.display_name()),
            Self::UnitRef { name } => name.clone(),
        }
    }

    #[must_use]
    pub fn is_scalar_real(&self) -> bool {
        matches!(self, Self::Float64)
    }
}
