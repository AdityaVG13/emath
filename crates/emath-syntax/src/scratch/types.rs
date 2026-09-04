//! Public scratch-expansion types: levels, outcomes, holes, worlds.

use super::*;

/// Progressive-exactness level of the surface that was expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchLevel {
    /// Bare expression or intent verb.
    L0,
    /// Named relationship plus optional `example` bindings.
    L1,
    /// `emath function Name:` (or model/policy) without required L3 sections.
    L2,
    /// Already a contracted declaration; expansion is identity.
    Canonical,
}

impl ScratchLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::Canonical => "canonical",
        }
    }
}

/// Level a successful rewrite may occupy. Canonical is identity, not a rewrite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchRewriteLevel {
    L0,
    L1,
    L2,
}

impl ScratchRewriteLevel {
    #[must_use]
    pub fn as_scratch_level(self) -> ScratchLevel {
        match self {
            Self::L0 => ScratchLevel::L0,
            Self::L1 => ScratchLevel::L1,
            Self::L2 => ScratchLevel::L2,
        }
    }
}

/// How scratch expansion concluded. A rewrite cannot be Canonical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionOutcome {
    Identity,
    Rewritten { level: ScratchRewriteLevel },
    Refused { level: ScratchLevel },
}

impl ExpansionOutcome {
    #[must_use]
    pub fn rewritten(self) -> bool {
        matches!(self, Self::Rewritten { .. })
    }

    #[must_use]
    pub fn level(self) -> ScratchLevel {
        match self {
            Self::Identity => ScratchLevel::Canonical,
            Self::Rewritten { level } => level.as_scratch_level(),
            Self::Refused { level } => level,
        }
    }
}

/// One inferred default recorded so the expansion is inspectable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScratchNote {
    pub inferred: String,
    pub rationale: String,
    pub replacement: String,
    pub stability: ExactnessStatus,
}

/// Symbolic or numeric labeled hole candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleKind {
    Symbolic,
    Numeric,
}

impl HoleKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }
}

/// Labeled candidate for a typed hole. Labels alternatives; never a filled-in solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleCandidate {
    pub label: String,
    pub kind: HoleKind,
}

/// An attempt that was considered and refused for a hole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleRejection {
    pub attempt: String,
    pub reason: String,
}

/// What happens next for an open hole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleContinuation {
    /// Meaning stays open; freeze must not claim exactness.
    Open,
    /// `find <name>` recorded a search goal over the hole.
    Search { goal: String },
}

impl HoleContinuation {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Search { .. } => "search",
        }
    }
}

/// Durable typed-hole object: constraints, labeled candidates, rejections, continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoleRecord {
    pub name: String,
    pub constraints: Vec<String>,
    pub candidates: Vec<HoleCandidate>,
    pub rejections: Vec<HoleRejection>,
    pub continuation: HoleContinuation,
}

impl HoleRecord {
    #[must_use]
    pub fn open(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            constraints: Vec::new(),
            candidates: Vec::new(),
            rejections: Vec::new(),
            continuation: HoleContinuation::Open,
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "hole {} constraints={} candidates={} rejections={} continuation={}",
            self.name,
            self.constraints.len(),
            self.candidates.len(),
            self.rejections.len(),
            self.continuation.as_str()
        )
    }
}

/// Closed set of labeled `solve` worlds. The menu is these five rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveWorld {
    RealPm,
    Complex,
    Modular,
    Symbolic,
    Numeric,
}

impl SolveWorld {
    pub const ALL: [Self; 5] = [
        Self::RealPm,
        Self::Complex,
        Self::Modular,
        Self::Symbolic,
        Self::Numeric,
    ];

    #[must_use]
    pub fn parse_label(label: &str) -> Option<Self> {
        match label {
            "real" | "real-pm" | "ℝ" => Some(Self::RealPm),
            "complex" => Some(Self::Complex),
            "modular" => Some(Self::Modular),
            "symbolic" => Some(Self::Symbolic),
            "numeric" => Some(Self::Numeric),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RealPm => "real-pm",
            Self::Complex => "complex",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn result_type(self) -> &'static str {
        match self {
            Self::RealPm => "Real",
            Self::Complex => "Complex",
            Self::Modular => "Int",
            Self::Symbolic => "expression",
            Self::Numeric => "Float64",
        }
    }

    #[must_use]
    pub fn domain(self) -> &'static str {
        match self {
            Self::RealPm => "Real",
            Self::Complex => "Complex",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn exactness(self) -> &'static str {
        match self {
            Self::RealPm | Self::Complex => "exact-algebraic",
            Self::Modular => "exact",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric-tolerance",
        }
    }

    #[must_use]
    pub fn method(self) -> &'static str {
        match self {
            Self::RealPm | Self::Complex => "algebraic",
            Self::Modular => "modular",
            Self::Symbolic => "symbolic",
            Self::Numeric => "numeric",
        }
    }

    #[must_use]
    pub fn evidence_class(self) -> &'static str {
        match self {
            Self::Symbolic => "identity",
            Self::RealPm | Self::Complex | Self::Modular | Self::Numeric => "residual",
        }
    }

    #[must_use]
    pub fn holes(self) -> &'static [&'static str] {
        match self {
            Self::Modular => &["modulus"],
            Self::Numeric => &["tolerance"],
            Self::RealPm | Self::Complex | Self::Symbolic => &[],
        }
    }

    #[must_use]
    pub fn beginner_default(self) -> bool {
        matches!(self, Self::RealPm)
    }

    #[must_use]
    pub fn pin_phrase(self) -> &'static str {
        match self {
            Self::RealPm => "over Real",
            Self::Complex => "over Complex",
            Self::Modular => "over modular",
            Self::Symbolic => "over symbolic",
            Self::Numeric => "over numeric",
        }
    }
}

/// At most one labeled `solve` world. Two worlds selected is unrepresentable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SolveIntent {
    #[default]
    Absent,
    Unlabeled,
    Over(SolveWorld),
}

impl SolveIntent {
    #[must_use]
    pub fn selected(self, world: SolveWorld) -> bool {
        matches!(self, Self::Over(w) if w == world)
    }

    #[must_use]
    pub fn menu(self) -> &'static [SolveWorld] {
        match self {
            Self::Absent => &[],
            Self::Unlabeled | Self::Over(_) => &SolveWorld::ALL,
        }
    }
}

/// Result of official scratch / L2 expansion.
#[derive(Debug)]
pub struct ScratchExpansion {
    pub expanded: String,
    pub outcome: ExpansionOutcome,
    pub notes: Vec<ScratchNote>,
    pub holes: Vec<HoleRecord>,
    pub solve: SolveIntent,
    pub diagnostics: Diagnostics,
}

impl ScratchExpansion {
    /// Display/JSON: true iff [`ExpansionOutcome::Rewritten`]. Never Canonical.
    #[must_use]
    pub fn rewritten(&self) -> bool {
        self.outcome.rewritten()
    }

    #[must_use]
    pub fn level(&self) -> ScratchLevel {
        self.outcome.level()
    }

    /// Source the parser should read: expanded text when the rewrite is clean.
    #[must_use]
    pub fn parse_source<'a>(&'a self, original: &'a str) -> &'a str {
        match self.outcome {
            ExpansionOutcome::Rewritten { .. } if !self.diagnostics.has_errors() => {
                self.expanded.as_str()
            }
            _ => original,
        }
    }
}
