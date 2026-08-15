//! Cross-world translation: morphisms, preservation obligations, evidence,
//! and strict/fast execution portfolios with deoptimization (V7 g11).

use crate::{fnv1a64, WorldId, WorldIr};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr,
    };
    use emath_term::Signature;

    fn symbol(name: &str) -> SymbolId {
        SymbolId(name.to_string())
    }

    fn signature_with(names: &[&str]) -> Signature {
        let mut signature = Signature::default();
        for name in names {
            let arity = if *name == "ζ" { 0 } else { 2 };
            signature.insert(symbol(name), arity).unwrap();
        }
        signature
    }

    fn world(name: &str, names: &[&str], extra_laws: &[&str]) -> WorldIr {
        let signature = signature_with(names);
        let symbols = names
            .iter()
            .map(|name| SymbolDef {
                id: symbol(name),
                display: (*name).to_string(),
                fixity: if *name == "ζ" {
                    Fixity::Constant
                } else {
                    Fixity::Infix
                },
                precedence: None,
                type_scheme: "Value^2 -> Value".to_string(),
            })
            .collect::<Vec<_>>();
        let operators = names
            .iter()
            .map(|name| OperatorDef {
                symbol: symbol(name),
                semantics: OperatorSemantics::DeclaredExpression(format!("{name}-op")),
                origin: MeaningOrigin::Declared,
            })
            .collect::<Vec<_>>();
        let mut laws = vec!["total".to_string(), "deterministic".to_string()];
        laws.extend(extra_laws.iter().map(|law| (*law).to_string()));
        WorldIr {
            version: 1,
            name: name.to_string(),
            signature,
            carriers: vec![CarrierDef {
                name: "Value".to_string(),
                type_expression: "Mod17".to_string(),
            }],
            symbols,
            operators,
            constructors: vec!["Value::Z17".to_string()],
            laws,
            holes: vec![],
            capabilities: vec!["pure".to_string()],
        }
    }

    fn exact_obligation(name: &str) -> PreservationObligation {
        PreservationObligation {
            symbol: symbol(name),
            relation: PreservationRelation::Exact,
            obligation: format!("forall x y. {name}(map(x), map(y)) == map({name}(x, y))"),
        }
    }

    fn fixture() -> (StrictFastPortfolio, u64) {
        let names = ["ζ", "⋈", "⧖", "⊛"];
        let strict = world("modular-17", &names, &[]);
        let fast = world("modular-17-fast", &names, &["fast-domain: input value < 8"]);
        let source = strict.identity();
        let target = fast.identity();
        let mut obligations: Vec<_> = names.iter().map(|name| exact_obligation(name)).collect();
        // Move ζ first to prove sorting determinism.
        obligations.rotate_left(1);
        let evidence_handle = EvidenceHandle {
            id: 0xdecaf,
            provenance: "seeded tables, verified by enumeration".to_string(),
            scope: "fast-path".to_string(),
        };
        let morphism = WorldMorphism::new(
            source,
            target,
            vec![CarrierMap {
                source_carrier: "Value".to_string(),
                target_carrier: "Value".to_string(),
                mapping: "identity on Z17".to_string(),
            }],
            obligations,
            vec![evidence_handle.clone()],
        );
        let portfolio = StrictFastPortfolio::new(
            strict,
            fast,
            morphism,
            FastPathGuard {
                domain: "input value < 8".to_string(),
                required_evidence: vec![evidence_handle.id],
            },
        )
        .unwrap();
        (portfolio, evidence_handle.id)
    }

    #[test]
    fn morphism_canonical_is_deterministic_and_mutation_sensitive() {
        let (portfolio, _) = fixture();
        let first = portfolio.strict_to_fast().canonical();
        let second = portfolio.strict_to_fast().canonical();
        assert_eq!(first, second);
        let mut changed = portfolio.strict_to_fast().clone();
        changed.source = WorldId(changed.source.0 ^ 1);
        assert_ne!(changed.canonical(), first);
        assert_ne!(changed.identity(), portfolio.strict_to_fast().identity());
    }

    #[test]
    fn portfolio_rejects_source_and_target_mismatches() {
        let names = ["ζ", "⋈", "⧖", "⊛"];
        let strict = world("a", &names, &[]);
        let fast = world("b", &names, &[]);
        let wrong = WorldId(1234);
        let morphism = WorldMorphism::new(
            wrong,
            fast.identity(),
            vec![],
            names.iter().map(|name| exact_obligation(name)).collect(),
            vec![],
        );
        assert_eq!(
            StrictFastPortfolio::new(
                strict.clone(),
                fast.clone(),
                morphism.clone(),
                FastPathGuard {
                    domain: "d".to_string(),
                    required_evidence: vec![],
                },
            ),
            Err(TranslationError::SourceMismatch)
        );
        let morphism2 = WorldMorphism::new(
            strict.identity(),
            WorldId(5678),
            vec![],
            names.iter().map(|name| exact_obligation(name)).collect(),
            vec![],
        );
        assert_eq!(
            StrictFastPortfolio::new(
                strict,
                fast,
                morphism2,
                FastPathGuard {
                    domain: "d".to_string(),
                    required_evidence: vec![],
                },
            ),
            Err(TranslationError::TargetMismatch)
        );
    }

    #[test]
    fn portfolio_rejects_unobligated_fast_symbol() {
        let names = ["ζ", "⋈"];
        let strict = world("s", &names, &[]);
        let fast = world("f", &names, &[]);
        let morphism = WorldMorphism::new(
            strict.identity(),
            fast.identity(),
            vec![],
            vec![exact_obligation("ζ")],
            vec![],
        );
        assert_eq!(
            StrictFastPortfolio::new(
                strict,
                fast,
                morphism,
                FastPathGuard {
                    domain: "d".to_string(),
                    required_evidence: vec![],
                },
            ),
            Err(TranslationError::UnobligatedFastSymbol(symbol("⋈")))
        );
    }

    #[test]
    fn fast_path_is_selected_when_all_guards_hold() {
        let (portfolio, _) = fixture();
        let used = [symbol("ζ"), symbol("⋈")];
        let selection = portfolio.select_world(&used, true, true);
        assert_eq!(selection.0, portfolio.fast().identity());
        assert!(selection.1.is_none());
        assert_eq!(
            portfolio.try_fast(&used, true, true),
            Ok(portfolio.fast().identity())
        );
    }

    #[test]
    fn fast_path_deoptimizes_on_uncovered_symbol() {
        let (portfolio, _) = fixture();
        let used = [symbol("ζ"), symbol("alien-op")];
        let selection = portfolio.select_world(&used, true, true);
        assert_eq!(selection.0, portfolio.strict().identity());
        let reason = selection.1.unwrap();
        assert!(matches!(
            reason,
            DeoptReason::Applicability { symbol, .. } if symbol == SymbolId("alien-op".to_string())
        ));
    }

    #[test]
    fn fast_path_deoptimizes_on_domain_violation() {
        let (portfolio, _) = fixture();
        let used = [symbol("⋈")];
        let selection = portfolio.select_world(&used, false, true);
        assert_eq!(selection.0, portfolio.strict().identity());
        assert_eq!(
            selection.1.unwrap(),
            DeoptReason::DomainViolation {
                domain: "input value < 8".to_string()
            }
        );
    }

    #[test]
    fn fast_path_deoptimizes_on_missing_evidence() {
        let (portfolio, evidence_id) = fixture();
        let used = [symbol("⋈")];
        let selection = portfolio.select_world(&used, true, false);
        assert_eq!(selection.0, portfolio.strict().identity());
        assert_eq!(
            selection.1.unwrap(),
            DeoptReason::EvidenceMissing {
                evidence: evidence_id,
                scope: "fast-path".to_string()
            }
        );
    }

    #[test]
    fn portfolio_canonical_is_deterministic() {
        let (portfolio, _) = fixture();
        assert_eq!(portfolio.canonical(), portfolio.canonical());
    }
}
