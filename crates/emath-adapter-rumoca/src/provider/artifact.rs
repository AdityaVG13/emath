//! Simulation-artifact construction and canonicalization.

use super::*;

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

pub(super) fn model_complexity(model: &StructuralModel) -> Result<(u64, u64), ArtifactError> {
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

pub(super) fn measure_expression(
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

pub(super) fn derivatives_canonical(derivatives: &[DerivativeMetadata]) -> String {
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

pub(super) fn artifact_canonical(
    model: &str,
    plan: &str,
    derivatives: &str,
    rust_source: &str,
) -> String {
    format!(
        "simulation-artifact:model:{model}:plan:{plan}:derivatives:{derivatives}:rust:{rust_source}"
    )
}

pub(super) fn unresolved_artifact() -> Outcome<SimulationArtifact, ArtifactError> {
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

pub(super) fn derivative_metadata(
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

pub(super) fn render_rust_component(
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

pub(super) fn render_rust_expression(
    expression: &EqExpr,
    plan: &DaePlan,
) -> Result<String, ArtifactError> {
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

pub(super) fn is_rust_identifier(name: &str) -> bool {
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

pub(super) const DEFAULT_SEAL: &str = "fnv1a64:0000000000000000";
