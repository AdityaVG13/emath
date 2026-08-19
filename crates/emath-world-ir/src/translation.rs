//! Cross-world translation: morphisms, preservation obligations, evidence,
//! and strict/fast execution portfolios with deoptimization.

use crate::{WorldId, WorldIr, fnv1a64};
use emath_term::SymbolId;

/// Relation claimed by a world morphism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreservationRelation {
    /// Semantics agree on every value.
    Exact,
    /// Target semantics admit at most the source semantics (or vice versa).
    Refinement,
    /// Target semantics agree within a stated error bound.
    Approximation,
    /// Target semantics agree on observable projections.
    Simulation,
    /// Target semantics agree on a shared observation predicate.
    ObservationalEquivalence,
}

impl PreservationRelation {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Refinement => "refinement",
            Self::Approximation => "approximation",
            Self::Simulation => "simulation",
            Self::ObservationalEquivalence => "observational-equivalence",
        }
    }

    /// Whether this relation transports interpretive authority unchanged.
    ///
    /// Matches the portfolio's conservative authority cap: `Exact` and
    /// `Refinement` transport authority; `Approximation`, `Simulation`,
    /// and `ObservationalEquivalence` only guarantee weaker agreement, so
    /// answers produced through them degrade to structural authority.
    #[must_use]
    pub fn transports_authority(self) -> bool {
        matches!(self, Self::Exact | Self::Refinement)
    }
}

/// Mapping between the carrier of the source world and the carrier of the
/// target world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierMap {
    /// Carrier name in the source world.
    pub source_carrier: String,
    /// Carrier name in the target world.
    pub target_carrier: String,
    /// Canonical description of the mapping (deterministic text).
    pub mapping: String,
}

impl CarrierMap {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}->{}:{}",
            self.source_carrier, self.target_carrier, self.mapping
        )
    }
}

/// One preservation obligation for an operator or constant under a
/// morphism. Each relation kind carries its own obligation text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreservationObligation {
    /// Symbol being transported.
    pub symbol: SymbolId,
    /// Claimed preservation relation.
    pub relation: PreservationRelation,
    /// Canonical obligation text, e.g.
    /// `forall x y. ⋈(map(x),map(y)) == map(⋈(x,y))` for exactness.
    pub obligation: String,
}

impl PreservationObligation {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}",
            self.symbol.0,
            self.relation.canonical(),
            self.obligation
        )
    }
}

/// Deterministic evidence handle scoping a morphism claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceHandle {
    /// Stable handle identity (content-hash of the underlying receipt).
    pub id: u64,
    /// Canonical provenance text.
    pub provenance: String,
    /// Scope, e.g. `obligation:⋈:exact`.
    pub scope: String,
}

impl EvidenceHandle {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{:x}:{}:{}", self.id, self.scope, self.provenance)
    }
}

/// A morphism from a source world to a target world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMorphism {
    /// Source world identity.
    pub source: WorldId,
    /// Target world identity.
    pub target: WorldId,
    /// Carrier mappings, sorted by canonical form for determinism.
    pub carrier_maps: Vec<CarrierMap>,
    /// Operator obligations, sorted by symbol for determinism.
    pub operator_obligations: Vec<PreservationObligation>,
    /// Evidence backing the obligations, sorted by id for determinism.
    pub evidence: Vec<EvidenceHandle>,
}

impl WorldMorphism {
    /// Builds a morphism with deterministically sorted obligations.
    #[must_use]
    pub fn new(
        source: WorldId,
        target: WorldId,
        mut carrier_maps: Vec<CarrierMap>,
        mut operator_obligations: Vec<PreservationObligation>,
        mut evidence: Vec<EvidenceHandle>,
    ) -> Self {
        carrier_maps.sort_by_key(CarrierMap::canonical);
        operator_obligations.sort_by_key(|a| a.symbol.0.clone());
        evidence.sort_by_key(|e| e.id);
        Self {
            source,
            target,
            carrier_maps,
            operator_obligations,
            evidence,
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let carriers = self
            .carrier_maps
            .iter()
            .map(CarrierMap::canonical)
            .collect::<Vec<_>>()
            .join(";");
        let obligations = self
            .operator_obligations
            .iter()
            .map(PreservationObligation::canonical)
            .collect::<Vec<_>>()
            .join(";");
        let evidence = self
            .evidence
            .iter()
            .map(EvidenceHandle::canonical)
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "morphism:src={}:dst={}:carriers={}:obligations={}:evidence={}",
            self.source.0, self.target.0, carriers, obligations, evidence
        )
    }

    /// Deterministic content identity of the morphism.
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64(self.canonical().as_bytes())
    }

    /// The claimed relation for a symbol, if any.
    #[must_use]
    pub fn relation_for(&self, symbol: &SymbolId) -> Option<PreservationRelation> {
        self.operator_obligations
            .iter()
            .find(|obligation| obligation.symbol == *symbol)
            .map(|obligation| obligation.relation)
    }
}

/// Why a fast-world execution path was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeoptReason {
    /// A used symbol is not covered by the fast world.
    Applicability {
        /// First uncovered symbol.
        symbol: SymbolId,
        /// Fast-world applicability domain text.
        domain: String,
    },
    /// Inputs fall outside the fast world's declared domain.
    DomainViolation {
        /// Fast-world applicability domain text.
        domain: String,
    },
    /// Required evidence was not validated.
    EvidenceMissing {
        /// First missing evidence handle.
        evidence: u64,
        /// Evidence scope.
        scope: String,
    },
    /// The caller requires full authority but a used symbol's obligation
    /// only preserves a weaker relation, so the fast answer would carry
    /// degraded authority.
    AuthorityDegraded {
        /// First symbol whose relation does not transport authority.
        symbol: SymbolId,
        /// Canonical name of the offending relation.
        relation: &'static str,
    },
}

impl DeoptReason {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Applicability { symbol, domain } => {
                format!("applicability:{}:{domain}", symbol.0)
            }
            Self::DomainViolation { domain } => format!("domain:{domain}"),
            Self::EvidenceMissing { evidence, scope } => {
                format!("evidence:{evidence:x}:{scope}")
            }
            Self::AuthorityDegraded { symbol, relation } => {
                format!("authority:{}:{relation}", symbol.0)
            }
        }
    }
}

/// Guards that must hold for the fast path to be admissible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastPathGuard {
    /// Canonical applicability domain text, e.g. `input value < 8`.
    pub domain: String,
    /// Evidence handles that must validate before the fast path is used.
    pub required_evidence: Vec<u64>,
}

impl FastPathGuard {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let evidence = self
            .required_evidence
            .iter()
            .map(|id| format!("{id:x}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("domain={}:evidence=[{}]", self.domain, evidence)
    }
}

/// Invariant violation when assembling a strict/fast portfolio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationError {
    /// The morphism source does not equal the strict world identity.
    SourceMismatch,
    /// The morphism target does not equal the fast world identity.
    TargetMismatch,
    /// A fast-world operator has no preservation obligation.
    UnobligatedFastSymbol(SymbolId),
}

impl TranslationError {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::SourceMismatch => "source-mismatch".to_string(),
            Self::TargetMismatch => "target-mismatch".to_string(),
            Self::UnobligatedFastSymbol(symbol) => {
                format!("unobligated-fast-symbol:{}", symbol.0)
            }
        }
    }
}

/// Strict/fast execution portfolio. The fast world is used only while its
/// guards hold; otherwise execution deoptimizes to the strict world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictFastPortfolio {
    strict: WorldIr,
    fast: WorldIr,
    strict_to_fast: WorldMorphism,
    guard: FastPathGuard,
}

impl StrictFastPortfolio {
    /// Assembles a portfolio with constructor invariants enforced:
    ///
    /// - `strict_to_fast.source` equals `strict.identity()`;
    /// - `strict_to_fast.target` equals `fast.identity()`;
    /// - every operator symbol of the fast world carries at least one
    ///   preservation obligation.
    pub fn new(
        strict: WorldIr,
        fast: WorldIr,
        strict_to_fast: WorldMorphism,
        guard: FastPathGuard,
    ) -> Result<Self, TranslationError> {
        let strict_id = strict.identity();
        let fast_id = fast.identity();
        if strict_to_fast.source != strict_id {
            return Err(TranslationError::SourceMismatch);
        }
        if strict_to_fast.target != fast_id {
            return Err(TranslationError::TargetMismatch);
        }
        if let Some(symbol) = fast.signature.iter().find_map(|(symbol, _)| {
            let covered = strict_to_fast
                .operator_obligations
                .iter()
                .any(|obligation| obligation.symbol == *symbol);
            (!covered).then(|| (*symbol).clone())
        }) {
            return Err(TranslationError::UnobligatedFastSymbol(symbol));
        }
        Ok(Self {
            strict,
            fast,
            strict_to_fast,
            guard,
        })
    }

    /// The strict world (always safe).
    #[must_use]
    pub fn strict(&self) -> &WorldIr {
        &self.strict
    }

    /// The fast world (guarded).
    #[must_use]
    pub fn fast(&self) -> &WorldIr {
        &self.fast
    }

    /// The strict-to-fast morphism backing the fast path.
    #[must_use]
    pub fn strict_to_fast(&self) -> &WorldMorphism {
        &self.strict_to_fast
    }

    /// The fast-path guards.
    #[must_use]
    pub fn guard(&self) -> &FastPathGuard {
        &self.guard
    }

    /// Fast-path admission check.
    ///
    /// Returns the fast world identity when every guard holds. Returns a
    /// [`DeoptReason`] when a guard fails; the caller must then evaluate in
    /// the strict world.
    pub fn try_fast(
        &self,
        used_symbols: &[SymbolId],
        inputs_in_domain: bool,
        evidence_valid: bool,
    ) -> Result<WorldId, DeoptReason> {
        if let Some(symbol) = used_symbols.iter().find(|symbol| {
            !self
                .fast
                .operators
                .iter()
                .any(|operator| operator.symbol == **symbol)
        }) {
            return Err(DeoptReason::Applicability {
                symbol: symbol.clone(),
                domain: self.guard.domain.clone(),
            });
        }
        if !inputs_in_domain {
            return Err(DeoptReason::DomainViolation {
                domain: self.guard.domain.clone(),
            });
        }
        if !evidence_valid {
            return Err(DeoptReason::EvidenceMissing {
                evidence: self
                    .guard
                    .required_evidence
                    .first()
                    .copied()
                    .unwrap_or_default(),
                scope: "fast-path".to_string(),
            });
        }
        Ok(self.fast.identity())
    }

    /// Selects the execution world: fast when all guards hold, otherwise the
    /// strict world (deoptimization).
    #[must_use]
    pub fn select_world(
        &self,
        used_symbols: &[SymbolId],
        inputs_in_domain: bool,
        evidence_valid: bool,
    ) -> (WorldId, Option<DeoptReason>) {
        match self.try_fast(used_symbols, inputs_in_domain, evidence_valid) {
            Ok(id) => (id, None),
            Err(reason) => (self.strict.identity(), Some(reason)),
        }
    }

    /// Authority-aware fast-path admission.
    ///
    /// Like [`Self::try_fast`], but when the caller requires full
    /// authority (an authoritative answer, not best-effort), the fast
    /// path is additionally refused if any used symbol's preservation
    /// obligation does not transport authority — the answer would silently
    /// degrade to structural authority. A symbol with no obligation at
    /// all already deoptimizes through the applicability guard.
    pub fn try_fast_with_authority(
        &self,
        used_symbols: &[SymbolId],
        inputs_in_domain: bool,
        evidence_valid: bool,
        require_full_authority: bool,
    ) -> Result<WorldId, DeoptReason> {
        if require_full_authority {
            for symbol in used_symbols {
                if let Some(relation) = self.strict_to_fast.relation_for(symbol)
                    && !relation.transports_authority()
                {
                    return Err(DeoptReason::AuthorityDegraded {
                        symbol: symbol.clone(),
                        relation: relation.canonical(),
                    });
                }
            }
        }
        self.try_fast(used_symbols, inputs_in_domain, evidence_valid)
    }

    /// Authority-aware world selection: fast when guards hold *and* the
    /// required authority is preserved, otherwise the strict world.
    #[must_use]
    pub fn select_world_with_authority(
        &self,
        used_symbols: &[SymbolId],
        inputs_in_domain: bool,
        evidence_valid: bool,
        require_full_authority: bool,
    ) -> (WorldId, Option<DeoptReason>) {
        match self.try_fast_with_authority(
            used_symbols,
            inputs_in_domain,
            evidence_valid,
            require_full_authority,
        ) {
            Ok(id) => (id, None),
            Err(reason) => (self.strict.identity(), Some(reason)),
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "portfolio:strict={}:fast={}:morphism={}:guard={}",
            self.strict.identity().0,
            self.fast.identity().0,
            self.strict_to_fast.canonical(),
            self.guard.canonical()
        )
    }
}
