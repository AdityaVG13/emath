//! Lab manifest data model: partitions, metrics, kill/fallback plans.

use super::*;

/// Workload partition role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PartitionKind {
    /// Training corpus (parameter search only, never decision).
    Training,
    /// Calibration corpus (tuning, never decision).
    Calibration,
    /// Validation corpus (first decision evidence).
    Validation,
    /// Holdout corpus (final decision evidence).
    Holdout,
    /// Stress corpus (limits, adversarial shapes).
    Stress,
}

impl PartitionKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Training => "training",
            Self::Calibration => "calibration",
            Self::Validation => "validation",
            Self::Holdout => "holdout",
            Self::Stress => "stress",
        }
    }

    /// Experiment-protocol stage (A–E) occupied by this partition.
    #[must_use]
    pub const fn stage(self) -> char {
        match self {
            Self::Training => 'A',
            Self::Calibration => 'B',
            Self::Validation => 'C',
            Self::Holdout => 'D',
            Self::Stress => 'E',
        }
    }
}

/// One frozen workload partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusPartition {
    /// Partition name (unique within the manifest).
    pub name: String,
    /// Partition role.
    pub kind: PartitionKind,
    /// Operations the partition represents (frozen count).
    pub operations: u64,
    /// Fingerprint of the partition content.
    pub fingerprint: ContentId,
}

/// Frozen build/run environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentPin {
    /// Rust toolchain channel (`stable`, `1.74.0`, ...).
    pub toolchain: String,
    /// Target triple.
    pub target_triple: String,
    /// Sorted feature list.
    pub features: Vec<String>,
    /// Host description (os/cpu).
    pub host: String,
}

impl EnvironmentPin {
    /// Deterministic environment token.
    #[must_use]
    pub fn token(&self) -> String {
        // Features are a set-like collection: sort before hashing so the
        // token is invariant under feature order.
        let mut features = self.features.clone();
        features.sort();
        format!(
            "{}@{}#{}:{}",
            self.toolchain,
            self.target_triple,
            features.join("+"),
            self.host
        )
    }
}

/// One frozen artifact under evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRef {
    /// Semantic package name.
    pub package: String,
    /// Content identity of the sealed artifact.
    pub content_id: ContentId,
    /// Crate profile used.
    pub profile: String,
}

impl ArtifactRef {
    /// Deterministic artifact token.
    #[must_use]
    pub fn token(&self) -> String {
        format!("{}@{}:{}", self.package, self.content_id.0, self.profile)
    }
}

/// Metric optimisation direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricDirection {
    /// Lower values are better (latency, memory, size, cost).
    LowerIsBetter,
    /// Higher values are better (throughput).
    HigherIsBetter,
}

impl MetricDirection {
    /// Stable direction token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LowerIsBetter => "lower",
            Self::HigherIsBetter => "higher",
        }
    }
}

/// Frozen metric definition.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricSpec {
    /// Stable metric id (unique within the manifest).
    pub id: String,
    /// Measurement kind token (latency, throughput, allocations, ...).
    pub kind: String,
    /// Unit (`ns`, `ops/s`, `bytes`, `joules`, ...).
    pub unit: String,
    /// Optimisation direction.
    pub direction: MetricDirection,
    /// Decision weight (positive; higher = more important).
    pub weight: f64,
}

/// Frozen correctness/performance thresholds.
#[derive(Clone, Debug, PartialEq)]
pub struct Thresholds {
    /// Worst acceptable candidate/baseline median ratio.
    pub max_median_regression: f64,
    /// Worst acceptable p99 regression ratio.
    pub max_p99_regression: f64,
    /// Worst acceptable peak-memory ratio.
    pub max_memory_regression: f64,
    /// Minimum correctness rate in `(0.0, 1.0]`.
    pub min_correctness_rate: f64,
    /// Energy budget in joules (`None` = unbounded).
    pub energy_budget_joules: Option<f64>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_median_regression: 0.95,
            max_p99_regression: 1.10,
            max_memory_regression: 1.10,
            min_correctness_rate: 1.0,
            energy_budget_joules: None,
        }
    }
}

/// Kill-rule trigger condition.
#[derive(Clone, Debug, PartialEq)]
pub enum KillCondition {
    /// Any correctness failure kills the run.
    CorrectnessFailure,
    /// Evidence missing for a declared metric.
    EvidenceMissing,
    /// Median ratio fell below the given candidate/baseline bound.
    RegressionBelow { median_ratio: f64 },
    /// Peak memory exceeded the given bytes.
    MemoryOver { bytes: u64 },
    /// Run duration exceeded the given seconds.
    HangOver { seconds: u64 },
}

/// Kill-rule outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillAction {
    /// Cancel the run, keep the incumbent.
    DeclareIncumbent,
    /// Cancel the run and continue debugging.
    CancelRun,
    /// Cancel the run and quarantine the candidate.
    QuarantineCandidate,
}

impl KillAction {
    /// Stable action token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeclareIncumbent => "declare_incumbent",
            Self::CancelRun => "cancel_run",
            Self::QuarantineCandidate => "quarantine_candidate",
        }
    }
}

/// One frozen kill rule.
#[derive(Clone, Debug, PartialEq)]
pub struct KillRule {
    /// Rule id (unique within the manifest).
    pub id: String,
    /// Trigger condition.
    pub condition: KillCondition,
    /// Action on trigger.
    pub action: KillAction,
}

/// Fallback behaviour when a stage fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackAction {
    /// Keep the baseline routed; candidate stays out.
    RetainBaseline,
    /// Shadow the candidate (measure only, never serve).
    ShadowCandidate,
    /// Quarantine the candidate for diagnostics.
    QuarantineCandidate,
    /// Emit an explicit diagnostic and stop automation.
    ExplicitDiagnostic,
}

impl FallbackAction {
    /// Stable action token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RetainBaseline => "retain_baseline",
            Self::ShadowCandidate => "shadow_candidate",
            Self::QuarantineCandidate => "quarantine_candidate",
            Self::ExplicitDiagnostic => "diagnostic",
        }
    }
}

/// Frozen per-stage fallback plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallbackPlan {
    /// Behaviour when the quality gate fails.
    pub on_gate_failure: FallbackAction,
    /// Behaviour when a metric regresses.
    pub on_regression: FallbackAction,
    /// Behaviour when measurement itself fails.
    pub on_measurement_failure: FallbackAction,
}

/// Frozen experiment manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct LabManifest {
    /// Schema token (`lab`).
    pub schema: String,
    /// Experiment identity.
    pub experiment_id: ContentId,
    /// Baseline artifact.
    pub baseline: ArtifactRef,
    /// Candidate artifact.
    pub candidate: ArtifactRef,
    /// Frozen corpus partitions.
    pub partitions: Vec<CorpusPartition>,
    /// Frozen metric set.
    pub metrics: Vec<MetricSpec>,
    /// Frozen thresholds.
    pub thresholds: Thresholds,
    /// Frozen kill rules.
    pub kill_rules: Vec<KillRule>,
    /// Frozen fallback plan.
    pub fallback: FallbackPlan,
    /// Frozen environment.
    pub environment: EnvironmentPin,
    /// Candidate generator identity (rewrite family, search method, …).
    pub generator: String,
    /// Deterministic campaign seed (frozen before measurement).
    pub seed: u64,
    /// Whether the manifest is frozen (dedicated decision gate).
    pub frozen: bool,
}

/// Manifest validation problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabProblem {
    /// Stable code (`E-HOST-003`/`E-HOST-004`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}
