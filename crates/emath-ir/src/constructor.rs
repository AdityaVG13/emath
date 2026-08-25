//! Constructor semantic representation: parameters, obligations
//! (classified static/runtime/solver/certificate/deferred), construction
//! receipts and receipt composition for delegating constructors.

use crate::ids::{ExprId, TypeId};
use emath_core::{fnv1a64_bytes, ContentId, Span};
use std::collections::BTreeMap;

/// Constructor authority: parameters, preconditions, assignments,
/// postconditions, defaults, error type and construction failure variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constructor {
    pub name: String,
    pub parameters: Vec<Field>,
    pub preconditions: Vec<ExprId>,
    pub assignments: BTreeMap<String, ExprId>,
    pub postconditions: Vec<ExprId>,
    /// Default values for parameters that may be omitted at call sites.
    pub defaults: BTreeMap<String, ExprId>,
    pub error_type: Option<TypeId>,
    pub is_public: bool,
    pub source: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: TypeId,
    pub visibility: Visibility,
    pub source: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Package,
    Private,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub given: BTreeMap<String, ExprId>,
    /// Optional assertion. `None` is a worked example: compute and
    /// display values, make no pass/fail claim.
    pub expect: Option<ExprId>,
    pub source: Span,
}

/// How a construction obligation is discharged: Phase 1 checks every
/// textual `require`/`ensure`/`invariant` at runtime; the rest of the
/// taxonomy (V6 doc 04) awaits discharge engines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObligationClass {
    /// Proven at compile time; no generated check.
    Static,
    /// Checked in the generated constructor before the value escapes.
    Runtime,
    /// Discharged by an external solver whose answer is checked.
    Solver,
    /// Backed by a certificate verified by an independent checker.
    Certificate,
    /// Explicitly deferred; the value carries the undischarged obligation.
    Deferred,
}

impl ObligationClass {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Runtime => "runtime",
            Self::Solver => "solver",
            Self::Certificate => "certificate",
            Self::Deferred => "deferred",
        }
    }

    /// Whether a value may escape construction while this obligation is
    /// still open. Only `Deferred` permits that, and only with the
    /// obligation recorded on the receipt.
    #[must_use]
    pub const fn permits_escape_undischarged(self) -> bool {
        matches!(self, Self::Deferred)
    }
}

/// Where a construction obligation sits relative to field initialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObligationKind {
    /// `require`: checked before any field is initialized.
    Precondition,
    /// `ensure` / `invariant`: checked after field init, before escape.
    Postcondition,
}

impl ObligationKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Precondition => "precondition",
            Self::Postcondition => "postcondition",
        }
    }
}

/// One classified construction obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionObligation {
    /// Discharge class.
    pub class: ObligationClass,
    /// Position relative to field initialization.
    pub kind: ObligationKind,
    /// The obligation expression.
    pub expression: ExprId,
}

impl Constructor {
    /// The obligation matrix: every precondition/postcondition with its
    /// discharge class. Phase 1 classifies all textual obligations as
    /// `Runtime` because the generated constructor checks each one.
    #[must_use]
    pub fn obligation_matrix(&self) -> Vec<ConstructionObligation> {
        let mut matrix: Vec<ConstructionObligation> = self
            .preconditions
            .iter()
            .map(|expression| ConstructionObligation {
                class: ObligationClass::Runtime,
                kind: ObligationKind::Precondition,
                expression: *expression,
            })
            .collect();
        matrix.extend(
            self.postconditions
                .iter()
                .map(|expression| ConstructionObligation {
                    class: ObligationClass::Runtime,
                    kind: ObligationKind::Postcondition,
                    expression: *expression,
                }),
        );
        matrix
    }

    /// The construction receipt for this constructor within `declaration`.
    #[must_use]
    pub fn receipt(&self, declaration: &str) -> ConstructionReceipt {
        ConstructionReceipt {
            declaration: declaration.to_string(),
            constructor: self.name.clone(),
            obligations: self.obligation_matrix(),
        }
    }
}

/// Evidence a constructor's obligations are fully accounted for: each is
/// discharged (static, runtime, solver, certificate) or recorded deferred.
/// Receipts compose across constructor delegation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionReceipt {
    /// Declaration that owns the constructor.
    pub declaration: String,
    /// Constructor name.
    pub constructor: String,
    /// The full obligation matrix, in check order.
    pub obligations: Vec<ConstructionObligation>,
}

impl ConstructionReceipt {
    /// Compose delegating with delegate receipts: carries every obligation,
    /// delegate first (checked first at runtime). No obligation is dropped.
    #[must_use]
    pub fn compose(delegating: &Self, delegate: &Self) -> Self {
        let mut obligations = delegate.obligations.clone();
        obligations.extend(delegating.obligations.iter().cloned());
        Self {
            declaration: delegating.declaration.clone(),
            constructor: format!("{}<-{}", delegating.constructor, delegate.constructor),
            obligations,
        }
    }

    /// Obligations that remain open after construction (deferred class).
    #[must_use]
    pub fn open_obligations(&self) -> Vec<&ConstructionObligation> {
        self.obligations
            .iter()
            .filter(|obligation| obligation.class.permits_escape_undischarged())
            .collect()
    }

    /// Deterministic receipt identity over the canonical encoding.
    #[must_use]
    pub fn identity(&self) -> ContentId {
        let rows: Vec<String> = self
            .obligations
            .iter()
            .map(|obligation| {
                format!(
                    "{}:{}:{}",
                    obligation.class.name(),
                    obligation.kind.name(),
                    obligation.expression.0
                )
            })
            .collect();
        let canonical = format!(
            "receipt:{}:{}:[{}]",
            self.declaration,
            self.constructor,
            rows.join(",")
        );
        ContentId(format!("{:016x}", fnv1a64_bytes(canonical.as_bytes())))
    }
}

// Construction-obligation tests moved to `tests/emath-ir/tests/constructor.rs`.
