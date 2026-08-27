//! DAE plan and simulation providers.
//!
//! Causalization and simulation are provider outputs, not universal SIR
//! meaning. Both run through the runtime Outcome/Budget/Continuation
//! contracts: only `Resolved` carries admitted value authority; exhaustion
//! and failure are typed. All numerics are deterministic f64 and the trace
//! canonical form is byte-identical across runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_core::{ContentId, SchemaId, fnv1a64_bytes};
use emath_runtime::{Budget, ContinuationHandle, EvidenceHandle, Outcome, UnresolvedReason};

use crate::lower::{DaePlan, LowerError};
use crate::structural::{EqExpr, StructuralModel};

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
    identity: u64,
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
    identity: u64,
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

/// Flattens a dynamic model into a deterministic runnable component.
///
/// Construction is bounded before validation/code generation. The current
/// component profile deliberately accepts exactly two scalar states and no
/// algebraic outputs; unsupported shapes refuse instead of being dropped.
pub fn build_simulation_artifact(
    model: &StructuralModel,
    budget: &Budget,
) -> Outcome<SimulationArtifact, ArtifactError> {
    let (work, estimated_bytes) = match model_complexity(model) {
        Ok(complexity) => complexity,
        Err(error) => return Outcome::Failed(error),
    };
    let estimated_output = estimated_bytes.saturating_add(work.saturating_mul(64));
    if work > budget.iterations
        || work > budget.evaluations
        || u128::from(work) > budget.work_units
        || estimated_bytes > budget.memory_bytes
        || estimated_output > budget.output_bytes
    {
        return unresolved_artifact();
    }

    let issues = model.validate();
    if let Some(issue) = issues.first() {
        return Outcome::Failed(ArtifactError {
            code: "E-PROV-237",
            message: format!(
                "artifact model refused by validation gate: {} issue(s); first: {}: {}",
                issues.len(),
                issue.code,
                issue.message
            ),
        });
    }
    let plan = match crate::lower::lower(model) {
        Ok(plan) => plan,
        Err(error) => {
            return Outcome::Failed(ArtifactError {
                code: error.code,
                message: error.message,
            });
        }
    };
    if plan.states.len() != 2 || !plan.variables.is_empty() {
        return Outcome::Failed(ArtifactError {
            code: "E-PROV-239",
            message: format!(
                "runnable component profile requires exactly two scalar states and no algebraic outputs; found {} state(s), {} output(s)",
                plan.states.len(),
                plan.variables.len()
            ),
        });
    }

    let derivatives = match derivative_metadata(model, &plan) {
        Ok(metadata) => metadata,
        Err(error) => return Outcome::Failed(error),
    };
    let rust_source = match render_rust_component(model, &plan, &derivatives) {
        Ok(source) => source,
        Err(error) => return Outcome::Failed(error),
    };
    let model_canonical = model.canonical();
    let plan_canonical = plan.canonical();
    let derivatives_canonical = derivatives_canonical(&derivatives);
    let output_bytes = u64::try_from(rust_source.len()).unwrap_or(u64::MAX);
    let retained_bytes = output_bytes
        .saturating_add(u64::try_from(model_canonical.len()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(plan_canonical.len()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(derivatives_canonical.len()).unwrap_or(u64::MAX));
    let transient_bytes = retained_bytes.saturating_mul(2);
    if output_bytes > budget.output_bytes || transient_bytes > budget.memory_bytes {
        return unresolved_artifact();
    }

    let mut artifact = SimulationArtifact {
        model: model.clone(),
        plan,
        derivatives,
        rust_source,
        identity: 0,
    };
    let canonical = artifact_canonical(
        &model_canonical,
        &plan_canonical,
        &derivatives_canonical,
        &artifact.rust_source,
    );
    artifact.identity = fnv1a64_bytes(canonical.as_bytes());
    let identity = ContentId(format!("fnv1a64:{:016x}", artifact.identity));
    Outcome::Resolved {
        value: artifact,
        evidence: EvidenceHandle {
            schema: SchemaId("emath.simulation-artifact".into()),
            identity,
        },
    }
}

fn model_complexity(model: &StructuralModel) -> Result<(u64, u64), ArtifactError> {
    let mut work = model
        .variables
        .len()
        .saturating_add(model.equations.len())
        .saturating_add(model.initial_conditions.len())
        .saturating_add(model.components.len())
        .saturating_add(model.connections.len())
        .saturating_add(model.events.len());
    let mut bytes = 0usize;
    for variable in &model.variables {
        bytes = bytes
            .saturating_add(variable.name.len())
            .saturating_add(variable.unit.name.len());
    }
    for component in &model.components {
        bytes = bytes.saturating_add(component.name.len());
    }
    for connection in &model.connections {
        bytes = bytes
            .saturating_add(connection.left.len())
            .saturating_add(connection.right.len());
    }
    for equation in &model.equations {
        bytes = bytes.saturating_add(equation.origin.len());
        measure_expression(&equation.lhs, 1, &mut work, &mut bytes)?;
        measure_expression(&equation.rhs, 1, &mut work, &mut bytes)?;
    }
    for initial in &model.initial_conditions {
        bytes = bytes.saturating_add(initial.target.len());
        measure_expression(&initial.value, 1, &mut work, &mut bytes)?;
    }
    for event in &model.events {
        bytes = bytes.saturating_add(event.name.len());
        measure_expression(&event.condition, 1, &mut work, &mut bytes)?;
    }
    let work = u64::try_from(work).unwrap_or(u64::MAX);
    let bytes = u64::try_from(bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(work.saturating_mul(32));
    Ok((work, bytes))
}

fn measure_expression(
    expression: &EqExpr,
    depth: usize,
    work: &mut usize,
    bytes: &mut usize,
) -> Result<(), ArtifactError> {
    if depth > emath_core::limits::DEFAULT_MAX_NESTING {
        return Err(ArtifactError {
            code: "E-PROV-239",
            message: format!(
                "component expression exceeds maximum nesting depth {}",
                emath_core::limits::DEFAULT_MAX_NESTING
            ),
        });
    }
    *work = work.saturating_add(1);
    match expression {
        EqExpr::Var(name) | EqExpr::Der(name) => {
            *bytes = bytes.saturating_add(name.len());
        }
        EqExpr::ConstF64(_) => {
            *bytes = bytes.saturating_add(std::mem::size_of::<u64>());
        }
        EqExpr::Add(left, right)
        | EqExpr::Sub(left, right)
        | EqExpr::Mul(left, right)
        | EqExpr::Div(left, right) => {
            measure_expression(left, depth + 1, work, bytes)?;
            measure_expression(right, depth + 1, work, bytes)?;
        }
        EqExpr::Pow(base, _) | EqExpr::Neg(base) => {
            measure_expression(base, depth + 1, work, bytes)?;
        }
    }
    Ok(())
}

fn derivatives_canonical(derivatives: &[DerivativeMetadata]) -> String {
    derivatives
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{}:{}",
                entry.state, entry.derivative, entry.equation, entry.origin
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn artifact_canonical(model: &str, plan: &str, derivatives: &str, rust_source: &str) -> String {
    format!(
        "simulation-artifact:model:{model}:plan:{plan}:derivatives:{derivatives}:rust:{rust_source}"
    )
}

fn unresolved_artifact() -> Outcome<SimulationArtifact, ArtifactError> {
    Outcome::Unresolved {
        reason: UnresolvedReason::BudgetExhausted,
        partial: None,
        continuation: Some(ContinuationHandle {
            schema: SchemaId("emath.simulation-artifact".into()),
            identity: ContentId(DEFAULT_SEAL.into()),
            provider_id: "rumoca.structural".into(),
        }),
        evidence: EvidenceHandle {
            schema: SchemaId("emath.simulation-artifact".into()),
            identity: ContentId(DEFAULT_SEAL.into()),
        },
    }
}

fn derivative_metadata(
    model: &StructuralModel,
    plan: &DaePlan,
) -> Result<Vec<DerivativeMetadata>, ArtifactError> {
    plan.derivatives
        .iter()
        .map(|derivative| {
            let equation = model
                .equations
                .iter()
                .position(|equation| equation.lhs == EqExpr::Der(derivative.state.clone()))
                .ok_or_else(|| ArtifactError {
                    code: "E-PROV-239",
                    message: format!("no flattened equation for `{}`", derivative.name),
                })?;
            Ok(DerivativeMetadata {
                state: derivative.state.clone(),
                derivative: derivative.name.clone(),
                equation,
                origin: model.equations[equation].origin.clone(),
            })
        })
        .collect()
}

fn render_rust_component(
    model: &StructuralModel,
    plan: &DaePlan,
    derivatives: &[DerivativeMetadata],
) -> Result<String, ArtifactError> {
    for name in plan.parameters.iter().chain(&plan.states) {
        if !is_rust_identifier(name) {
            return Err(ArtifactError {
                code: "E-PROV-239",
                message: format!("`{name}` is not a safe Rust component identifier"),
            });
        }
    }

    let mut source = String::from(
        "#![forbid(unsafe_code)]\n\
         #[derive(Clone, Copy, Debug, PartialEq)]\n\
         pub struct Parameters {\n",
    );
    for parameter in &plan.parameters {
        let _ = writeln!(source, "    pub {parameter}: f64,");
    }
    source.push_str(
        "}\n\
         #[derive(Clone, Copy, Debug, PartialEq)]\n\
         pub struct State {\n",
    );
    for state in &plan.states {
        let _ = writeln!(source, "    pub {state}: f64,");
    }
    source.push_str(
        "}\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         pub struct DerivativeMetadata {\n\
         \x20   pub state: &'static str,\n\
         \x20   pub derivative: &'static str,\n\
         \x20   pub equation: usize,\n\
         \x20   pub origin: &'static str,\n\
         }\n\
         pub const DERIVATIVES: &[DerivativeMetadata] = &[\n",
    );
    for entry in derivatives {
        let _ = writeln!(
            source,
            "    DerivativeMetadata {{ state: {:?}, derivative: {:?}, equation: {}, origin: {:?} }},",
            entry.state, entry.derivative, entry.equation, entry.origin
        );
    }
    source.push_str("];\nimpl State {\n    #[must_use]\n    pub fn derivatives(self, p: &Parameters) -> Self {\n");
    for derivative in &plan.derivatives {
        let equation = model
            .equations
            .iter()
            .find(|equation| equation.lhs == EqExpr::Der(derivative.state.clone()))
            .expect("metadata verified every derivative equation");
        let expression = render_rust_expression(&equation.rhs, plan)?;
        let _ = writeln!(source, "        let d_{} = {expression};", derivative.state);
    }
    source.push_str("        Self {\n");
    for state in &plan.states {
        let _ = writeln!(source, "            {state}: d_{state},");
    }
    source.push_str(
        "        }\n\
         \x20   }\n\
         \x20   #[must_use]\n\
         \x20   pub fn step_euler(self, p: &Parameters, dt: f64) -> Self {\n\
         \x20       let d = self.derivatives(p);\n\
         \x20       Self {\n",
    );
    for state in &plan.states {
        let _ = writeln!(
            source,
            "            {state}: self.{state} + dt * d.{state},"
        );
    }
    source.push_str("        }\n    }\n}\n");
    Ok(source)
}

fn render_rust_expression(expression: &EqExpr, plan: &DaePlan) -> Result<String, ArtifactError> {
    let render = |expression| render_rust_expression(expression, plan);
    match expression {
        EqExpr::Var(name) if plan.parameters.contains(name) => Ok(format!("p.{name}")),
        EqExpr::Var(name) if plan.states.contains(name) => Ok(format!("self.{name}")),
        EqExpr::Var(name) => Err(ArtifactError {
            code: "E-PROV-239",
            message: format!("component expression references unsupported variable `{name}`"),
        }),
        EqExpr::Der(name) => Err(ArtifactError {
            code: "E-PROV-239",
            message: format!("component RHS references non-causal derivative `der({name})`"),
        }),
        EqExpr::ConstF64(bits) => Ok(format!("f64::from_bits({bits})")),
        EqExpr::Add(left, right) => Ok(format!("({} + {})", render(left)?, render(right)?)),
        EqExpr::Sub(left, right) => Ok(format!("({} - {})", render(left)?, render(right)?)),
        EqExpr::Mul(left, right) => Ok(format!("({} * {})", render(left)?, render(right)?)),
        EqExpr::Div(left, right) => Ok(format!("({} / {})", render(left)?, render(right)?)),
        EqExpr::Pow(base, exponent) => Ok(format!("({}).powi({exponent})", render(base)?)),
        EqExpr::Neg(inner) => Ok(format!("(-{})", render(inner)?)),
    }
}

fn is_rust_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
        && !matches!(
            name,
            "as" | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "async"
                | "await"
                | "dyn"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "gen"
                | "macro"
                | "override"
                | "priv"
                | "try"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
        )
}

const DEFAULT_SEAL: &str = "fnv1a64:0000000000000000";

/// Runs a forward-Euler simulation of a causal DAE plan through the runtime
/// Outcome contract. Parameters must be exact f64 values; the trace is
/// deterministic, and the work respects `budget`.
pub fn simulate(
    model: &StructuralModel,
    plan: &DaePlan,
    parameters: &BTreeMap<String, f64>,
    config: &SimulationConfig,
    budget: &Budget,
) -> Outcome<SimulationResult, SimError> {
    let continuation = || ContinuationHandle {
        schema: SchemaId("emath.simulation".into()),
        identity: ContentId(DEFAULT_SEAL.into()),
        provider_id: "emath-native-euler".into(),
    };
    let evidence = || EvidenceHandle {
        schema: SchemaId("emath.simulation".into()),
        identity: ContentId(DEFAULT_SEAL.into()),
    };

    let issues = model.validate();
    if !issues.is_empty() {
        return Outcome::Failed(SimError {
            code: "E-PROV-237",
            message: format!(
                "provider model refused by validation gate: {} issue(s); first: {}: {}",
                issues.len(),
                issues[0].code,
                issues[0].message
            ),
        });
    }
    // Budget preflight: steps, evaluation count, memory footprint, output
    // size and work units are all bounded; a hungry run is never started.
    let point_bytes = u64::try_from(std::mem::size_of::<SimPoint>()).unwrap_or(64);
    let per_step = u64::try_from(plan.equations.len().max(1)).unwrap_or(1);
    if config.steps > budget.iterations
        || config.steps.saturating_mul(per_step) > budget.evaluations
        || config.steps.saturating_mul(point_bytes) > budget.memory_bytes
        || config.steps.saturating_mul(point_bytes) > budget.output_bytes
        || u128::from(config.steps).saturating_mul(u128::from(per_step)) > budget.work_units
    {
        return Outcome::Unresolved {
            reason: UnresolvedReason::BudgetExhausted,
            partial: None,
            continuation: Some(continuation()),
            evidence: evidence(),
        };
    }

    // Plan shape preflight (bug-hunt residual): a simulation requires at
    // least two states (position/velocity fixture), a derivative row for
    // every state and a finite positive step. Malformed plans fail closed
    // under `E-PROV-235` instead of indexing past the end of states.
    if plan.states.len() < 2 {
        return Outcome::Failed(SimError {
            code: "E-PROV-235",
            message: format!(
                "DaePlan has {} state(s); simulation requires at least two",
                plan.states.len()
            ),
        });
    }
    // The fixture-time recorder represents at most two states; a larger
    // plan is refused instead of silently dropping every extra state.
    if plan.states.len() > 2 {
        return Outcome::Failed(SimError {
            code: "E-PROV-238",
            message: format!(
                "DaePlan has {} states; the 2-state recorder cannot represent it",
                plan.states.len()
            ),
        });
    }
    if !config.dt.is_finite() || config.dt <= 0.0 {
        return Outcome::Failed(SimError {
            code: "E-PROV-235",
            message: format!("invalid time step `dt={}`", config.dt),
        });
    }
    for state in &plan.states {
        if !plan.derivatives.iter().any(|entry| entry.state == *state) {
            return Outcome::Failed(SimError {
                code: "E-PROV-235",
                message: format!("missing derivative row for state `{state}`"),
            });
        }
    }

    // Parameter completeness is checked before any stepping.
    for parameter in &plan.parameters {
        if !parameters.contains_key(parameter) {
            return Outcome::Failed(SimError {
                code: "E-PROV-230",
                message: format!("missing parameter value for `{parameter}`"),
            });
        }
    }

    let mut states: BTreeMap<String, f64> = BTreeMap::new();
    for state in &plan.states {
        states.insert(state.clone(), 0.0);
    }
    for initial in &plan.initial_conditions {
        let (target, value) = initial.split_once('=').unwrap_or((initial.as_str(), ""));
        let resolved = value.parse::<f64>().or_else(|_| {
            parameters.get(value).copied().ok_or_else(|| SimError {
                code: "E-PROV-234",
                message: format!("unresolvable initial value `{value}` for `{target}`"),
            })
        });
        let resolved = match resolved {
            Ok(value) => value,
            Err(error) => return Outcome::Failed(error),
        };
        if let Some(slot) = states.get_mut(target) {
            *slot = resolved;
        } else {
            return Outcome::Failed(SimError {
                code: "E-PROV-234",
                message: format!("initial condition targets unknown state `{target}`"),
            });
        }
    }

    let mut derivatives: BTreeMap<String, f64> = BTreeMap::new();
    for entry in &plan.derivatives {
        derivatives.insert(entry.state.clone(), 0.0);
    }

    let mut points = Vec::with_capacity(usize::try_from(config.steps).unwrap_or(0) + 1);
    points.push(SimPoint {
        t: 0.0,
        // Preflight guarantees at least two states, all present in states.
        position: states[&plan.states[0].clone()],
        velocity: states[&plan.states[1].clone()],
    });
    let mut t = 0.0;
    let mut max_lte: f64 = 0.0;

    for _ in 0..config.steps {
        // Causal evaluation: derivatives and outputs in plan order.
        for equation_index in &plan.order {
            let equation = &model.equations[*equation_index];
            let value = match eval(&equation.rhs, parameters, &states, &derivatives) {
                Ok(value) if value.is_finite() => value,
                Ok(_) => {
                    return Outcome::Failed(SimError {
                        code: "E-PROV-231",
                        message: "non-finite value during evaluation".into(),
                    });
                }
                Err(error) => return Outcome::Failed(error),
            };
            match &equation.lhs {
                EqExpr::Der(state) => {
                    if let Some(slot) = derivatives.get_mut(state) {
                        *slot = value;
                    } else {
                        return Outcome::Failed(SimError {
                            code: "E-PROV-232",
                            message: format!("assignment to unknown derivative `der({state})`"),
                        });
                    }
                }
                EqExpr::Var(name) => {
                    if let Some(slot) = states.get_mut(name) {
                        *slot = value;
                    } else {
                        // Non-state variables (e.g. outputs) were silently
                        // dropped before; refuse loudly (E-PROV-236).
                        return Outcome::Failed(SimError {
                            code: "E-PROV-236",
                            message: format!(
                                "assignment to non-state variable '{name}' is not supported"
                            ),
                        });
                    }
                }
                _ => {
                    return Outcome::Failed(SimError {
                        code: "E-PROV-236",
                        message: "equation LHS is not an assignable variable or der reference"
                            .into(),
                    });
                }
            }
        }

        // Euler integration of state derivatives.
        let mut step_lte: f64 = 0.0;
        for state in &plan.states {
            let derivative = derivatives[state];
            let next = states[state] + derivative * config.dt;
            if config.error_estimate {
                step_lte = step_lte.max(0.5 * config.dt * derivative.abs());
            }
            states.insert(state.clone(), next);
        }
        max_lte = max_lte.max(step_lte);
        t += config.dt;
        points.push(SimPoint {
            t,
            position: states[&plan.states[0].clone()],
            velocity: states[&plan.states[1].clone()],
        });
    }

    let result = SimulationResult {
        points,
        final_position: states[&plan.states[0].clone()],
        final_velocity: states[&plan.states[1].clone()],
        max_lte,
        steps: config.steps,
        termination: "completed",
        identity: 0,
    };
    let mut sealed = result.clone();
    sealed.identity = fnv1a64_bytes(result.canonical().as_bytes());
    let identity = ContentId(format!("fnv1a64:{:016x}", sealed.content_identity()));
    Outcome::Resolved {
        value: sealed,
        evidence: EvidenceHandle {
            schema: SchemaId("emath.simulation".into()),
            identity,
        },
    }
}

/// Evaluates an expression over parameter, state and derivative values.
fn eval(
    expression: &EqExpr,
    parameters: &BTreeMap<String, f64>,
    states: &BTreeMap<String, f64>,
    derivatives: &BTreeMap<String, f64>,
) -> Result<f64, SimError> {
    match expression {
        EqExpr::Var(name) => parameters
            .get(name)
            .or_else(|| states.get(name))
            .copied()
            .ok_or_else(|| SimError {
                code: "E-PROV-230",
                message: format!("unknown variable `{name}` during evaluation"),
            }),
        EqExpr::Der(name) => derivatives.get(name).copied().ok_or_else(|| SimError {
            code: "E-PROV-232",
            message: format!("unknown derivative `der({name})` during evaluation"),
        }),
        EqExpr::ConstF64(bits) => Ok(f64::from_bits(*bits)),
        EqExpr::Add(left, right) => Ok(eval(left, parameters, states, derivatives)?
            + eval(right, parameters, states, derivatives)?),
        EqExpr::Sub(left, right) => Ok(eval(left, parameters, states, derivatives)?
            - eval(right, parameters, states, derivatives)?),
        EqExpr::Mul(left, right) => Ok(eval(left, parameters, states, derivatives)?
            * eval(right, parameters, states, derivatives)?),
        EqExpr::Div(left, right) => {
            let divisor = eval(right, parameters, states, derivatives)?;
            if divisor == 0.0 {
                return Err(SimError {
                    code: "E-PROV-233",
                    message: "division by zero during evaluation".into(),
                });
            }
            Ok(eval(left, parameters, states, derivatives)? / divisor)
        }
        EqExpr::Pow(base, exponent) => {
            Ok(eval(base, parameters, states, derivatives)?.powi(*exponent))
        }
        EqExpr::Neg(inner) => Ok(-eval(inner, parameters, states, derivatives)?),
    }
}
