//! Typed finite-category wrappers (emath-r3-abstract-algebra-88wo
//! thin B39 slice): the error model over the raw kernels in
//! [`crate::body`].
//!
//! Representation law (the orch-decided masa-style carrier): a finite
//! category is a dense composition table with per-morphism dom/cod;
//! diagrams are face path-pairs. Category laws (composition, identity,
//! associativity) are CERTIFIED before any commutativity answer.
//!
//! Refusals are typed, never silent: `E-CAT-001` (non-finite entry),
//! `E-CAT-002` (shape: dims, face record, path geometry), `E-CAT-003`
//! (out-of-range/non-integral index), `E-CAT-004` (composition law),
//! `E-CAT-005` (identity law), `E-CAT-006` (associativity law),
//! `E-CAT-007` (carrier too large to certify associativity).
//!
//! No-claim boundaries: functors, natural transformations, and
//! higher-category surfaces are not implemented; forms/manifolds (B38)
//! and algebraic geometry (B45) stay deferred. Determinism class:
//! fixed-order law passes, first-failure refusal, index-fold path
//! evaluation; identical inputs are bit-identical.

/// Finite-category refusal. Closed set; codes are the language surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryError {
    /// `E-CAT-001` — a non-finite entry anywhere in the carrier.
    NonFinite,
    /// `E-CAT-002` — shape: dimensions, face record, path geometry.
    BadShape,
    /// `E-CAT-003` — an out-of-range or non-integral index.
    BadIndex,
    /// `E-CAT-004` — the composition law is violated.
    EntryLaw,
    /// `E-CAT-005` — the identity law is violated.
    IdentityLaw,
    /// `E-CAT-006` — the associativity law is violated.
    AssociativityLaw,
    /// `E-CAT-007` — the carrier exceeds the certifiable bound.
    TooLarge,
}

impl CategoryError {
    /// Stable diagnostic code (the language surface).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NonFinite => "E-CAT-001",
            Self::BadShape => "E-CAT-002",
            Self::BadIndex => "E-CAT-003",
            Self::EntryLaw => "E-CAT-004",
            Self::IdentityLaw => "E-CAT-005",
            Self::AssociativityLaw => "E-CAT-006",
            Self::TooLarge => "E-CAT-007",
        }
    }
}

impl std::fmt::Display for CategoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => write!(
                formatter,
                "{code}: category carriers must be finite",
                code = self.code()
            ),
            Self::BadShape => write!(
                formatter,
                "{code}: malformed category carrier or face record",
                code = self.code()
            ),
            Self::BadIndex => write!(
                formatter,
                "{code}: object/morphism indices must be whole and in range",
                code = self.code()
            ),
            Self::EntryLaw => write!(
                formatter,
                "{code}: the composition table violates the composition law",
                code = self.code()
            ),
            Self::IdentityLaw => write!(
                formatter,
                "{code}: an appearing object has no identity morphism",
                code = self.code()
            ),
            Self::AssociativityLaw => write!(
                formatter,
                "{code}: the composition table is not associative",
                code = self.code()
            ),
            Self::TooLarge => write!(
                formatter,
                "{code}: too many morphisms to certify associativity",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for CategoryError {}

fn category_error(status: crate::body::CategoryStatus) -> CategoryError {
    // The `Valid` arm is unreachable by construction (the mapper only
    // sees gate failures) but the function stays total.
    match status {
        crate::body::CategoryStatus::Valid => CategoryError::EntryLaw,
        crate::body::CategoryStatus::NonFinite => CategoryError::NonFinite,
        crate::body::CategoryStatus::BadShape => CategoryError::BadShape,
        crate::body::CategoryStatus::BadIndex => CategoryError::BadIndex,
        crate::body::CategoryStatus::EntryLaw => CategoryError::EntryLaw,
        crate::body::CategoryStatus::IdentityLaw => CategoryError::IdentityLaw,
        crate::body::CategoryStatus::AssociativityLaw => CategoryError::AssociativityLaw,
        crate::body::CategoryStatus::TooLarge => CategoryError::TooLarge,
    }
}

/// Certify the carrier as a category (laws verified, never assumed).
///
/// # Errors
/// The first violated law in the documented pass order, typed
/// (`E-CAT-001..007`).
pub fn category_check(dom: &[f64], cod: &[f64], comp: &[Vec<f64>]) -> Result<bool, CategoryError> {
    match crate::body::category_check_status(dom, cod, comp) {
        crate::body::CategoryStatus::Valid => Ok(true),
        status => Err(category_error(status)),
    }
}

/// Diagram commutativity over face path-pairs; the carrier must
/// certify first.
///
/// # Errors
/// Any carrier-law or face-geometry refusal, typed
/// (`E-CAT-001..007`).
pub fn diagram_commutative(
    dom: &[f64],
    cod: &[f64],
    comp: &[Vec<f64>],
    faces: &[f64],
) -> Result<Vec<bool>, CategoryError> {
    crate::body::category_diagram_commutative_status(dom, cod, comp, faces).map_err(category_error)
}
