//! Compiler-phase census for the Rumoca provider seam.
//!
//! Documents the provider compiler phases that emath models flow through,
//! with their stability posture and whether emath relies on the public
//! contract. Deterministic, static, and provider-free (phase names are
//! neutral Modelica-compiler phase names, not fork types).

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
pub const PHASES: [PhaseRecord; 9] = [
    PhaseRecord {
        kind: PhaseKind::Parse,
        stability: Stability::Stable,
        public_contract: true,
        note: "textual parse of the Modelica subset",
    },
    PhaseRecord {
        kind: PhaseKind::Resolve,
        stability: Stability::Stable,
        public_contract: true,
        note: "name resolution and visibility",
    },
    PhaseRecord {
        kind: PhaseKind::TypeCheck,
        stability: Stability::Stable,
        public_contract: true,
        note: "type and unit checking",
    },
    PhaseRecord {
        kind: PhaseKind::Instantiation,
        stability: Stability::Stable,
        public_contract: true,
        note: "model instantiation with modifiers",
    },
    PhaseRecord {
        kind: PhaseKind::Flattening,
        stability: Stability::Stable,
        public_contract: true,
        note: "hierarchical flattening",
    },
    PhaseRecord {
        kind: PhaseKind::DaeConversion,
        stability: Stability::Stable,
        public_contract: true,
        note: "equation/differential conversion",
    },
    PhaseRecord {
        kind: PhaseKind::StructuralAnalysis,
        stability: Stability::Developmental,
        public_contract: false,
        note: "causalization, matching and tearing",
    },
    PhaseRecord {
        kind: PhaseKind::Simulation,
        stability: Stability::Developmental,
        public_contract: false,
        note: "numerical simulation execution",
    },
    PhaseRecord {
        kind: PhaseKind::Templates,
        stability: Stability::Experimental,
        public_contract: false,
        note: "template/modifier expansion",
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
