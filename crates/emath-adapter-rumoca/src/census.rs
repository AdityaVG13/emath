//! Compiler-phase census for the Rumoca provider seam.
//!
//! Documents the Modelica compiler phases and what Phase 1 actually
//! provides for each. Honesty contract: Phase 1 consumes **no upstream
//! Rumoca fork** — every posture describes an in-tree native stand-in
//! (subset string scanner, validation gate, native causalizer and
//! forward-Euler simulator), so no phase is marked `Stable` and no phase
//! is consumed through an upstream `public_contract`. Deterministic,
//! static, and provider-free (phase names are neutral Modelica-compiler
//! phase names, not fork types).

/// Rust-units subset of upstream compiler phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PhaseKind {
    /// Textual syntax/semantic parse.
    Parse,
    /// Name resolution and scoping.
    Resolve,
    /// Type checking.
    TypeCheck,
    /// Model instantiation.
    Instantiation,
    /// Flattening of instantiated components.
    Flattening,
    /// Conversion to differential-algebraic equations.
    DaeConversion,
    /// Structural analysis/causalization.
    StructuralAnalysis,
    /// Simulation execution.
    Simulation,
    /// Template/modifier expansion.
    Templates,
}

/// Stability posture of a provider phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    /// Emitted output is load-bearing for emath.
    Stable,
    /// Behavior may evolve; consumers must pin versions.
    Developmental,
    /// Exploration only; no compatibility contract.
    Experimental,
}

/// A single census row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseRecord {
    /// Phase identity.
    pub kind: PhaseKind,
    /// Stability posture.
    pub stability: Stability,
    /// Whether emath consumes this phase through its public contract.
    pub public_contract: bool,
    /// One-line description.
    pub note: &'static str,
}

/// Canonical census table (stable order, never sorted at runtime).
///
/// Phase 1 consumes no upstream Rumoca fork: the in-tree adapter drives
/// native stand-ins only, so no phase is `Stable` and none is marked
/// `public_contract`.
pub const PHASES: [PhaseRecord; 9] = [
    PhaseRecord {
        kind: PhaseKind::Parse,
        stability: Stability::Developmental,
        public_contract: false,
        note: "Modelica subset string scanner (retained declarations); no upstream parser",
    },
    PhaseRecord {
        kind: PhaseKind::Resolve,
        stability: Stability::Experimental,
        public_contract: false,
        note: "no name resolver in Phase 1",
    },
    PhaseRecord {
        kind: PhaseKind::TypeCheck,
        stability: Stability::Developmental,
        public_contract: false,
        note: "structural model validation gate (units, duplicate names)",
    },
    PhaseRecord {
        kind: PhaseKind::Instantiation,
        stability: Stability::Experimental,
        public_contract: false,
        note: "no model instantiation in Phase 1",
    },
    PhaseRecord {
        kind: PhaseKind::Flattening,
        stability: Stability::Experimental,
        public_contract: false,
        note: "no hierarchical flattening in Phase 1",
    },
    PhaseRecord {
        kind: PhaseKind::DaeConversion,
        stability: Stability::Developmental,
        public_contract: false,
        note: "native causalization/lowering (provider emath-native-causalizer)",
    },
    PhaseRecord {
        kind: PhaseKind::StructuralAnalysis,
        stability: Stability::Developmental,
        public_contract: false,
        note: "native matching/ordering inside the causalizer; no upstream engine",
    },
    PhaseRecord {
        kind: PhaseKind::Simulation,
        stability: Stability::Developmental,
        public_contract: false,
        note: "native forward-Euler simulator (provider emath-native-euler)",
    },
    PhaseRecord {
        kind: PhaseKind::Templates,
        stability: Stability::Experimental,
        public_contract: false,
        note: "no template/modifier expansion in Phase 1",
    },
];

/// Returns the census record for a phase kind.
#[must_use]
pub fn phase(kind: PhaseKind) -> Option<&'static PhaseRecord> {
    let mut index = 0;
    while index < PHASES.len() {
        if PHASES[index].kind == kind {
            return Some(&PHASES[index]);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{PHASES, PhaseKind, Stability, phase};

    #[test]
    fn no_phase_claims_upstream_stability_or_public_contract() {
        // Phase 1 has native stand-ins only; a phase marked Stable or
        // public_contract would claim an upstream Rumoca engine that is
        // not consumed.
        for record in &PHASES {
            assert_ne!(
                record.stability,
                Stability::Stable,
                "phase {:?} must not claim Stable without an upstream engine",
                record.kind
            );
            assert!(
                !record.public_contract,
                "phase {:?} must not claim a public upstream contract",
                record.kind
            );
        }
    }

    #[test]
    fn census_phase_lookup_round_trips() {
        for record in &PHASES {
            assert_eq!(
                phase(record.kind),
                Some(record),
                "phase lookup must find every census row"
            );
        }
        assert_eq!(
            phase(PhaseKind::Resolve).unwrap().note,
            "no name resolver in Phase 1"
        );
    }
}
