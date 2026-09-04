//! Manifest parsing and validation.

use super::*;

impl LabManifest {
        /// Validates; every problem carries a stable code.
    #[must_use]
    pub fn validate(&self) -> Vec<LabProblem> {
        let mut problems = Vec::new();
        if self.schema != "lab" {
            problems.push(problem("E-HOST-003", "schema must be lab"));
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
        if self.generator.is_empty() {
            problems.push(problem(
                "E-HOST-003",
                "manifest requires a generator identity",
            ));
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
        let schema = expect_string(field(object, "schema")?, "schema")?.to_string();
        let experiment_id =
            ContentId(expect_string(field(object, "experiment_id")?, "experiment_id")?.to_string());
        let frozen = expect_bool(field(object, "frozen")?, "frozen")?;
        let baseline = artifact_from_json(field(object, "baseline")?, "baseline")?;
        let candidate = artifact_from_json(field(object, "candidate")?, "candidate")?;
        let generator = expect_string(field(object, "generator")?, "generator")?.to_string();
        let seed = expect_u64(field(object, "seed")?, "seed")?;
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
            generator,
            seed,
            frozen,
        };
        if let Some(problem) = manifest.validate().into_iter().next() {
            return Err(LabError::new(problem.code, problem.message));
        }
        Ok(manifest)
    }
}
