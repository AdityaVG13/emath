//! Manifest parsing and validation.

use super::{LabManifest, LabProblem, problem, CorpusPartition, MetricSpec, KillRule, kill_condition_token, ContentId, fnv1a64_bytes, json, JsonValue, artifact_json, json_count, kill_condition_json, LabError, expect_object, owned_string_field, bool_field, artifact_from_json, field, u64_field, array_field, object_field, string_field, parse_partition_kind, expect_u64, parse_direction, number_field, Thresholds, optional_number_field, kill_condition_from_json, parse_kill_action, FallbackPlan, FallbackAction, parse_fallback_action, EnvironmentPin, expect_string};

impl LabManifest {
        /// Validates; every problem carries a stable code.
    #[must_use]
    pub fn validate(&self) -> Vec<LabProblem> {
        let mut problems = Vec::new();
        self.validate_identity(&mut problems);
        self.validate_partitions(&mut problems);
        self.validate_metrics(&mut problems);
        self.validate_thresholds(&mut problems);
        self.validate_rules(&mut problems);
        problems
    }

    fn validate_identity(&self, problems: &mut Vec<LabProblem>) {
        if self.schema != "lab" {
            problems.push(problem("E-HOST-003", "schema must be lab"));
        }
        if self.generator.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires a generator identity",
            ));
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
    }

    fn validate_partitions(&self, problems: &mut Vec<LabProblem>) {
        if self.partitions.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires at least one partition",
            ));
        }
        reject_duplicate_ids(
            problems,
            self.partitions.iter().map(|part| part.name.as_str()),
            "duplicate partition name",
        );
    }

    fn validate_metrics(&self, problems: &mut Vec<LabProblem>) {
        if self.metrics.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires at least one metric",
            ));
        }
        reject_duplicate_ids(
            problems,
            self.metrics.iter().map(|metric| metric.id.as_str()),
            "duplicate metric id",
        );
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
    }

    fn validate_thresholds(&self, problems: &mut Vec<LabProblem>) {
        let thresholds = &self.thresholds;
        require_positive(
            problems,
            thresholds.max_median_regression,
            "max_median_regression must be positive",
        );
        require_positive(
            problems,
            thresholds.max_p99_regression,
            "max_p99_regression must be positive",
        );
        require_positive(
            problems,
            thresholds.max_memory_regression,
            "max_memory_regression must be positive",
        );
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
    }

    fn validate_rules(&self, problems: &mut Vec<LabProblem>) {
        reject_duplicate_ids(
            problems,
            self.kill_rules.iter().map(|rule| rule.id.as_str()),
            "duplicate kill rule id",
        );
    }

    /// Versioned canonical encoding (`lab:...`); identity input.
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
            "lab:{}:{}:{}:{}:{}:{}:{}:[{}]:[{}]:[{}]:{}:{}:{}:{}",
            self.experiment_id.0,
            if self.frozen { "frozen" } else { "draft" },
            self.baseline.token(),
            self.candidate.token(),
            self.generator,
            self.seed,
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
                "generator".into(),
                JsonValue::String(self.generator.clone()),
            ),
            ("seed".into(), json_count(self.seed)),
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
        let manifest = LabManifest {
            schema: owned_string_field(object, "schema")?,
            experiment_id: ContentId(owned_string_field(object, "experiment_id")?),
            frozen: bool_field(object, "frozen")?,
            baseline: artifact_from_json(field(object, "baseline")?, "baseline")?,
            candidate: artifact_from_json(field(object, "candidate")?, "candidate")?,
            generator: owned_string_field(object, "generator")?,
            seed: u64_field(object, "seed")?,
            partitions: array_field(object, "partitions", "partitions")?
                .iter()
                .map(partition_from_json)
                .collect::<Result<Vec<CorpusPartition>, LabError>>()?,
            metrics: array_field(object, "metrics", "metrics")?
                .iter()
                .map(metric_from_json)
                .collect::<Result<Vec<MetricSpec>, LabError>>()?,
            thresholds: thresholds_from_json(object_field(object, "thresholds", "thresholds")?)?,
            kill_rules: array_field(object, "kill_rules", "kill_rules")?
                .iter()
                .map(kill_rule_from_json)
                .collect::<Result<Vec<KillRule>, LabError>>()?,
            fallback: fallback_from_json(object_field(object, "fallback", "fallback")?)?,
            environment: environment_from_json(object_field(
                object,
                "environment",
                "environment",
            )?)?,
        };
        if let Some(problem) = manifest.validate().into_iter().next() {
            return Err(LabError::new(problem.code, problem.message));
        }
        Ok(manifest)
    }
}

fn partition_from_json(entry: &JsonValue) -> Result<CorpusPartition, LabError> {
    let object = expect_object(entry, "partition")?;
    Ok(CorpusPartition {
        name: string_field(object, "name", "partition.name")?.to_string(),
        kind: parse_partition_kind(string_field(object, "kind", "partition.kind")?)?,
        operations: expect_u64(field(object, "operations")?, "partition.operations")?,
        fingerprint: ContentId(
            string_field(object, "fingerprint", "partition.fingerprint")?.to_string(),
        ),
    })
}

fn metric_from_json(entry: &JsonValue) -> Result<MetricSpec, LabError> {
    let object = expect_object(entry, "metric")?;
    Ok(MetricSpec {
        id: string_field(object, "id", "metric.id")?.to_string(),
        kind: string_field(object, "kind", "metric.kind")?.to_string(),
        unit: string_field(object, "unit", "metric.unit")?.to_string(),
        direction: parse_direction(string_field(object, "direction", "metric.direction")?)?,
        weight: number_field(object, "weight", "metric.weight")?,
    })
}

fn thresholds_from_json(object: &[(String, JsonValue)]) -> Result<Thresholds, LabError> {
    const FIELDS: [(&str, &str); 4] = [
        ("max_median_regression", "thresholds.max_median_regression"),
        ("max_p99_regression", "thresholds.max_p99_regression"),
        ("max_memory_regression", "thresholds.max_memory_regression"),
        ("min_correctness_rate", "thresholds.min_correctness_rate"),
    ];
    let mut nums = [0.0; 4];
    for (slot, &(key, path)) in FIELDS.iter().enumerate() {
        nums[slot] = number_field(object, key, path)?;
    }
    Ok(Thresholds {
        max_median_regression: nums[0],
        max_p99_regression: nums[1],
        max_memory_regression: nums[2],
        min_correctness_rate: nums[3],
        energy_budget_joules: optional_number_field(
            object,
            "energy_budget_joules",
            "thresholds.energy_budget_joules",
        )?,
    })
}

fn kill_rule_from_json(entry: &JsonValue) -> Result<KillRule, LabError> {
    let object = expect_object(entry, "kill_rule")?;
    Ok(KillRule {
        id: string_field(object, "id", "kill_rule.id")?.to_string(),
        condition: kill_condition_from_json(field(object, "condition")?)?,
        action: parse_kill_action(string_field(object, "action", "kill_rule.action")?)?,
    })
}

fn fallback_from_json(object: &[(String, JsonValue)]) -> Result<FallbackPlan, LabError> {
    const FIELDS: [(&str, &str); 3] = [
        ("on_gate_failure", "fallback.on_gate_failure"),
        ("on_regression", "fallback.on_regression"),
        ("on_measurement_failure", "fallback.on_measurement_failure"),
    ];
    let mut actions = [FallbackAction::RetainBaseline; 3];
    for (slot, &(key, path)) in FIELDS.iter().enumerate() {
        actions[slot] = parse_fallback_action(string_field(object, key, path)?)?;
    }
    Ok(FallbackPlan {
        on_gate_failure: actions[0],
        on_regression: actions[1],
        on_measurement_failure: actions[2],
    })
}

fn environment_from_json(object: &[(String, JsonValue)]) -> Result<EnvironmentPin, LabError> {
    Ok(EnvironmentPin {
        toolchain: string_field(object, "toolchain", "environment.toolchain")?.to_string(),
        target_triple: string_field(object, "target_triple", "environment.target_triple")?
            .to_string(),
        features: array_field(object, "features", "environment.features")?
            .iter()
            .map(|entry| expect_string(entry, "environment.features[]").map(str::to_string))
            .collect::<Result<Vec<String>, LabError>>()?,
        host: string_field(object, "host", "environment.host")?.to_string(),
    })
}

fn reject_duplicate_ids<'a>(
    problems: &mut Vec<LabProblem>,
    ids: impl Iterator<Item = &'a str>,
    message: &str,
) {
    let mut names: Vec<&str> = ids.collect();
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        problems.push(problem("E-HOST-003", message));
    }
}

fn require_positive(problems: &mut Vec<LabProblem>, value: f64, message: &str) {
    if !value.is_finite() || value <= 0.0 {
        problems.push(problem("E-HOST-003", message));
    }
}
