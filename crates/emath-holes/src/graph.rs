//! Deterministic meaning-hole graph (spec 8).

use emath_world_ir::fnv1a64;

/// Hole kinds (spec 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeaningHoleKind {
    /// How the source is encoded.
    Encoding,
    /// Token boundary ambiguity.
    TokenBoundary,
    /// Fixity of an operator.
    Fixity,
    /// Precedence of an operator.
    Precedence,
    /// Associativity of an operator.
    Associativity,
    /// Arity of an operator.
    Arity,
    /// Carrier/domain choice.
    Carrier,
    /// Type of a symbol.
    Type,
    /// Operator definition.
    OperatorDefinition,
    /// Constant definition.
    ConstantDefinition,
    /// Constructor rule.
    Constructor,
    /// Law shape.
    Law,
    /// Variable value.
    VariableValue,
    /// Goal shape.
    Goal,
    /// Provider selection.
    Provider,
    /// Evidence requirement.
    Evidence,
}

impl MeaningHoleKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Encoding => "encoding",
            Self::TokenBoundary => "token-boundary",
            Self::Fixity => "fixity",
            Self::Precedence => "precedence",
            Self::Associativity => "associativity",
            Self::Arity => "arity",
            Self::Carrier => "carrier",
            Self::Type => "type",
            Self::OperatorDefinition => "operator-definition",
            Self::ConstantDefinition => "constant-definition",
            Self::Constructor => "constructor",
            Self::Law => "law",
            Self::VariableValue => "variable-value",
            Self::Goal => "goal",
            Self::Provider => "provider",
            Self::Evidence => "evidence",
        }
    }
}

/// Hole states (spec 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HoleState {
    /// Not yet addressed.
    Open,
    /// A candidate value exists but has not been admitted.
    Proposed,
    /// Admitted; the hole is closed by a continuation.
    Solved,
    /// Multiple admitted candidates remain; not collapsed silently.
    Ambiguous,
    /// Constraints exclude every candidate.
    Contradictory,
    /// Deferred by policy.
    Deferred,
    /// The solver budget was exhausted before admission.
    BudgetExhausted,
}

impl HoleState {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Proposed => "proposed",
            Self::Solved => "solved",
            Self::Ambiguous => "ambiguous",
            Self::Contradictory => "contradictory",
            Self::Deferred => "deferred",
            Self::BudgetExhausted => "budget-exhausted",
        }
    }
}

/// One meaning hole with its full contract (spec 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningHole {
    /// Stable deterministic identity: never changes when the hole state
    /// or budget is updated (non-destructive refinement).
    pub id: u64,
    /// Hole kind.
    pub kind: MeaningHoleKind,
    /// Source span or derived origin.
    pub origin: String,
    /// Constraints, in declaration order (canonical texts).
    pub constraints: Vec<String>,
    /// Candidate values, sorted.
    pub candidate_values: Vec<String>,
    /// Dependency hole ids (prerequisites), sorted.
    pub dependencies: Vec<u64>,
    /// Budget consumed so far.
    pub budget_consumed: u64,
    /// Current state.
    pub state: HoleState,
    /// Solver-provided status text (receipt note), empty by default.
    pub status: String,
}

impl MeaningHole {
    /// Builds a hole with a stable deterministic identity; collections
    /// are sorted internally.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: MeaningHoleKind,
        origin: impl Into<String>,
        constraints: Vec<String>,
        mut candidate_values: Vec<String>,
        mut dependencies: Vec<u64>,
    ) -> Self {
        candidate_values.sort();
        dependencies.sort_unstable();
        let origin = origin.into();
        let stable = format!(
            "hole:{}:{}:{}:{}:{}",
            kind.canonical(),
            origin,
            constraints.join("|"),
            candidate_values.join(","),
            dependencies
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        Self {
            id: fnv1a64(stable.as_bytes()),
            kind,
            origin,
            constraints,
            candidate_values,
            dependencies,
            budget_consumed: 0,
            state: HoleState::Open,
            status: String::new(),
        }
    }

    /// Deterministic canonical form of the full contract (identity
    /// excluded; state and budget included for display).
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.kind.canonical(),
            self.origin,
            self.constraints.join("|"),
            self.candidate_values.join(","),
            self.dependencies
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.budget_consumed,
            self.state.canonical(),
        )
    }
}

/// Deterministic meaning-hole graph (spec 8 "hole graph").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleGraph {
    holes: Vec<MeaningHole>,
    /// Deterministic graph identity.
    pub id: u64,
}

impl HoleGraph {
    /// Builds a graph; holes are sorted by id (input order does not
    /// matter). The graph identity is a full-state fingerprint: hole id,
    /// state, budget, and status. Hole ids themselves stay stable, so a
    /// continuation produces a new graph identity without re-identifying
    /// any hole.
    #[must_use]
    pub fn new(mut holes: Vec<MeaningHole>) -> Self {
        holes.sort_by_key(|hole| hole.id);
        let id = fnv1a64(
            holes
                .iter()
                .map(|hole| {
                    format!(
                        "{}:{}:{}:{}",
                        hole.id,
                        hole.state.canonical(),
                        hole.budget_consumed,
                        hole.status
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
                .as_bytes(),
        );
        Self { holes, id }
    }

    /// Holes sorted by id.
    #[must_use]
    pub fn holes(&self) -> &[MeaningHole] {
        &self.holes
    }

    /// A hole by id.
    #[must_use]
    pub fn hole(&self, id: u64) -> Option<&MeaningHole> {
        self.holes.iter().find(|hole| hole.id == id)
    }

    /// Dependency (prerequisite) ids of a hole.
    #[must_use]
    pub fn dependencies(&self, id: u64) -> Vec<u64> {
        self.hole(id)
            .map_or_else(Vec::new, |hole| hole.dependencies.clone())
    }

    /// Dependents of a hole: holes that list it as a prerequisite.
    #[must_use]
    pub fn dependents(&self, id: u64) -> Vec<u64> {
        self.holes
            .iter()
            .filter(|hole| hole.dependencies.contains(&id))
            .map(|hole| hole.id)
            .collect()
    }

    /// Continuation step: returns a new graph with `hole_id` updated to
    /// `state` with `extra_budget` added and `status` recorded. The
    /// current graph is never mutated (non-destructive refinement,
    /// spec 8); the hole id stays stable.
    #[must_use]
    pub fn with_updated(
        &self,
        hole_id: u64,
        state: HoleState,
        extra_budget: u64,
        status: impl Into<String>,
    ) -> Self {
        let status = status.into();
        let holes = self
            .holes
            .iter()
            .map(|hole| {
                if hole.id != hole_id {
                    return hole.clone();
                }
                let mut updated = hole.clone();
                updated.state = state;
                updated.budget_consumed = hole.budget_consumed.saturating_add(extra_budget);
                updated.status.clone_from(&status);
                updated
            })
            .collect();
        Self::new(holes)
    }
}
