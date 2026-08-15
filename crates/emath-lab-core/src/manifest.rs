//!: frozen experiment manifest.
//!
//! Freezes workload corpus partitions, environment, baseline/candidate
//! artifact identities, metric set, statistical protocol, thresholds,
//! kill rules and fallback behaviour before any measurement or promotion
//! decision. The manifest self-validates (`E-HOST-003`/`E-HOST-004`),
//! has a versioned canonical encoding (`lab:v1:...`) for identity and a
//! deterministic canonical JSON form for audit/receipt replay.

use crate::error::LabError;
use crate::json::{self, JsonValue};
use crate::stats::StatisticalProtocol;
use emath_core::{fnv1a64_bytes, ContentId};

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
        format!(
            "{}@{}#{}:{}",
            self.toolchain,
            self.target_triple,
            self.features.join("+"),
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

/// Frozen metric definition for an experiment.
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
    /// Schema token (`lab:v1`).
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

impl LabManifest {
    /// Validates the manifest; every problem carries a stable code.
    #[must_use]
    pub fn validate(&self) -> Vec<LabProblem> {
        let mut problems = Vec::new();
        if self.schema != "lab:v1" {
            problems.push(problem("E-HOST-003", "schema must be lab:v1"));
        }
        if self.partitions.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires at least one partition",
            ));
        }
        let mut partition_names: Vec<&str> = self
            .partitions
            .iter()
            .map(|part| part.name.as_str())
            .collect();
        partition_names.sort_unstable();
        if partition_names.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(problem("E-HOST-003", "duplicate partition name"));
        }
        if self.metrics.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires at least one metric",
            ));
        }
        let mut metric_ids: Vec<&str> = self
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect();
        metric_ids.sort_unstable();
        if metric_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(problem("E-HOST-003", "duplicate metric id"));
        }
        if self
            .metrics
            .iter()
            .any(|metric| metric.weight <= 0.0 || !metric.weight.is_finite())
        {
            problems.push(problem(
                "E-HOST-003",
                "metric weight must be positive and finite",
            ));
        }
        let thresholds = &self.thresholds;
        if !thresholds.max_median_regression.is_finite() || thresholds.max_median_regression <= 0.0
        {
            problems.push(problem(
                "E-HOST-003",
                "max_median_regression must be positive",
            ));
        }
        if !thresholds.max_p99_regression.is_finite() || thresholds.max_p99_regression <= 0.0 {
            problems.push(problem("E-HOST-003", "max_p99_regression must be positive"));
        }
        if !thresholds.max_memory_regression.is_finite() || thresholds.max_memory_regression <= 0.0
        {
            problems.push(problem(
                "E-HOST-003",
                "max_memory_regression must be positive",
            ));
        }
        if !thresholds.min_correctness_rate.is_finite()
            || thresholds.min_correctness_rate <= 0.0
            || thresholds.min_correctness_rate > 1.0
        {
            problems.push(problem(
                "E-HOST-003",
                "min_correctness_rate must be in (0.0, 1.0]",
            ));
        }
        if thresholds
            .energy_budget_joules
            .is_some_and(|budget| budget <= 0.0 || !budget.is_finite())
        {
            problems.push(problem(
                "E-HOST-003",
                "energy budget must be positive when present",
            ));
        }
        let mut rule_ids: Vec<&str> = self
            .kill_rules
            .iter()
            .map(|rule| rule.id.as_str())
            .collect();
        rule_ids.sort_unstable();
        if rule_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(problem("E-HOST-003", "duplicate kill rule id"));
        }
        if !self.frozen {
            problems.push(problem(
                "E-HOST-004",
                "experiment manifest must be frozen before measurement",
            ));
        }
        if self.baseline.content_id == self.candidate.content_id {
            problems.push(problem(
                "E-HOST-004",
                "baseline and candidate must be distinct artifacts",
            ));
        }
        problems
    }

    /// Versioned canonical encoding (`lab:v1:...`); identity input.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut partitions: Vec<&CorpusPartition> = self.partitions.iter().collect();
        partitions.sort_by(|left, right| left.name.cmp(&right.name));
        let partition_token: Vec<String> = partitions
            .iter()
            .map(|part| {
                format!(
                    "{}:{}:{}:{}",
                    part.name,
                    part.kind.as_str(),
                    part.operations,
                    part.fingerprint.0
                )
            })
            .collect();
        let mut metrics: Vec<&MetricSpec> = self.metrics.iter().collect();
        metrics.sort_by(|left, right| left.id.cmp(&right.id));
        let metric_token: Vec<String> = metrics
            .iter()
            .map(|metric| {
                format!(
                    "{}:{}:{}:{}:{}",
                    metric.id,
                    metric.kind,
                    metric.unit,
                    metric.direction.as_str(),
                    metric.weight
                )
            })
            .collect();
        let mut rules: Vec<&KillRule> = self.kill_rules.iter().collect();
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        let rule_token: Vec<String> = rules
            .iter()
            .map(|rule| {
                format!(
                    "{}:{}:{}",
                    rule.id,
                    kill_condition_token(&rule.condition),
                    rule.action.as_str()
                )
            })
            .collect();
        let thresholds = &self.thresholds;
        format!(
            "lab:v1:{}:{}:{}:{}:{}:[{}]:[{}]:[{}]:{}:{}:{}:{}",
            self.experiment_id.0,
            if self.frozen { "frozen" } else { "draft" },
            self.baseline.token(),
            self.candidate.token(),
            self.environment.token(),
            partition_token.join(";"),
            metric_token.join(";"),
            rule_token.join(";"),
            thresholds.max_median_regression,
            thresholds.max_p99_regression,
            thresholds.max_memory_regression,
            thresholds.min_correctness_rate,
        )
    }

    /// FNV-1a64 identity of the canonical form.
    #[must_use]
    pub fn identity(&self) -> ContentId {
        ContentId(format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(self.canonical().as_bytes())
        ))
    }

    /// Deterministic canonical JSON (keys sorted, arrays ordered).
    #[must_use]
    pub fn to_json(&self) -> String {
        json::write(&JsonValue::Object(vec![
            ("schema".into(), JsonValue::String(self.schema.clone())),
            (
                "experiment_id".into(),
                JsonValue::String(self.experiment_id.0.clone()),
            ),
            ("frozen".into(), JsonValue::Bool(self.frozen)),
            ("baseline".into(), artifact_json(&self.baseline)),
            ("candidate".into(), artifact_json(&self.candidate)),
            (
                "partitions".into(),
                JsonValue::Array(
                    self.partitions
                        .iter()
                        .map(|part| {
                            JsonValue::Object(vec![
                                ("name".into(), JsonValue::String(part.name.clone())),
                                ("kind".into(), JsonValue::String(part.kind.as_str().into())),
                                ("operations".into(), json_count(part.operations)),
                                (
                                    "fingerprint".into(),
                                    JsonValue::String(part.fingerprint.0.clone()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "metrics".into(),
                JsonValue::Array(
                    self.metrics
                        .iter()
                        .map(|metric| {
                            JsonValue::Object(vec![
                                ("id".into(), JsonValue::String(metric.id.clone())),
                                ("kind".into(), JsonValue::String(metric.kind.clone())),
                                ("unit".into(), JsonValue::String(metric.unit.clone())),
                                (
                                    "direction".into(),
                                    JsonValue::String(metric.direction.as_str().into()),
                                ),
                                ("weight".into(), JsonValue::Number(metric.weight)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "thresholds".into(),
                JsonValue::Object(vec![
                    (
                        "max_median_regression".into(),
                        JsonValue::Number(self.thresholds.max_median_regression),
                    ),
                    (
                        "max_p99_regression".into(),
                        JsonValue::Number(self.thresholds.max_p99_regression),
                    ),
                    (
                        "max_memory_regression".into(),
                        JsonValue::Number(self.thresholds.max_memory_regression),
                    ),
                    (
                        "min_correctness_rate".into(),
                        JsonValue::Number(self.thresholds.min_correctness_rate),
                    ),
                    (
                        "energy_budget_joules".into(),
                        self.thresholds
                            .energy_budget_joules
                            .map_or(JsonValue::Null, JsonValue::Number),
                    ),
                ]),
            ),
            (
                "kill_rules".into(),
                JsonValue::Array(
                    self.kill_rules
                        .iter()
                        .map(|rule| {
                            let mut fields = vec![
                                ("id".into(), JsonValue::String(rule.id.clone())),
                                ("condition".into(), kill_condition_json(&rule.condition)),
                                (
                                    "action".into(),
                                    JsonValue::String(rule.action.as_str().into()),
                                ),
                            ];
                            fields.sort_by(
                                |left: &(String, JsonValue), right: &(String, JsonValue)| {
                                    left.0.cmp(&right.0)
                                },
                            );
                            JsonValue::Object(fields)
                        })
                        .collect(),
                ),
            ),
            (
                "fallback".into(),
                JsonValue::Object(vec![
                    (
                        "on_gate_failure".into(),
                        JsonValue::String(self.fallback.on_gate_failure.as_str().into()),
                    ),
                    (
                        "on_regression".into(),
                        JsonValue::String(self.fallback.on_regression.as_str().into()),
                    ),
                    (
                        "on_measurement_failure".into(),
                        JsonValue::String(self.fallback.on_measurement_failure.as_str().into()),
                    ),
                ]),
            ),
            (
                "environment".into(),
                JsonValue::Object(vec![
                    (
                        "toolchain".into(),
                        JsonValue::String(self.environment.toolchain.clone()),
                    ),
                    (
                        "target_triple".into(),
                        JsonValue::String(self.environment.target_triple.clone()),
                    ),
                    (
                        "features".into(),
                        JsonValue::Array(
                            self.environment
                                .features
                                .iter()
                                .cloned()
                                .map(JsonValue::String)
                                .collect(),
                        ),
                    ),
                    (
                        "host".into(),
                        JsonValue::String(self.environment.host.clone()),
                    ),
                ]),
            ),
        ]))
    }

    /// Parses the canonical JSON back into a manifest (`E-HOST-003`).
    pub fn from_json(text: &str) -> Result<LabManifest, LabError> {
        let value = json::parse(text).map_err(|error| {
            LabError::new("E-HOST-003", format!("manifest JSON is invalid: {error}"))
        })?;
        let object = expect_object(&value, "manifest")?;
        let schema = expect_string(field(object, "schema")?, "schema")?.to_string();
        let experiment_id =
            ContentId(expect_string(field(object, "experiment_id")?, "experiment_id")?.to_string());
        let frozen = expect_bool(field(object, "frozen")?, "frozen")?;
        let baseline = artifact_from_json(field(object, "baseline")?, "baseline")?;
        let candidate = artifact_from_json(field(object, "candidate")?, "candidate")?;
        let partitions = expect_array(field(object, "partitions")?, "partitions")?
            .iter()
            .map(|entry| {
                let object = expect_object(entry, "partition")?;
                Ok(CorpusPartition {
                    name: expect_string(field(object, "name")?, "partition.name")?.to_string(),
                    kind: parse_partition_kind(expect_string(
                        field(object, "kind")?,
                        "partition.kind",
                    )?)?,
                    operations: expect_u64(field(object, "operations")?, "partition.operations")?,
                    fingerprint: ContentId(
                        expect_string(field(object, "fingerprint")?, "partition.fingerprint")?
                            .to_string(),
                    ),
                })
            })
            .collect::<Result<Vec<CorpusPartition>, LabError>>()?;
        let metrics = expect_array(field(object, "metrics")?, "metrics")?
            .iter()
            .map(|entry| {
                let object = expect_object(entry, "metric")?;
                Ok(MetricSpec {
                    id: expect_string(field(object, "id")?, "metric.id")?.to_string(),
                    kind: expect_string(field(object, "kind")?, "metric.kind")?.to_string(),
                    unit: expect_string(field(object, "unit")?, "metric.unit")?.to_string(),
                    direction: parse_direction(expect_string(
                        field(object, "direction")?,
                        "metric.direction",
                    )?)?,
                    weight: expect_number(field(object, "weight")?, "metric.weight")?,
                })
            })
            .collect::<Result<Vec<MetricSpec>, LabError>>()?;
        let thresholds_value = expect_object(field(object, "thresholds")?, "thresholds")?;
        let thresholds = Thresholds {
            max_median_regression: expect_number(
                field(thresholds_value, "max_median_regression")?,
                "thresholds.max_median_regression",
            )?,
            max_p99_regression: expect_number(
                field(thresholds_value, "max_p99_regression")?,
                "thresholds.max_p99_regression",
            )?,
            max_memory_regression: expect_number(
                field(thresholds_value, "max_memory_regression")?,
                "thresholds.max_memory_regression",
            )?,
            min_correctness_rate: expect_number(
                field(thresholds_value, "min_correctness_rate")?,
                "thresholds.min_correctness_rate",
            )?,
            energy_budget_joules: match field(thresholds_value, "energy_budget_joules")? {
                JsonValue::Null => None,
                other => Some(expect_number(other, "thresholds.energy_budget_joules")?),
            },
        };
        let kill_rules = expect_array(field(object, "kill_rules")?, "kill_rules")?
            .iter()
            .map(|entry| {
                let object = expect_object(entry, "kill_rule")?;
                Ok(KillRule {
                    id: expect_string(field(object, "id")?, "kill_rule.id")?.to_string(),
                    condition: kill_condition_from_json(field(object, "condition")?)?,
                    action: parse_kill_action(expect_string(
                        field(object, "action")?,
                        "kill_rule.action",
                    )?)?,
                })
            })
            .collect::<Result<Vec<KillRule>, LabError>>()?;
        let fallback_value = expect_object(field(object, "fallback")?, "fallback")?;
        let fallback = FallbackPlan {
            on_gate_failure: parse_fallback_action(expect_string(
                field(fallback_value, "on_gate_failure")?,
                "fallback.on_gate_failure",
            )?)?,
            on_regression: parse_fallback_action(expect_string(
                field(fallback_value, "on_regression")?,
                "fallback.on_regression",
            )?)?,
            on_measurement_failure: parse_fallback_action(expect_string(
                field(fallback_value, "on_measurement_failure")?,
                "fallback.on_measurement_failure",
            )?)?,
        };
        let environment_value = expect_object(field(object, "environment")?, "environment")?;
        let environment = EnvironmentPin {
            toolchain: expect_string(
                field(environment_value, "toolchain")?,
                "environment.toolchain",
            )?
            .to_string(),
            target_triple: expect_string(
                field(environment_value, "target_triple")?,
                "environment.target_triple",
            )?
            .to_string(),
            features: expect_array(
                field(environment_value, "features")?,
                "environment.features",
            )?
            .iter()
            .map(|entry| expect_string(entry, "environment.features[]").map(str::to_string))
            .collect::<Result<Vec<String>, LabError>>()?,
            host: expect_string(field(environment_value, "host")?, "environment.host")?.to_string(),
        };
        let manifest = LabManifest {
            schema,
            experiment_id,
            baseline,
            candidate,
            partitions,
            metrics,
            thresholds,
            kill_rules,
            fallback,
            environment,
            frozen,
        };
        if let Some(problem) = manifest.validate().into_iter().next() {
            return Err(LabError::new(problem.code, problem.message));
        }
        Ok(manifest)
    }
}

fn problem(code: &'static str, message: &str) -> LabProblem {
    LabProblem {
        code,
        message: message.to_string(),
    }
}

fn artifact_json(artifact: &ArtifactRef) -> JsonValue {
    JsonValue::Object(vec![
        (
            "package".into(),
            JsonValue::String(artifact.package.clone()),
        ),
        (
            "content_id".into(),
            JsonValue::String(artifact.content_id.0.clone()),
        ),
        (
            "profile".into(),
            JsonValue::String(artifact.profile.clone()),
        ),
    ])
}

fn artifact_from_json(value: &JsonValue, at: &str) -> Result<ArtifactRef, LabError> {
    let object = expect_object(value, at)?;
    Ok(ArtifactRef {
        package: expect_string(field(object, "package")?, &format!("{at}.package"))?.to_string(),
        content_id: ContentId(
            expect_string(field(object, "content_id")?, &format!("{at}.content_id"))?.to_string(),
        ),
        profile: expect_string(field(object, "profile")?, &format!("{at}.profile"))?.to_string(),
    })
}

fn field<'a>(object: &'a [(String, JsonValue)], name: &str) -> Result<&'a JsonValue, LabError> {
    object
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| {
            LabError::new(
                "E-HOST-003",
                format!("manifest JSON is missing field {name}"),
            )
        })
}

fn expect_object<'a>(
    value: &'a JsonValue,
    at: &str,
) -> Result<&'a [(String, JsonValue)], LabError> {
    match value {
        JsonValue::Object(fields) => Ok(fields),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON object"),
        )),
    }
}

fn expect_array<'a>(value: &'a JsonValue, at: &str) -> Result<&'a [JsonValue], LabError> {
    match value {
        JsonValue::Array(items) => Ok(items),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON array"),
        )),
    }
}

fn expect_string<'a>(value: &'a JsonValue, at: &str) -> Result<&'a str, LabError> {
    match value {
        JsonValue::String(text) => Ok(text),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON string"),
        )),
    }
}

fn expect_number(value: &JsonValue, at: &str) -> Result<f64, LabError> {
    match value {
        JsonValue::Number(number) => Ok(*number),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON number"),
        )),
    }
}

/// Non-negative integer field; rejects fractional, negative, NaN and
/// out-of-range values instead of silently truncating.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn expect_u64(value: &JsonValue, at: &str) -> Result<u64, LabError> {
    let number = expect_number(value, at)?;
    if number.is_finite() && number.fract() == 0.0 && number >= 0.0 && number < 2_f64.powi(64) {
        Ok(number as u64)
    } else {
        Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a non-negative integer"),
        ))
    }
}

/// Serializes an integer count as a JSON number; counts are practical
/// magnitudes, so the `u64 -> f64` conversion is exact in practice.
#[allow(clippy::cast_precision_loss)]
fn json_count(count: u64) -> JsonValue {
    JsonValue::Number(count as f64)
}

fn expect_bool(value: &JsonValue, at: &str) -> Result<bool, LabError> {
    match value {
        JsonValue::Bool(flag) => Ok(*flag),
        _ => Err(LabError::new(
            "E-HOST-003",
            format!("{at} must be a JSON boolean"),
        )),
    }
}

fn parse_partition_kind(token: &str) -> Result<PartitionKind, LabError> {
    match token {
        "training" => Ok(PartitionKind::Training),
        "calibration" => Ok(PartitionKind::Calibration),
        "validation" => Ok(PartitionKind::Validation),
        "holdout" => Ok(PartitionKind::Holdout),
        "stress" => Ok(PartitionKind::Stress),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown partition kind {other}"),
        )),
    }
}

fn parse_direction(token: &str) -> Result<MetricDirection, LabError> {
    match token {
        "lower" => Ok(MetricDirection::LowerIsBetter),
        "higher" => Ok(MetricDirection::HigherIsBetter),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown metric direction {other}"),
        )),
    }
}

fn parse_kill_action(token: &str) -> Result<KillAction, LabError> {
    match token {
        "declare_incumbent" => Ok(KillAction::DeclareIncumbent),
        "cancel_run" => Ok(KillAction::CancelRun),
        "quarantine_candidate" => Ok(KillAction::QuarantineCandidate),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown kill action {other}"),
        )),
    }
}

fn parse_fallback_action(token: &str) -> Result<FallbackAction, LabError> {
    match token {
        "retain_baseline" => Ok(FallbackAction::RetainBaseline),
        "shadow_candidate" => Ok(FallbackAction::ShadowCandidate),
        "quarantine_candidate" => Ok(FallbackAction::QuarantineCandidate),
        "diagnostic" => Ok(FallbackAction::ExplicitDiagnostic),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown fallback action {other}"),
        )),
    }
}

fn kill_condition_token(condition: &KillCondition) -> String {
    match condition {
        KillCondition::CorrectnessFailure => "correctness".to_string(),
        KillCondition::EvidenceMissing => "evidence_missing".to_string(),
        KillCondition::RegressionBelow { median_ratio } => {
            format!("regression_below:{median_ratio}")
        }
        KillCondition::MemoryOver { bytes } => format!("memory_over:{bytes}"),
        KillCondition::HangOver { seconds } => format!("hang_over:{seconds}"),
    }
}

fn kill_condition_json(condition: &KillCondition) -> JsonValue {
    match condition {
        KillCondition::CorrectnessFailure => JsonValue::Object(vec![(
            "kind".into(),
            JsonValue::String("correctness".into()),
        )]),
        KillCondition::EvidenceMissing => JsonValue::Object(vec![(
            "kind".into(),
            JsonValue::String("evidence_missing".into()),
        )]),
        KillCondition::RegressionBelow { median_ratio } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("regression_below".into())),
            ("median_ratio".into(), JsonValue::Number(*median_ratio)),
        ]),
        KillCondition::MemoryOver { bytes } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("memory_over".into())),
            ("bytes".into(), json_count(*bytes)),
        ]),
        KillCondition::HangOver { seconds } => JsonValue::Object(vec![
            ("kind".into(), JsonValue::String("hang_over".into())),
            ("seconds".into(), json_count(*seconds)),
        ]),
    }
}

fn kill_condition_from_json(value: &JsonValue) -> Result<KillCondition, LabError> {
    let object = expect_object(value, "kill_rule.condition")?;
    let kind = expect_string(field(object, "kind")?, "kill_rule.condition.kind")?;
    match kind {
        "correctness" => Ok(KillCondition::CorrectnessFailure),
        "evidence_missing" => Ok(KillCondition::EvidenceMissing),
        "regression_below" => Ok(KillCondition::RegressionBelow {
            median_ratio: expect_number(
                field(object, "median_ratio")?,
                "kill_rule.condition.median_ratio",
            )?,
        }),
        "memory_over" => Ok(KillCondition::MemoryOver {
            bytes: expect_u64(field(object, "bytes")?, "kill_rule.condition.bytes")?,
        }),
        "hang_over" => Ok(KillCondition::HangOver {
            seconds: expect_u64(field(object, "seconds")?, "kill_rule.condition.seconds")?,
        }),
        other => Err(LabError::new(
            "E-HOST-003",
            format!("unknown kill condition {other}"),
        )),
    }
}

/// Fills the manifest with the frozen statistical protocol reference.
/// The protocol itself lives in `crate::stats`; this helper is the manifest
/// side of wiring.
#[must_use]
pub fn protocol_token(protocol: &StatisticalProtocol) -> String {
    format!(
        "warm={}:reps={}:min={}:paired={}:seed={}:outlier={}:retain={}:randomize={}",
        protocol.warmup_repetitions,
        protocol.repetitions,
        protocol.min_repetitions,
        protocol.paired,
        protocol.seed,
        outlier_token(&protocol.outlier),
        protocol.retain_raw,
        protocol.randomize_order
    )
}

fn outlier_token(outlier: &crate::stats::OutlierPolicy) -> String {
    match outlier {
        crate::stats::OutlierPolicy::KeepAll => "keep-all".to_string(),
        crate::stats::OutlierPolicy::MadTrim { factor } => format!("mad-trim:{factor}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{OutlierPolicy, StatisticalProtocol};

    fn manifest() -> LabManifest {
        LabManifest {
            schema: "lab:v1".into(),
            experiment_id: ContentId("exp-01".into()),
            baseline: ArtifactRef {
                package: "emath.score".into(),
                content_id: ContentId("base-hash".into()),
                profile: "release".into(),
            },
            candidate: ArtifactRef {
                package: "emath.score".into(),
                content_id: ContentId("cand-hash".into()),
                profile: "release".into(),
            },
            partitions: vec![CorpusPartition {
                name: "fp64-ops".into(),
                kind: PartitionKind::Holdout,
                operations: 1_000_000,
                fingerprint: ContentId("part-fp".into()),
            }],
            metrics: vec![MetricSpec {
                id: "latency".into(),
                kind: "latency".into(),
                unit: "ns".into(),
                direction: MetricDirection::LowerIsBetter,
                weight: 1.0,
            }],
            thresholds: Thresholds::default(),
            kill_rules: vec![],
            fallback: FallbackPlan {
                on_gate_failure: FallbackAction::RetainBaseline,
                on_regression: FallbackAction::ShadowCandidate,
                on_measurement_failure: FallbackAction::QuarantineCandidate,
            },
            environment: EnvironmentPin {
                toolchain: "1.74.0".into(),
                target_triple: "aarch64-apple-darwin".into(),
                features: vec!["fp64".into()],
                host: "macos-25/arm64".into(),
            },
            frozen: true,
        }
    }

    #[test]
    fn manifest_validates_clean_when_frozen() {
        assert!(manifest().validate().is_empty());
        assert!(manifest().canonical().starts_with("lab:v1:exp-01:frozen:"));
    }

    #[test]
    fn draft_and_identical_artifacts_are_refused() {
        let mut draft = manifest();
        draft.frozen = false;
        assert!(draft
            .validate()
            .iter()
            .any(|problem| problem.code == "E-HOST-004"));
        let mut identical = manifest();
        identical.candidate = identical.baseline.clone();
        assert!(identical
            .validate()
            .iter()
            .any(|problem| problem.code == "E-HOST-004"));
    }

    #[test]
    fn structural_problems_are_stable_codes() {
        let mut no_metrics = manifest();
        no_metrics.metrics.clear();
        assert!(no_metrics
            .validate()
            .iter()
            .any(|problem| problem.code == "E-HOST-003"));
        let mut bad_weight = manifest();
        bad_weight.metrics[0].weight = -1.0;
        assert!(bad_weight
            .validate()
            .iter()
            .any(|problem| problem.code == "E-HOST-003"));
        let mut dup = manifest();
        dup.partitions.push(dup.partitions[0].clone());
        assert!(dup
            .validate()
            .iter()
            .any(|problem| problem.code == "E-HOST-003"));
    }

    #[test]
    fn canonical_identity_is_stable_and_sensitive() {
        let first = manifest();
        let mut perturbed = manifest();
        perturbed.thresholds.max_p99_regression = 1.2;
        assert_eq!(first.identity(), first.identity());
        assert_ne!(first.identity(), perturbed.identity());
    }

    #[test]
    fn json_round_trip_preserves_manifest() {
        let original = manifest();
        let text = original.to_json();
        assert_eq!(json::write(&json::parse(&text).unwrap()), text);
        let parsed = LabManifest::from_json(&text).unwrap();
        assert_eq!(parsed.canonical(), original.canonical());
        assert_eq!(parsed.identity(), original.identity());
    }

    #[test]
    fn json_rejects_malformed_manifests() {
        assert!(LabManifest::from_json("{}").is_err());
        assert!(LabManifest::from_json("{\"schema\":\"other\"}").is_err());
        // Unfreezing the manifest in JSON is structurally valid but not
        // frozen, so from_json refills via validate() and refuses.
        let mut unfrozen = manifest();
        unfrozen.frozen = false;
        let text = unfrozen.to_json();
        let error = LabManifest::from_json(&text).unwrap_err();
        assert_eq!(error.code, "E-HOST-004");
        // Malformed JSON documents are refused with the manifest code.
        let error = LabManifest::from_json("{\"frozen\": tru").unwrap_err();
        assert_eq!(error.code, "E-HOST-003");
    }

    #[test]
    fn protocol_token_is_deterministic() {
        let protocol = StatisticalProtocol {
            warmup_repetitions: 2,
            repetitions: 10,
            min_repetitions: 5,
            paired: true,
            seed: 7,
            outlier: OutlierPolicy::MadTrim { factor: 3.0 },
            retain_raw: true,
            randomize_order: false,
        };
        assert_eq!(
            protocol_token(&protocol),
            "warm=2:reps=10:min=5:paired=true:seed=7:outlier=mad-trim:3:retain=true:randomize=false"
        );
        assert_eq!(protocol_token(&protocol), protocol_token(&protocol));
    }
}
