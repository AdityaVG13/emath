//! Semantic variables, changes, world deltas, receipts.

use super::*;

/// Separates prior and next payloads in an operational [`SemanticChange::description`].
pub const PATCH_SEPARATOR: char = '\u{1f}';

/// Encodes a reversible semantic patch (`prior` then `next`).
#[must_use]
pub fn encode_patch(prior: &str, next: &str) -> String {
    format!("{prior}{PATCH_SEPARATOR}{next}")
}

/// Which World IR component a tuning variable or semantic delta may vary;
/// declaration order is the deterministic [`Ord`] order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticVariableKind {
    /// A carrier/domain choice.
    Carrier,
    /// A symbol declaration (id, fixity, precedence, type scheme).
    Symbol,
    /// The first-order signature.
    Signature,
    /// An operator's meaning.
    Operator,
    /// A constant's meaning.
    Constant,
    /// A constructor contract.
    Constructor,
    /// A law's shape.
    Law,
    /// A declared effect or capability.
    Effect,
}

impl SemanticVariableKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Carrier => "carrier",
            Self::Symbol => "symbol",
            Self::Signature => "signature",
            Self::Operator => "operator",
            Self::Constant => "constant",
            Self::Constructor => "constructor",
            Self::Law => "law",
            Self::Effect => "effect",
        }
    }

    /// Inverse of [`Self::canonical`].
    #[must_use]
    pub fn from_canonical(name: &str) -> Option<Self> {
        Some(match name {
            "carrier" => Self::Carrier,
            "symbol" => Self::Symbol,
            "signature" => Self::Signature,
            "operator" => Self::Operator,
            "constant" => Self::Constant,
            "constructor" => Self::Constructor,
            "law" => Self::Law,
            "effect" => Self::Effect,
            _ => return None,
        })
    }

    /// World IR component this kind touches (field name on `WorldIr`).
    #[must_use]
    pub fn component(self) -> &'static str {
        match self {
            Self::Carrier => "carriers",
            Self::Symbol => "symbols",
            Self::Signature => "signature",
            Self::Operator => "operators",
            Self::Constant => "constants",
            Self::Constructor => "constructors",
            Self::Law => "laws",
            Self::Effect => "effects",
        }
    }
}

/// One tunable semantic variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticVariable {
    /// What is varied.
    pub kind: SemanticVariableKind,
    /// Symbol being varied, when the kind targets a symbol.
    pub symbol: Option<SymbolId>,
    /// Canonical domain text (allowable variation), e.g.
    /// `finite tables over Bool`.
    pub domain: String,
}

impl SemanticVariable {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}",
            self.kind.canonical(),
            self.symbol
                .as_ref()
                .map_or_else(String::new, |s| s.0.clone()),
            self.domain
        )
    }
}

/// One concrete world change proposed by a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticChange {
    /// What the change targets.
    pub kind: SemanticVariableKind,
    /// Symbol targeted, when applicable.
    pub symbol: Option<SymbolId>,
    /// Canonical description of the new meaning.
    pub description: String,
    /// Provenance of the proposal (`synthesized`, `agent-proposal`,
    /// `evolutionary`, `fitted`, ...).
    pub provenance: String,
}

impl SemanticChange {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.kind.canonical(),
            self.symbol
                .as_ref()
                .map_or_else(String::new, |s| s.0.clone()),
            self.description,
            self.provenance
        )
    }

    /// Operational replace whose `description` encodes `prior` then `next`.
    ///
    /// [`WorldDelta::apply`] writes `next`; [`WorldDelta::revert`] restores `prior`.
    #[must_use]
    pub fn replace(
        kind: SemanticVariableKind,
        symbol: Option<SymbolId>,
        prior: impl Into<String>,
        next: impl Into<String>,
        provenance: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            symbol,
            description: encode_patch(&prior.into(), &next.into()),
            provenance: provenance.into(),
        }
    }
}

/// A world delta: a base world plus concrete semantic changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDelta {
    /// Base world identity.
    pub base_world: WorldId,
    /// Concrete changes, sorted by canonical form.
    pub changes: Vec<SemanticChange>,
}

impl WorldDelta {
    /// Builds a delta with deterministically sorted changes.
    #[must_use]
    pub fn new(base_world: WorldId, mut changes: Vec<SemanticChange>) -> Self {
        changes.sort_by_key(SemanticChange::canonical);
        Self {
            base_world,
            changes,
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "world-delta:base={}:{}",
            self.base_world.0,
            self.changes
                .iter()
                .map(SemanticChange::canonical)
                .collect::<Vec<_>>()
                .join(";")
        )
    }

    /// World IR components touched by this change set, sorted uniquely.
    #[must_use]
    pub fn locality(&self) -> Vec<&'static str> {
        locality_of(self.changes.iter().map(|change| change.kind))
    }

    /// Deterministic receipt over this delta's base fingerprint and changes.
    #[must_use]
    pub fn receipt(&self) -> DeltaReceipt {
        DeltaReceipt::new(self.base_world.0, &self.changes)
    }

    /// Applies this delta to `base`. `base.identity()` must match
    /// [`Self::base_world`]; missing targets are [`DeltaError::MissingTarget`],
    /// never silent no-ops; changes run in canonical sort order.
    pub fn apply(&self, base: &WorldIr) -> Result<WorldIr, DeltaError> {
        apply_or_revert(self, base, false)
    }

    /// Inverse of [`Self::apply`]: `revert(apply(base))` equals `base` in
    /// canonical form and identity; non-reversible changes are
    /// [`DeltaError::NotReversible`].
    pub fn revert(&self, applied: &WorldIr) -> Result<WorldIr, DeltaError> {
        apply_or_revert(self, applied, true)
    }
}

/// Typed refusal while applying or reverting a [`WorldDelta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// `base.identity()` did not match [`WorldDelta::base_world`].
    BaseMismatch {
        /// Identity recorded on the delta.
        expected: WorldId,
        /// Identity of the supplied world.
        actual: WorldId,
    },
    /// The addressed component is absent from the world.
    MissingTarget {
        /// Delta kind that failed to resolve.
        kind: SemanticVariableKind,
        /// Target id (symbol or list entry).
        target: String,
    },
    /// Live payload did not match the encoded prior (apply) or next (revert).
    PriorMismatch {
        /// Delta kind that failed the check.
        kind: SemanticVariableKind,
        /// Target id.
        target: String,
        /// Payload the delta expected to find.
        expected: String,
        /// Payload present on the world.
        actual: String,
    },
    /// Patch payload could not be parsed for this kind.
    MalformedPatch {
        /// Delta kind whose payload failed.
        kind: SemanticVariableKind,
        /// Human-readable parse reason.
        reason: String,
    },
    /// Revert was asked to invert a description that has no prior payload.
    NotReversible {
        /// Delta kind that cannot be inverted.
        kind: SemanticVariableKind,
    },
    /// A non-empty change set left [`WorldIr::identity`] unchanged.
    IdentityUnchanged,
    /// Revert did not restore [`WorldDelta::base_world`].
    DidNotRestore {
        /// Expected restored identity.
        expected: WorldId,
        /// Identity after revert.
        actual: WorldId,
    },
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseMismatch { expected, actual } => {
                write!(
                    f,
                    "world delta base mismatch: expected {}, actual {}",
                    expected.0, actual.0
                )
            }
            Self::MissingTarget { kind, target } => {
                write!(
                    f,
                    "world delta target missing: {} {target}",
                    kind.canonical()
                )
            }
            Self::PriorMismatch {
                kind,
                target,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "world delta prior mismatch: {} {target}: expected {expected}, actual {actual}",
                    kind.canonical()
                )
            }
            Self::MalformedPatch { kind, reason } => {
                write!(
                    f,
                    "world delta malformed patch: {}: {reason}",
                    kind.canonical()
                )
            }
            Self::NotReversible { kind } => {
                write!(f, "world delta is not reversible: {}", kind.canonical())
            }
            Self::IdentityUnchanged => {
                write!(f, "world delta did not change WorldId")
            }
            Self::DidNotRestore { expected, actual } => {
                write!(
                    f,
                    "world delta revert did not restore identity: expected {}, actual {}",
                    expected.0, actual.0
                )
            }
        }
    }
}

impl std::error::Error for DeltaError {}

/// Receipt of a world delta: base fingerprint, applied changes, identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaReceipt {
    /// Base world fingerprint (`WorldIr::identity` / `WorldId.0`).
    pub base_fingerprint: u64,
    /// Applied change canonical strings, sorted.
    pub applied: Vec<String>,
    /// FNV-1a64 over [`Self::canonical`].
    pub identity: u64,
}

impl DeltaReceipt {
    /// Builds a receipt with a deterministic identity: the same
    /// `(base_fingerprint, changes)` pair always yields the same identity.
    #[must_use]
    pub fn new(base_fingerprint: u64, changes: &[SemanticChange]) -> Self {
        let mut applied: Vec<String> = changes.iter().map(SemanticChange::canonical).collect();
        applied.sort();
        let receipt = Self {
            base_fingerprint,
            applied,
            identity: 0,
        };
        let identity = emath_world_ir::fnv1a64(receipt.canonical().as_bytes());
        Self {
            identity,
            ..receipt
        }
    }

    /// Deterministic canonical preimage for [`Self::identity`].
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "delta-receipt:base={}:{}",
            self.base_fingerprint,
            self.applied.join(";")
        )
    }

    /// World IR components touched by the applied changes, sorted uniquely.
    #[must_use]
    pub fn locality(&self) -> Vec<&'static str> {
        locality_of(self.applied.iter().filter_map(|applied| {
            applied
                .split(':')
                .next()
                .and_then(SemanticVariableKind::from_canonical)
        }))
    }
}

/// Sorted unique World IR component names for a set of delta kinds.
pub(super) fn locality_of(
    kinds: impl IntoIterator<Item = SemanticVariableKind>,
) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = kinds
        .into_iter()
        .map(SemanticVariableKind::component)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
