//! DAE plan and simulation providers.
//!
//! Causalization and simulation are provider outputs, not universal SIR
//! meaning. Both run through the runtime Outcome/Budget/Continuation
//! contracts: only `Resolved` carries admitted value authority; exhaustion
//! and failure are typed. All numerics are deterministic f64 and the trace
//! canonical form is byte-identical across runs.

use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::lower;
    use crate::structural::{EqExpr, Equation, StructuralModel, Unit, VariableDecl, VariableKind};
    use emath_ir::TypeNode;
    use emath_runtime::{Budget, Outcome, UnresolvedReason};
    use std::collections::BTreeMap;

    fn two_state_model() -> StructuralModel {
        StructuralModel {
            variables: vec![
                VariableDecl {
                    name: "x".into(),
                    kind: VariableKind::State,
                    unit: Unit::seconds(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "y".into(),
                    kind: VariableKind::State,
                    unit: Unit::seconds(),
                    ty: TypeNode::Float64,
                },
            ],
            equations: vec![
                Equation {
                    lhs: EqExpr::Der("x".into()),
                    rhs: EqExpr::constant(0.0),
                    origin: "fixture".into(),
                },
                Equation {
                    lhs: EqExpr::Der("y".into()),
                    rhs: EqExpr::constant(0.0),
                    origin: "fixture".into(),
                },
            ],
            ..StructuralModel::default()
        }
    }

    #[test]
    fn tiny_memory_budget_refuses_simulation() {
        // Budget.memory_bytes must be consulted: a footprint below the
        // trace size refuses up front instead of running.
        let model = two_state_model();
        let plan = lower(&model).expect("two-state model lowers");
        let outcome = simulate(
            &model,
            &plan,
            &BTreeMap::new(),
            &SimulationConfig {
                dt: 0.001,
                steps: 5,
                error_estimate: false,
            },
            &Budget {
                memory_bytes: 1,
                ..Budget::default()
            },
        );
        assert!(
            matches!(
                outcome,
                Outcome::Unresolved {
                    reason: UnresolvedReason::BudgetExhausted,
                    ..
                }
            ),
            "a memory footprint below the trace size must refuse, got {outcome:?}"
        );
    }

    #[test]
    fn three_state_plan_refused_by_two_state_recorder() {
        // The fixture-time recorder represents at most two states; a
        // 3-state plan must be refused under E-PROV-238 instead of
        // silently dropping state three from the recorded trajectory.
        let model = StructuralModel {
            variables: vec![
                VariableDecl {
                    name: "x".into(),
                    kind: VariableKind::State,
                    unit: Unit::seconds(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "y".into(),
                    kind: VariableKind::State,
                    unit: Unit::seconds(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "z".into(),
                    kind: VariableKind::State,
                    unit: Unit::seconds(),
                    ty: TypeNode::Float64,
                },
            ],
            equations: vec![
                Equation {
                    lhs: EqExpr::Der("x".into()),
                    rhs: EqExpr::constant(0.0),
                    origin: "fixture".into(),
                },
                Equation {
                    lhs: EqExpr::Der("y".into()),
                    rhs: EqExpr::constant(0.0),
                    origin: "fixture".into(),
                },
                Equation {
                    lhs: EqExpr::Der("z".into()),
                    rhs: EqExpr::constant(0.0),
                    origin: "fixture".into(),
                },
            ],
            ..StructuralModel::default()
        };
        let plan = lower(&model).expect("three-state model lowers");
        let outcome = simulate(
            &model,
            &plan,
            &BTreeMap::new(),
            &SimulationConfig {
                dt: 0.001,
                steps: 5,
                error_estimate: false,
            },
            &Budget::default(),
        );
        match outcome {
            Outcome::Failed(error) => assert_eq!(error.code, "E-PROV-238"),
            other => panic!("three-state plan must be refused, got {other:?}"),
        }
    }
}
