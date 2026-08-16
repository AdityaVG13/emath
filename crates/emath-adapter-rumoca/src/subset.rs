//!: first emath dynamic-model subset contract.
//!
//! The subset covers parameters and state, scalar/vector quantities,
//! equations and derivatives, initial values, components and connections,
//! continuous time and basic events. Anything outside receives a typed
//! refusal; nothing is silently accepted.

use crate::structural::{StructuralModel, VariableKind};

/// Dynamic-model subset feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubsetFeature {
    /// Parameters (fixed over the horizon).
    Parameters,
    /// Time-varying state variables.
    State,
    /// Scalar and vector quantities.
    ScalarVector,
    /// Algebraic equations.
    Equations,
    /// Time derivatives.
    Derivatives,
    /// Initial values.
    InitialValues,
    /// Components.
    Components,
    /// Connections.
    Connections,
    /// Continuous time.
    ContinuousTime,
    /// Basic (continuous) events.
    BasicEvents,
}

/// Subset refusal with a stable `E-KIND-3xx` code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubsetIssue {
    /// Stable code (`E-KIND-310`/`E-KIND-311`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Checks a structural model against the first dynamic-model subset.
///
/// Supported features pass silently; refusal issues carry the offending
/// element. The returned vector is deterministic (model order).
#[must_use]
pub fn check(model: &StructuralModel) -> Vec<SubsetIssue> {
    let mut issues = Vec::new();
    for variable in &model.variables {
        if !matches!(variable.kind, VariableKind::Parameter | VariableKind::State) {
            issues.push(SubsetIssue {
                code: "E-KIND-310",
                message: format!(
                    "variable `{}` uses role outside dynamic-model subset",
                    variable.name
                ),
            });
        }
    }
    for component in &model.components {
        if crate::map::classify(component_kind_construct(component.kind)).map_or(true, |mapping| {
            mapping.class == crate::map::MappingClass::Unsupported
        }) {
            issues.push(SubsetIssue {
                code: "E-KIND-311",
                message: format!(
                    "component `{}` outside dynamic-model subset",
                    component.name
                ),
            });
        }
    }
    for event in &model.events {
        if !event.continuous {
            issues.push(SubsetIssue {
                code: "E-KIND-310",
                message: format!(
                    "event `{}` is discrete; Phase 1 subset accepts basic continuous events only",
                    event.name
                ),
            });
        }
    }
    issues
}

fn component_kind_construct(kind: crate::structural::ComponentKind) -> &'static str {
    match kind {
        crate::structural::ComponentKind::Model | crate::structural::ComponentKind::Block => {
            "equation"
        }
        crate::structural::ComponentKind::Connector => "connector",
        crate::structural::ComponentKind::Record => "record",
    }
}
