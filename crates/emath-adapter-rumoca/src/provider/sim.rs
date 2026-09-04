//! Simulation configuration and result types.

use super::*;

/// Provides a causal DAE plan for a structural model within a budget.
pub fn provide_dae_plan(model: &StructuralModel, budget: &Budget) -> Outcome<DaePlan, LowerError> {
    let evaluations = u64::try_from(model.equations.len()).unwrap_or(u64::MAX);
    let evidence = EvidenceHandle {
        schema: SchemaId("emath.structural-plan".into()),
        identity: ContentId("fnv1a64:0000000000000000".into()),
    };
    if evaluations > budget.evaluations {
        return Outcome::Unresolved {
            reason: UnresolvedReason::BudgetExhausted,
            partial: None,
            continuation: Some(ContinuationHandle {
                schema: SchemaId("emath.structural-plan".into()),
                identity: ContentId("fnv1a64:0000000000000000".into()),
                provider_id: "emath-native-causalizer".into(),
            }),
            evidence,
        };
    }
    let issues = model.validate();
    if !issues.is_empty() {
        return Outcome::Failed(LowerError {
            code: "E-PROV-237",
            message: format!(
                "provider model refused by validation gate: {} issue(s); first: {}: {}",
                issues.len(),
                issues[0].code,
                issues[0].message
            ),
        });
    }
    match crate::lower::lower(model) {
        Ok(plan) => {
            let identity = ContentId(format!("fnv1a64:{:016x}", plan.content_identity()));
            Outcome::Resolved {
                value: plan,
                evidence: EvidenceHandle {
                    schema: SchemaId("emath.structural-plan".into()),
                    identity,
                },
            }
        }
        Err(error) => Outcome::Failed(error),
    }
}

/// Deterministic simulation configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimulationConfig {
    /// Fixed time step (seconds).
    pub dt: f64,
    /// Number of steps.
    pub steps: u64,
    /// Whether a local truncation-error estimate is recorded.
    pub error_estimate: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            dt: 0.001,
            steps: 1000,
            error_estimate: true,
        }
    }
}

impl SimulationConfig {
    /// Horizon in seconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // steps bounded far below 2^53
    pub fn horizon(&self) -> f64 {
        self.dt * self.steps as f64
    }
}

/// One recorded trajectory point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimPoint {
    /// Time (s).
    pub t: f64,
    /// First state value (model state order; fixture: position).
    pub position: f64,
    /// Second state value (model state order; fixture: velocity).
    pub velocity: f64,
}

/// Deterministic simulation trace.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulationResult {
    /// Recorded points (initial point plus one per step).
    pub points: Vec<SimPoint>,
    /// Final position.
    pub final_position: f64,
    /// Final velocity.
    pub final_velocity: f64,
    /// Maximum estimated local truncation error.
    pub max_lte: f64,
    /// Steps executed.
    pub steps: u64,
    /// Termination disposition.
    pub termination: &'static str,
    pub(super) identity: u64,
}

impl SimulationResult {
    /// Deterministic canonical rendering (scientific notation, trajectory
    /// order preserved).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str("sim:{");
        for point in &self.points {
            let _ = std::fmt::write(
                &mut out,
                format_args!("{}:{:e}:{:e};", point.t, point.position, point.velocity),
            );
        }
        let _ = std::fmt::write(
            &mut out,
            format_args!(
                "}}final:{:e}:{:e}:lte:{:e}:steps:{}:term:{}",
                self.final_position,
                self.final_velocity,
                self.max_lte,
                self.steps,
                self.termination
            ),
        );
        out
    }

    /// FNV-1a64 identity over the canonical rendering.
    #[must_use]
    pub fn content_identity(&self) -> u64 {
        self.identity
    }
}

/// Simulation failure.
#[derive(Clone, Debug, PartialEq)]
pub struct SimError {
    /// Stable code (`E-PROV-230`..`E-PROV-235`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Derivative row retained in a generated simulation artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivativeMetadata {
    /// State variable.
    pub state: String,
    /// Canonical derivative identifier.
    pub derivative: String,
    /// Equation index in the flattened model.
    pub equation: usize,
    /// Original component/source anchor.
    pub origin: String,
}

/// Deterministic, runnable Rust-side simulation component.
///
/// The generated Rust source is the portable component representation.
/// `run` executes the same flattened plan in-process, preserving the runtime
/// budget and typed-outcome contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationArtifact {
    /// Neutral flattened model.
    pub model: StructuralModel,
    /// Causal DAE plan consumed by the component.
    pub plan: DaePlan,
    /// State-to-equation derivative mapping with source anchors.
    pub derivatives: Vec<DerivativeMetadata>,
    /// Byte-deterministic, safe Rust component source.
    pub rust_source: String,
    pub(super) identity: u64,
}

impl SimulationArtifact {
    /// Runs the component with exact parameters and a bounded configuration.
    pub fn run(
        &self,
        parameters: &BTreeMap<String, f64>,
        config: &SimulationConfig,
        budget: &Budget,
    ) -> Outcome<SimulationResult, SimError> {
        simulate(&self.model, &self.plan, parameters, config, budget)
    }

    /// Canonical artifact rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        artifact_canonical(
            &self.model.canonical(),
            &self.plan.canonical(),
            &derivatives_canonical(&self.derivatives),
            &self.rust_source,
        )
    }

    /// FNV-1a64 identity over the complete canonical artifact.
    #[must_use]
    pub fn content_identity(&self) -> u64 {
        self.identity
    }
}

/// Simulation-artifact construction failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactError {
    /// Stable code (`E-PROV-220..223`, `E-PROV-237`, or `E-PROV-239`).
    pub code: &'static str,
    /// Actionable refusal detail.
    pub message: String,
}
