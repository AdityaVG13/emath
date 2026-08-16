//!: goal schema.
//!
//! Full goal schema: core kinds plus a custom-goal envelope, inputs and
//! outputs, accuracy, evidence, budget, target, determinism and fallback
//! policy. The schema validates itself (`E-GOAL-011`..`E-GOAL-013`) and
//! carries a versioned canonical encoding for plan identity.

use emath_core::{fnv1a64_bytes, ContentId, SchemaId};
use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalKind,
    TargetProfile,
};

/// Kind in the goal schema, including the custom-goal envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoalKindSpec {
    /// `evaluate <output>`.
    Evaluate,
    /// `differentiate <expr>`.
    Differentiate,
    /// `integrate <expr>`.
    Integrate,
    /// `solve <expr>`.
    Solve,
    /// `optimize <expr>`.
    Optimize,
    /// `simulate <system>`.
    Simulate,
    /// `search <domain>`.
    Search,
    /// `prove <proposition>`.
    Prove,
    /// `verify <claim>`.
    Verify,
    /// `compile <module>`.
    Compile,
    /// `benchmark <expr>`.
    Benchmark,
    /// Custom-goal envelope with its schema id and typed fields.
    Custom {
        /// Envelope schema identity.
        schema: SchemaId,
        /// Ordered fields preserved verbatim.
        fields: Vec<(String, String)>,
    },
}

impl GoalKindSpec {
    /// Stable kind name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Evaluate => "evaluate",
            Self::Differentiate => "differentiate",
            Self::Integrate => "integrate",
            Self::Solve => "solve",
            Self::Optimize => "optimize",
            Self::Simulate => "simulate",
            Self::Search => "search",
            Self::Prove => "prove",
            Self::Verify => "verify",
            Self::Compile => "compile",
            Self::Benchmark => "benchmark",
            Self::Custom { .. } => "custom",
        }
    }
}

/// Budget constraint attached to a goal schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetConstraint {
    /// Maximum compile work units.
    pub max_compile_work: Option<u64>,
    /// Maximum runtime work units.
    pub max_runtime_work: Option<u64>,
    /// Work unit name (e.g. `steps`, `seconds`).
    pub unit: String,
}

impl BudgetConstraint {
    /// No budget constraint.
    #[must_use]
    pub fn none() -> Self {
        Self {
            max_compile_work: None,
            max_runtime_work: None,
            unit: String::new(),
        }
    }
}

/// Full goal schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalSchema {
    /// Goal kind (or custom envelope).
    pub kind: GoalKindSpec,
    /// Declared inputs.
    pub inputs: Vec<String>,
    /// Declared outputs.
    pub outputs: Vec<String>,
    /// Accuracy policy.
    pub accuracy: ExactnessPolicy,
    /// Evidence level required.
    pub evidence: EvidenceLevel,
    /// Budget.
    pub budget: BudgetConstraint,
    /// Target profile.
    pub target: TargetProfile,
    /// Determinism policy.
    pub determinism: DeterminismPolicy,
    /// Fallback policy.
    pub fallback: FallbackPolicy,
    /// Produce string.
    pub produce: String,
}

/// Schema validation problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalSchemaProblem {
    /// Stable code (`E-GOAL-011`..`E-GOAL-013`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl GoalSchema {
    /// Validates the schema; every problem has a stable code.
    #[must_use]
    pub fn validate(&self) -> Vec<GoalSchemaProblem> {
        let mut problems = Vec::new();
        if self.outputs.is_empty() {
            problems.push(GoalSchemaProblem {
                code: "E-GOAL-011",
                message: "goal schema requires at least one output".into(),
            });
        }
        let mut names: Vec<&str> = self.outputs.iter().map(String::as_str).collect();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(GoalSchemaProblem {
                code: "E-GOAL-013",
                message: "duplicate output name in goal schema".into(),
            });
        }
        if (self.budget.max_compile_work.is_some() || self.budget.max_runtime_work.is_some())
            && self.budget.unit.is_empty()
        {
            problems.push(GoalSchemaProblem {
                code: "E-GOAL-012",
                message: "budget limit without work unit".into(),
            });
        }
        problems
    }

    /// Versioned canonical encoding (`goal:v1:...`); the plan-identity input.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "goal:v1:{}:[{}]:[{}]:{}:{}:{}:{}:{}:{}:{}:{}",
            self.kind.name(),
            self.inputs.join(","),
            self.outputs.join(","),
            exactness_token(&self.accuracy),
            self.evidence.as_str(),
            budget_token(&self.budget),
            self.target.family,
            self.determinism_token(),
            fallback_token(&self.fallback),
            self.produce,
            custom_token(&self.kind),
        )
    }
}

/// Deterministic exactness token.
#[must_use]
pub fn exactness_token(policy: &ExactnessPolicy) -> String {
    match policy {
        ExactnessPolicy::Exact => "exact".to_string(),
        ExactnessPolicy::Bounded { tolerance_literal } => {
            format!("bounded:{tolerance_literal}")
        }
        ExactnessPolicy::CheckedNumeric => "checked".to_string(),
        ExactnessPolicy::Estimate => "estimate".to_string(),
        ExactnessPolicy::AnyExplicit => "any".to_string(),
    }
}

/// Deterministic budget token (`-` for absent components).
#[must_use]
pub fn budget_token(budget: &BudgetConstraint) -> String {
    format!(
        "{}:{}:{}",
        budget
            .max_compile_work
            .map_or_else(|| "-".to_string(), |limit| limit.to_string()),
        budget
            .max_runtime_work
            .map_or_else(|| "-".to_string(), |limit| limit.to_string()),
        if budget.unit.is_empty() {
            "-"
        } else {
            &budget.unit
        }
    )
}

/// Deterministic target token.
#[must_use]
pub fn target_token(target: &TargetProfile) -> String {
    format!(
        "{}:{}:{}",
        target.family,
        target.triple.as_deref().unwrap_or("-"),
        target.features.join("+")
    )
}

/// Deterministic fallback token.
#[must_use]
pub fn fallback_token(policy: &FallbackPolicy) -> &'static str {
    match policy {
        FallbackPolicy::NativeOnly => "native-only",
        FallbackPolicy::Parametric => "parametric",
        FallbackPolicy::Continuation => "continuation",
        FallbackPolicy::Diagnostic => "diagnostic",
        FallbackPolicy::ExplicitLadder => "explicit-ladder",
    }
}

/// Deterministic custom-envelope token.
#[must_use]
pub fn custom_token(kind: &GoalKindSpec) -> String {
    match kind {
        GoalKindSpec::Custom { schema, fields } => {
            let mut body: Vec<String> = fields.iter().map(|(k, v)| format!("{k}={v}")).collect();
            body.sort();
            format!("{}:{}", schema.0, body.join("&"))
        }
        _ => "-".to_string(),
    }
}

impl GoalSchema {
    /// Determinism token.
    #[must_use]
    pub fn determinism_token(&self) -> &'static str {
        match self.determinism {
            DeterminismPolicy::Required => "required",
            DeterminismPolicy::Preferred => "preferred",
            DeterminismPolicy::Unspecified => "unspecified",
        }
    }

    /// FNV-1a64 identity of the canonical form.
    #[must_use]
    pub fn identity(&self) -> ContentId {
        ContentId(format!(
            "fnv1a64:{:016x}",
            fnv1a64_bytes(self.canonical().as_bytes())
        ))
    }

    /// Derives a schema from an elaborated GIR goal (bidirectional mapping).
    #[must_use]
    pub fn from_goal(goal: &Goal) -> Self {
        let kind = match &goal.kind {
            GoalKind::Evaluate => GoalKindSpec::Evaluate,
            GoalKind::Differentiate => GoalKindSpec::Differentiate,
            GoalKind::Integrate => GoalKindSpec::Integrate,
            GoalKind::Solve => GoalKindSpec::Solve,
            GoalKind::Optimize => GoalKindSpec::Optimize,
            GoalKind::Simulate => GoalKindSpec::Simulate,
            GoalKind::Search => GoalKindSpec::Search,
            GoalKind::Prove => GoalKindSpec::Prove,
            GoalKind::Verify => GoalKindSpec::Verify,
            GoalKind::Compile => GoalKindSpec::Compile,
            GoalKind::Benchmark => GoalKindSpec::Benchmark,
            GoalKind::Custom(schema) => GoalKindSpec::Custom {
                schema: schema.clone(),
                fields: vec![],
            },
        };
        Self {
            kind,
            inputs: vec![],
            outputs: vec![goal.target.clone()],
            accuracy: goal.requirements.exactness.clone(),
            evidence: goal.requirements.evidence,
            budget: BudgetConstraint::none(),
            target: goal.requirements.target.clone(),
            determinism: goal.requirements.determinism,
            fallback: goal.requirements.fallback,
            produce: goal.requirements.produce.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::Span;
    use emath_ir::{GoalId, GoalRequirements};

    fn sample_goal() -> Goal {
        Goal {
            id: GoalId(1),
            kind: GoalKind::Evaluate,
            target: "y".into(),
            expression: None,
            requirements: GoalRequirements {
                evidence: EvidenceLevel::E1,
                exactness: ExactnessPolicy::Exact,
                determinism: DeterminismPolicy::Required,
                target: TargetProfile {
                    family: "rust-library".into(),
                    triple: None,
                    features: vec![],
                },
                fallback: FallbackPolicy::NativeOnly,
                produce: "rust.library".into(),
            },
            source: Span::default(),
        }
    }

    #[test]
    fn schema_derives_from_gir_goal() {
        let schema = GoalSchema::from_goal(&sample_goal());
        assert_eq!(schema.kind.name(), "evaluate");
        assert_eq!(schema.outputs, ["y"]);
        assert!(schema.validate().is_empty());
        assert!(schema.canonical().starts_with("goal:v1:evaluate:"));
    }

    #[test]
    fn validation_reports_stable_codes() {
        let mut schema = GoalSchema::from_goal(&sample_goal());
        schema.outputs.clear();
        let problems = schema.validate();
        assert!(problems.iter().any(|p| p.code == "E-GOAL-011"));
        schema.outputs = vec!["y".into(), "y".into()];
        assert!(schema.validate().iter().any(|p| p.code == "E-GOAL-013"));
    }

    #[test]
    fn budget_without_unit_is_refused() {
        let mut schema = GoalSchema::from_goal(&sample_goal());
        schema.budget = BudgetConstraint {
            max_compile_work: Some(5),
            max_runtime_work: None,
            unit: String::new(),
        };
        assert!(schema.validate().iter().any(|p| p.code == "E-GOAL-012"));
    }

    #[test]
    fn custom_envelope_canonical_is_stable_golden() {
        let schema = GoalSchema {
            kind: GoalKindSpec::Custom {
                schema: SchemaId("emath.custom.benchmark.v1".into()),
                fields: vec![("warmup".into(), "2".into()), ("runs".into(), "10".into())],
            },
            inputs: vec!["x".into()],
            outputs: vec!["score".into()],
            accuracy: ExactnessPolicy::AnyExplicit,
            evidence: EvidenceLevel::E2,
            budget: BudgetConstraint::none(),
            target: TargetProfile {
                family: "benchmark".into(),
                triple: None,
                features: vec![],
            },
            determinism: DeterminismPolicy::Preferred,
            fallback: FallbackPolicy::Diagnostic,
            produce: "bench.report".into(),
        };
        assert_eq!(
            schema.canonical(),
            "goal:v1:custom:[x]:[score]:any:E2:-:-:-:benchmark:preferred:diagnostic:bench.report:emath.custom.benchmark.v1:runs=10&warmup=2"
        );
        assert_eq!(schema.identity(), schema.identity());
    }
}
