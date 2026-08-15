//!: emath-to-DAE lowering with structural analysis.
//!
//! Lowering is deterministic: parameters, variables and states are
//! canonicalized, equations are causally ordered by their matched unknown,
//! and every equation keeps provenance. Matching, order and (on failure)
//! tearing candidates are reported as provider-plan outputs, never as
//! universal SIR meaning.

use std::collections::{BTreeMap, BTreeSet};

use emath_core::fnv1a64_bytes;

use crate::structural::{EqExpr, StructuralModel, VariableKind};

/// A time derivative of a state variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivativeDef {
    /// State variable name.
    pub state: String,
    /// Derived identifier used in equations, e.g. `der(x)`.
    pub name: String,
}

/// Equation provenance (origin component and guide expression).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EqProvenance {
    /// Equation index in the plan.
    pub equation: usize,
    /// Origin component path.
    pub component: String,
    /// Guide (canonical lhs).
    pub guide: String,
}

/// Causally ordered differential-algebraic equation plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaePlan {
    /// Parameters, sorted.
    pub parameters: Vec<String>,
    /// Non-parameter variables, sorted.
    pub variables: Vec<String>,
    /// State variables, sorted.
    pub states: Vec<String>,
    /// State derivatives.
    pub derivatives: Vec<DerivativeDef>,
    /// Initial conditions as `target=value` canonicals.
    pub initial_conditions: Vec<String>,
    /// Equations as `lhs=rhs` canonicals.
    pub equations: Vec<String>,
    /// Causal evaluation order (equation indices).
    pub order: Vec<usize>,
    /// Matching list `eq{i}->{unknown}`.
    pub matching: Vec<String>,
    /// Tearing candidates (empty when causalization succeeds).
    pub tearing: Vec<String>,
    /// Per-equation provenance.
    pub provenance: Vec<EqProvenance>,
    identity: u64,
}

impl DaePlan {
    /// Deterministic canonical rendering.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "daep:params:{}:vars:{}:states:{}:dervs:{}:init:{}:eqs:{}:order:{}:match:{}:tear:{}",
            self.parameters.join(","),
            self.variables.join(","),
            self.states.join(","),
            self.derivatives
                .iter()
                .map(|entry| format!("{}->{}", entry.state, entry.name))
                .collect::<Vec<_>>()
                .join(","),
            self.initial_conditions.join(","),
            self.equations.join(","),
            self.order
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.matching.join(","),
            self.tearing.join(",")
        )
    }

    /// FNV-1a64 content identity over the canonical rendering.
    #[must_use]
    pub fn content_identity(&self) -> u64 {
        self.identity
    }
}

/// Lowering failure (structural analysis of the equation system).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerError {
    /// Stable code (`E-PROV-220`/`E-PROV-221`/`E-PROV-222`/`E-PROV-223`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

fn derivative_name(state: &str) -> String {
    format!("der({state})")
}

/// Lowers a validated structural model into a causal DAE plan.
pub fn lower(model: &StructuralModel) -> Result<DaePlan, LowerError> {
    let mut parameters = Vec::new();
    let mut variables = Vec::new();
    let mut states = Vec::new();
    for variable in &model.variables {
        match variable.kind {
            VariableKind::Parameter => parameters.push(variable.name.clone()),
            VariableKind::State => states.push(variable.name.clone()),
            VariableKind::Output | VariableKind::Alias => variables.push(variable.name.clone()),
        }
    }
    parameters.sort_unstable();
    variables.sort_unstable();
    states.sort_unstable();
    let parameters_set: BTreeSet<String> = parameters.iter().cloned().collect();

    let derivatives: Vec<DerivativeDef> = states
        .iter()
        .map(|state| DerivativeDef {
            state: state.clone(),
            name: derivative_name(state),
        })
        .collect();

    // Match every equation to the unknown its lhs produces.
    let mut produced: BTreeMap<String, usize> = BTreeMap::new();
    for (index, equation) in model.equations.iter().enumerate() {
        match produced_identifier(&equation.lhs) {
            Some(identifier) => {
                if produced.insert(identifier.clone(), index).is_some() {
                    return Err(LowerError {
                        code: "E-PROV-222",
                        message: format!("multiple equations produce `{identifier}`"),
                    });
                }
            }
            None => {
                return Err(LowerError {
                    code: "E-PROV-223",
                    message: format!(
                        "equation {index} has no single unknown on its left-hand side"
                    ),
                });
            }
        }
    }

    // Every state derivative must be produced; so must every non-parameter.
    for state in &states {
        let expected = derivative_name(state);
        if !produced.contains_key(&expected) {
            return Err(LowerError {
                code: "E-PROV-220",
                message: format!("underdetermined: no equation produces {expected}"),
            });
        }
    }
    for variable in &variables {
        if !produced.contains_key(variable) {
            return Err(LowerError {
                code: "E-PROV-220",
                message: format!("underdetermined: no equation produces `{variable}`"),
            });
        }
    }

    // Dependencies: equation j needs the unknowns produced by i.
    let mut edges: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut indegree = vec![0usize; model.equations.len()];
    for (index, equation) in model.equations.iter().enumerate() {
        for key in rhs_unknown_keys(&equation.rhs, &parameters_set) {
            if let Some(producer) = produced.get(&key) {
                let producer_index = *producer;
                if producer_index != index && edges.entry(producer_index).or_default().insert(index)
                {
                    indegree[index] += 1;
                }
            }
        }
    }

    // Kahn's algorithm with deterministic tie-break (smallest index first).
    let mut ready: BTreeSet<usize> = (0..model.equations.len())
        .filter(|index| indegree[*index] == 0)
        .collect();
    let mut order = Vec::with_capacity(model.equations.len());
    while let Some(next) = ready.iter().next().copied() {
        ready.remove(&next);
        order.push(next);
        if let Some(dependents) = edges.remove(&next) {
            for dependent in dependents {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    ready.insert(dependent);
                }
            }
        }
    }

    if order.len() < model.equations.len() {
        let mut candidates: Vec<usize> = (0..model.equations.len())
            .filter(|index| !order.contains(index))
            .collect();
        candidates.sort_unstable();
        return Err(LowerError {
            code: "E-PROV-221",
            message: format!(
                "algebraic cycle; tearing candidates: {}",
                candidates
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        });
    }

    let mut matching = Vec::with_capacity(model.equations.len());
    let mut provenance = Vec::with_capacity(model.equations.len());
    for (index, equation) in model.equations.iter().enumerate() {
        if let Some(identifier) = produced_identifier(&equation.lhs) {
            matching.push(format!("eq{index}->{identifier}"));
        }
        provenance.push(EqProvenance {
            equation: index,
            component: equation.origin.clone(),
            guide: equation.lhs.canonical(),
        });
    }

    let mut plan = DaePlan {
        parameters,
        variables,
        states,
        derivatives,
        initial_conditions: model
            .initial_conditions
            .iter()
            .map(|condition| format!("{}={}", condition.target, condition.value.canonical()))
            .collect(),
        equations: model
            .equations
            .iter()
            .map(|equation| format!("{}={}", equation.lhs.canonical(), equation.rhs.canonical()))
            .collect(),
        order,
        matching,
        tearing: Vec::new(),
        provenance,
        identity: 0,
    };
    plan.identity = fnv1a64_bytes(plan.canonical().as_bytes());
    Ok(plan)
}

/// The unknown an equation lhs produces: `x` for `Var(x)`, `der(x)` for `Der(x)`.
fn produced_identifier(expression: &EqExpr) -> Option<String> {
    match expression {
        EqExpr::Var(name) => Some(name.clone()),
        EqExpr::Der(name) => Some(derivative_name(name)),
        _ => None,
    }
}

/// Dependency keys of an expression: plain state references use the state
/// value, explicit derivatives use the derivative identifier, parameters
/// are excluded (produced by no equation).
fn rhs_unknown_keys(expression: &EqExpr, parameters: &BTreeSet<String>) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    collect_keys(expression, parameters, &mut keys);
    keys
}

fn collect_keys(expression: &EqExpr, parameters: &BTreeSet<String>, keys: &mut BTreeSet<String>) {
    match expression {
        EqExpr::Var(name) => {
            if !parameters.contains(name) {
                keys.insert(name.clone());
            }
        }
        EqExpr::Der(name) => {
            keys.insert(derivative_name(name));
        }
        EqExpr::ConstF64(_) => {}
        EqExpr::Add(left, right)
        | EqExpr::Sub(left, right)
        | EqExpr::Mul(left, right)
        | EqExpr::Div(left, right) => {
            collect_keys(left, parameters, keys);
            collect_keys(right, parameters, keys);
        }
        EqExpr::Pow(base, _) | EqExpr::Neg(base) => {
            collect_keys(base, parameters, keys);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::{
        Component, ComponentKind, Connection, Dimensions, EqExpr, Equation, InitialCondition,
        StructuralModel, Unit, VariableDecl, VariableKind,
    };
    use emath_ir::TypeNode;

    fn mass_spring() -> StructuralModel {
        StructuralModel {
            components: vec![
                Component {
                    name: "mass".into(),
                    kind: ComponentKind::Model,
                },
                Component {
                    name: "spring".into(),
                    kind: ComponentKind::Model,
                },
            ],
            variables: vec![
                VariableDecl {
                    name: "m".into(),
                    kind: VariableKind::Parameter,
                    unit: Unit::new("kg".into(), Dimensions::kilograms()),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "c".into(),
                    kind: VariableKind::Parameter,
                    unit: Unit::new(
                        "kg/s".into(),
                        Dimensions::kilograms().div(Dimensions::seconds()),
                    ),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "k".into(),
                    kind: VariableKind::Parameter,
                    unit: Unit::new(
                        "kg/s2".into(),
                        Dimensions::kilograms()
                            .div(Dimensions::seconds())
                            .div(Dimensions::seconds()),
                    ),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "x".into(),
                    kind: VariableKind::State,
                    unit: Unit::meters(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "v".into(),
                    kind: VariableKind::State,
                    unit: Unit::new(
                        "m/s".into(),
                        Dimensions::meters().div(Dimensions::seconds()),
                    ),
                    ty: TypeNode::Float64,
                },
            ],
            equations: vec![
                Equation {
                    lhs: EqExpr::Der("x".into()),
                    rhs: EqExpr::Var("v".into()),
                    origin: "mass".into(),
                },
                Equation {
                    lhs: EqExpr::Der("v".into()),
                    rhs: EqExpr::Div(
                        Box::new(EqExpr::Neg(Box::new(EqExpr::Add(
                            Box::new(EqExpr::Mul(
                                Box::new(EqExpr::Var("c".into())),
                                Box::new(EqExpr::Var("v".into())),
                            )),
                            Box::new(EqExpr::Mul(
                                Box::new(EqExpr::Var("k".into())),
                                Box::new(EqExpr::Var("x".into())),
                            )),
                        )))),
                        Box::new(EqExpr::Var("m".into())),
                    ),
                    origin: "mass".into(),
                },
            ],
            initial_conditions: vec![
                InitialCondition {
                    target: "x".into(),
                    value: EqExpr::constant(1.0),
                },
                InitialCondition {
                    target: "v".into(),
                    value: EqExpr::constant(0.0),
                },
            ],
            connections: vec![
                Connection {
                    left: "mass.port".into(),
                    right: "spring.first".into(),
                },
                Connection {
                    left: "spring.second".into(),
                    right: "mass.port".into(),
                },
            ],
            events: vec![],
        }
    }

    #[test]
    fn mass_spring_lowers_to_causal_plan() {
        let model = mass_spring();
        let issues = model.validate();
        assert!(issues.is_empty(), "model should validate: {issues:?}");
        let plan = lower(&model).unwrap();
        assert_eq!(plan.states, ["v", "x"]);
        assert_eq!(plan.parameters, ["c", "k", "m"]);
        assert_eq!(plan.order.len(), 2);
        // der(x)=v must be causally first.
        assert_eq!(plan.order[0], 0);
        assert_eq!(plan.matching, ["eq0->der(x)", "eq1->der(v)"]);
        assert!(plan.tearing.is_empty());
        assert!(plan.initial_conditions.contains(&"x=1e0".to_string()));
        assert_eq!(plan.provenance[0].component, "mass");
        let again = lower(&model).unwrap();
        assert_eq!(plan.canonical(), again.canonical());
        assert_eq!(plan.content_identity(), again.content_identity());
    }

    #[test]
    fn underdetermined_system_is_typed_error() {
        let mut model = mass_spring();
        model.equations.pop();
        let error = lower(&model).unwrap_err();
        assert_eq!(error.code, "E-PROV-220");
        assert!(error.message.contains("der(v)"));
    }

    #[test]
    fn algebraic_cycle_reports_tearing_candidates() {
        let model = StructuralModel {
            components: vec![
                Component {
                    name: "a".into(),
                    kind: ComponentKind::Model,
                },
                Component {
                    name: "b".into(),
                    kind: ComponentKind::Model,
                },
            ],
            variables: vec![
                VariableDecl {
                    name: "v".into(),
                    kind: VariableKind::Output,
                    unit: Unit::dimensionless(),
                    ty: TypeNode::Float64,
                },
                VariableDecl {
                    name: "w".into(),
                    kind: VariableKind::Output,
                    unit: Unit::dimensionless(),
                    ty: TypeNode::Float64,
                },
            ],
            equations: vec![
                Equation {
                    lhs: EqExpr::Var("v".into()),
                    rhs: EqExpr::Var("w".into()),
                    origin: "a".into(),
                },
                Equation {
                    lhs: EqExpr::Var("w".into()),
                    rhs: EqExpr::Var("v".into()),
                    origin: "b".into(),
                },
            ],
            initial_conditions: vec![],
            connections: vec![],
            events: vec![],
        };
        let error = lower(&model).unwrap_err();
        assert_eq!(error.code, "E-PROV-221");
        assert!(error.message.contains("tearing"));
    }

    #[test]
    fn conflicting_producers_are_typed_error() {
        let mut model = mass_spring();
        model.variables.push(VariableDecl {
            name: "q".into(),
            kind: VariableKind::Output,
            unit: Unit::dimensionless(),
            ty: TypeNode::Float64,
        });
        model.equations.push(Equation {
            lhs: EqExpr::Var("q".into()),
            rhs: EqExpr::constant(0.0),
            origin: "a".into(),
        });
        model.equations.push(Equation {
            lhs: EqExpr::Var("q".into()),
            rhs: EqExpr::constant(1.0),
            origin: "b".into(),
        });
        let error = lower(&model).unwrap_err();
        assert_eq!(error.code, "E-PROV-222");
    }
}
