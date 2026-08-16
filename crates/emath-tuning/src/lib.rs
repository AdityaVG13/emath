#![forbid(unsafe_code)]

//! Semantic and joint tuning.
//!
//! Semantic tuning varies selected carriers, operators, constants, laws, or
//! valuations while protecting declared laws and held-out examples. Joint
//! tuning varies the world and the implementation together:
//!
//! ```text
//! candidate = WorldDelta + ExecutionDelta
//! ```
//!
//! Promotion requires (1) semantic admission, (2) evidence threshold,
//! (3) resource envelope (protected host metrics), (4) fallback
//! availability, and (5) a deterministic receipt. A world that merely
//! memorizes construction examples must not promote as a general meaning,
//! so every candidate is tested against held-out references before it can
//! be selected.

pub mod campaign;

use emath_term::SymbolId;

/// Which part of a world a tuning variable may vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticVariableKind {
    /// A carrier/domain choice.
    Carrier,
    /// An operator's meaning.
    Operator,
    /// A constant's meaning.
    Constant,
    /// A law's shape.
    Law,
    /// A valuation (cost/score) function.
    Valuation,
}

impl SemanticVariableKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Carrier => "carrier",
            Self::Operator => "operator",
            Self::Constant => "constant",
            Self::Law => "law",
            Self::Valuation => "valuation",
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
}

/// A world delta: a base world plus concrete semantic changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDelta {
    /// Base world identity.
    pub base_world: emath_world_ir::WorldId,
    /// Concrete changes, sorted by canonical form.
    pub changes: Vec<SemanticChange>,
}

impl WorldDelta {
    /// Builds a delta with deterministically sorted changes.
    #[must_use]
    pub fn new(base_world: emath_world_ir::WorldId, mut changes: Vec<SemanticChange>) -> Self {
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
}

/// An implementation delta: lowering, precision, provider, target, schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDelta {
    /// Lowering/algorithm label, e.g. `polynomial`.
    pub lowering: String,
    /// Precision label, e.g. `f64`, `f32_bounded`.
    pub precision: String,
    /// Provider label, e.g. `native`, `dew`.
    pub provider: String,
    /// Target label, e.g. `cpu.simd`, `gpu.wgsl`.
    pub target: String,
    /// Schedule label, e.g. `shadow-first`.
    pub schedule: String,
}

impl ExecutionDelta {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "exec-delta:{}/{}:{}:{}:{}",
            self.lowering, self.precision, self.provider, self.target, self.schedule
        )
    }
}

/// A joint tuning candidate: world delta plus execution delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointCandidate {
    /// Stable label.
    pub label: String,
    /// World changes.
    pub world: WorldDelta,
    /// Implementation changes.
    pub execution: ExecutionDelta,
    /// Whether the candidate passed the held-out challenge.
    pub held_out_verified: bool,
    /// Evidence units admitting the candidate semantics.
    pub evidence_units: u32,
    /// Candidate content identity (FNV-1a64 over canonical form).
    pub identity: u64,
}

impl JointCandidate {
    /// Builds a candidate with deterministic identity.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        world: WorldDelta,
        execution: ExecutionDelta,
        held_out_verified: bool,
        evidence_units: u32,
    ) -> Self {
        let label = label.into();
        let candidate = Self {
            label,
            world,
            execution,
            held_out_verified,
            evidence_units,
            identity: 0,
        };
        let identity = emath_world_ir::fnv1a64(candidate.canonical().as_bytes());
        Self {
            identity,
            ..candidate
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "candidate:{}:{}:{}:held-out={}:evidence={}",
            self.label,
            self.world.canonical(),
            self.execution.canonical(),
            self.held_out_verified,
            self.evidence_units
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_world_ir::WorldId;

    fn candidate(label: &str, held_out: bool, evidence: u32) -> JointCandidate {
        JointCandidate::new(
            label,
            WorldDelta::new(
                WorldId(7),
                vec![SemanticChange {
                    kind: SemanticVariableKind::Operator,
                    symbol: Some(SymbolId("⋈".into())),
                    description: "xor over Bool".into(),
                    provenance: "synthesized".into(),
                }],
            ),
            ExecutionDelta {
                lowering: "algebraic".into(),
                precision: "f64".into(),
                provider: "native".into(),
                target: "cpu".into(),
                schedule: "direct".into(),
            },
            held_out,
            evidence,
        )
    }

    #[test]
    fn candidate_identity_is_deterministic_and_mutation_sensitive() {
        let a = candidate("a", true, 3);
        let b = candidate("a", true, 3);
        assert_eq!(a.identity, b.identity);
        let changed = candidate("a", true, 4);
        assert_ne!(a.identity, changed.identity);
        let relabeled = candidate("x", true, 3);
        assert_ne!(a.identity, relabeled.identity);
    }

    #[test]
    fn deltas_canonicalize_deterministically() {
        let world = WorldDelta::new(
            WorldId(7),
            vec![
                SemanticChange {
                    kind: SemanticVariableKind::Law,
                    symbol: None,
                    description: "commutativity".into(),
                    provenance: "agent-proposal".into(),
                },
                SemanticChange {
                    kind: SemanticVariableKind::Operator,
                    symbol: Some(SymbolId("⋈".into())),
                    description: "xor over Bool".into(),
                    provenance: "synthesized".into(),
                },
            ],
        );
        assert_eq!(world.canonical(), world.canonical());
    }
}
