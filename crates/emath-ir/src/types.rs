//! Provider-free semantic type nodes (SIR types).

use emath_core::QualifiedName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeNode {
    Bool,
    Nat,
    Int,
    Rational,
    /// Real numbers: IEEE-754 binary64, strict semantics.
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
        extent: Option<String>,
    },
    Matrix {
        element: Box<TypeNode>,
        rows: Option<String>,
        cols: Option<String>,
    },
    Tensor {
        element: Box<TypeNode>,
        shape: Vec<String>,
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
                    extent.clone().unwrap_or_default()
                )
            }
            Self::Matrix {
                element,
                rows,
                cols,
            } => format!(
                "Matrix<{}, {}, {}>",
                element.display_name(),
                rows.clone().unwrap_or_default(),
                cols.clone().unwrap_or_default()
            ),
            Self::Tensor { element, shape } => {
                format!("Tensor<{}, [{}]>", element.display_name(), shape.join(", "))
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
