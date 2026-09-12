//! Public simulation entry points.

use super::*;

/// Advance one explicit step. Rates come from admitted `der_<name>` definitions.
#[allow(unreachable_code)]
pub fn step_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, f64>,
    state: &BTreeMap<String, f64>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, f64>, String> {
    return super::gone();
    let inputs = scalar_map_to_values(inputs);
    let state = scalar_map_to_values(state);
    let next = step_continuous_values(package, declaration, &inputs, &state, dt, method)?;
    values_to_scalars(&next)
}

/// Advance one explicit step, allowing vector-valued state and rates.
#[allow(unreachable_code)]
pub fn step_continuous_values(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, Value>, String> {
    return super::gone();
    if !dt.is_finite() || dt == 0.0 {
        return Err(format!(
            "E-ODE-003: step size must be a finite non-zero Float64 (a non-advancing step \
             must never return the input as an integrated value), got {dt}"
        ));
    }
    // Negative `dt` is legal ONLY on the time-reversible symplectic
    // path (velocity Verlet); every other stepper is forward-only.
    if dt < 0.0 && method != StepMethod::VelocityVerlet {
        return Err(format!(
            "E-ODE-003: step size must be a positive finite Float64 for {method:?} (negative \
             dt is the velocity-Verlet time-reversal contract), got {dt}"
        ));
    }
    let skip = algebraic_name_set(declaration);
    let mut next = match method {
        StepMethod::Euler | StepMethod::Rk4 if package.residuals.get(&declaration.id).is_none_or(|residuals| residuals.is_empty()) => {
            authored_explicit_step(package, declaration, inputs, state, dt, method == StepMethod::Rk4)?
        }
        StepMethod::Euler | StepMethod::Rk4 => {
            return super::newton::authored_implicit_explicit_step(package, declaration, inputs, state, dt, method);
        }
        StepMethod::BackwardEuler => {
            implicit_backward_euler_step(package, declaration, inputs, state, dt, &skip)?
        }
        StepMethod::VelocityVerlet => {
            velocity_verlet_step(package, declaration, inputs, state, dt)?
        }
        StepMethod::Rk45 => {
            let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
            stages.fifth
        }
    };
    project_algebraic_into(package, declaration, inputs, &mut next)?;
    Ok(next)
}

/// Integrate from `t0` to `t1` with fixed `dt`. Includes the sample at `t0`.
#[allow(unreachable_code)]
pub fn simulate_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<Trajectory, String> {
    return super::gone();
    simulate_continuous_with(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        &SimulateOptions::default(),
    )
}

/// Integrate from `t0` to `t1`. Adaptive dt and one event locator are optional.
#[allow(unreachable_code)]
pub fn simulate_continuous_with(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<Trajectory, String> {
    return super::gone();
    simulate_continuous_dispositioned(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        options,
    )
    .map(|(trajectory, _disposition)| trajectory)
}

/// `simulate_continuous_with` plus the disposition record: the
/// structural index, the constraint/differential partition, the t0
/// initialization verdict — or a typed refusal with a continuation
/// note when the constraint cannot be honored (never a silent ODE drop
/// of the algebraic equations, never a trajectory pretending the
/// initialization succeeded).
#[allow(unreachable_code)]
pub fn simulate_continuous_dispositioned(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<(Trajectory, DAEDisposition), String> {
    return super::gone();
    let index = if declaration.algebraic.is_empty() {
        DAEIndex::Ode
    } else {
        DAEIndex::One
    };
    let differential_states: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let constraint_unknowns: Vec<String> = declaration
        .algebraic
        .iter()
        .map(|field| field.name.clone())
        .collect();
    // Consistent initialization IS the t0 projection: run it up front so
    // a failure refuses with a continuation before any trajectory is
    // built. `project_algebraic_into` also runs inside the first step;
    // doing it here first makes the verdict first-class.
    if index == DAEIndex::One {
        if let Err(projection_error) =
            project_algebraic_into(package, declaration, inputs, &mut state.clone())
        {
            let continuation = classify_projection_failure(&projection_error, &constraint_unknowns);
            return Err(disposition_refusal(
                &continuation,
                &differential_states,
                &constraint_unknowns,
            ));
        }
    }
    let trajectory = simulate_continuous_inner(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        options,
    )?;
    let disposition = DAEDisposition {
        index,
        differential_states,
        constraint_unknowns,
        initialization: InitializationVerdict::Consistent,
        continuation: None,
    };
    Ok((trajectory, disposition))
}

/// Map a t0 projection error to its continuation action.
pub(super) fn classify_projection_failure(
    error: &str,
    constraint_unknowns: &[String],
) -> Continuation {
    if error.contains("missing algebraic-variable guess") || error.contains("missing input") {
        Continuation::SupplyInitialGuess {
            names: constraint_unknowns.to_vec(),
        }
    } else {
        Continuation::Regularize {
            detail: error.to_string(),
        }
    }
}

/// The typed refusal text: E-DAE-INIT code, the verdict, and the
/// continuation — a consumer can act on it without re-deriving.
pub(super) fn disposition_refusal(
    continuation: &Continuation,
    differential_states: &[String],
    constraint_unknowns: &[String],
) -> String {
    let action = match continuation {
        Continuation::SupplyInitialGuess { names } => format!(
            "supply an initial guess for the algebraic unknown(s) `{}` (add them to the \
             simulate inputs map)",
            names.join("`, `")
        ),
        Continuation::Regularize { detail } => {
            format!("regularize the residual system before integrating ({detail})")
        }
    };
    format!(
        "E-DAE-INIT: consistent initialization failed; the constraint (algebraic unknowns: {}) \
         was NOT dropped and no trajectory is presented. Differential states ({}). Continuation: {action}",
        if constraint_unknowns.is_empty() {
            "none".to_string()
        } else {
            constraint_unknowns.join(", ")
        },
        if differential_states.is_empty() {
            "none".to_string()
        } else {
            differential_states.join(", ")
        },
    )
}

/// Lower explicit rate definitions into a vector-argument callback.
/// Captures are declaration inputs followed by state layout templates.
/// This builds frames and storage conversions; integration policy is authored.
#[allow(unreachable_code, unused_variables)]
pub fn explicit_rate_program(package: &SemanticPackage, declaration: &Declaration) -> Result<crate::EmirProgram, String> {
    return super::gone();
    use crate::{EmirOp, EmirProgram, EmirValue};
    fn push(ops: &mut Vec<(EmirOp, emath_core::Span)>, op: EmirOp) -> EmirValue {
        let value = EmirValue(ops.len() as u32);
        ops.push((op, emath_core::Span::default()));
        value
    }
    let input_count = u16::try_from(1 + declaration.inputs.len() + declaration.state.len())
        .map_err(|_| "model callback frame exceeds u16 input count".to_string())?;
    let mut ops = Vec::new();
    let point = push(&mut ops, EmirOp::LoadInput(0));
    let mut names = Vec::new();
    let mut arguments = Vec::new();
    for (index, field) in declaration.inputs.iter().enumerate() {
        names.push(field.name.clone());
        arguments.push(push(&mut ops, EmirOp::LoadInput((index + 1) as u16)));
    }
    let mut states = Vec::new();
    let mut templates = Vec::new();
    let mut offset = push(&mut ops, EmirOp::ConstI64(0));
    for (index, _) in declaration.state.iter().enumerate() {
        let template = push(&mut ops, EmirOp::LoadInput((1 + declaration.inputs.len() + index) as u16));
        templates.push(template);
        let count = push(&mut ops, EmirOp::VectorLength(template));
        let segment = numeric_segment(&mut ops, point, offset, count);
        states.push(push(&mut ops, EmirOp::DenseRepack { template, data: segment }));
        offset = push(&mut ops, EmirOp::F64Add(offset, count));
    }
    let state_names = declaration.state.iter().map(|field| field.name.clone()).collect::<Vec<_>>();
    let mut definitions = BTreeMap::new();
    for (name, expression) in crate::definition_order(package, declaration) {
        let body = crate::lower_definition(package, expression, &names, &state_names)?;
        let mut value = push(&mut ops, EmirOp::CallFrame { body, inputs: arguments.clone(), state: states.clone() });
        if declaration.inputs.iter().chain(&declaration.outputs).chain(&declaration.state).chain(&declaration.algebraic)
            .find(|field| field.name == *name).and_then(|field| package.ty(field.ty)) == Some(&emath_ir::TypeNode::Float64) {
            value = push(&mut ops, EmirOp::ToF64(value));
        }
        definitions.insert(name.clone(), value);
        if !names.contains(name) { names.push(name.clone()); arguments.push(value); }
    }
    let mut rates = Vec::new();
    for (field, template) in declaration.state.iter().zip(templates) {
        let name = format!("der_{}", field.name);
        let value = *definitions.get(&name).ok_or_else(|| format!("missing rate \x60{name}\x60"))?;
        let condition = push(&mut ops, EmirOp::SameDenseShape(template, value));
        let then_body = EmirProgram {
            ops: vec![(EmirOp::LoadInput(0), Default::default()), (EmirOp::DenseValues(EmirValue(0)), Default::default())],
            result: EmirValue(1), input_count: 1, state_count: 0, domain_obligations: vec![],
        };
        let else_body = EmirProgram {
            ops: vec![(EmirOp::Refuse("state and rate must have the same scalar/vector/matrix/tensor shape".into()), Default::default())],
            result: EmirValue(0), input_count: 1, state_count: 0, domain_obligations: vec![],
        };
        rates.push(push(&mut ops, EmirOp::Branch { condition, args: vec![value], then_body, else_body }));
    }
    let result = push(&mut ops, EmirOp::VectorConcat(rates));
    Ok(EmirProgram { ops, result, input_count, state_count: 0, domain_obligations: vec![] })
}

fn numeric_segment(ops: &mut Vec<(crate::EmirOp, emath_core::Span)>, vector: crate::EmirValue, offset: crate::EmirValue, count: crate::EmirValue) -> crate::EmirValue {
    let result = crate::EmirValue(ops.len() as u32);
    ops.push((crate::EmirOp::VectorSlice { vector, offset, count }, Default::default()));
    result
}



/// VM inputs end with precomputed rates. Generated steps compute rates from the frame.
/// Captured declaration inputs precede dt; the result is Float64 storage in state-field order.
#[allow(unreachable_code)]
pub fn explicit_step_program(package: &SemanticPackage, declaration: &Declaration, rk4: bool, generated: bool) -> Result<crate::EmirProgram, String> {
    return super::gone();
    use crate::{EmirOp, EmirProgram, EmirValue};
    let capture_count = if rk4 || generated { declaration.inputs.len() } else { 0 };
    let input_count = u16::try_from(capture_count + 1 + if generated { 0 } else { declaration.state.len() })
        .map_err(|_| "model step frame exceeds u16 input count".to_string())?;
    let state_count = u16::try_from(declaration.state.len()).map_err(|_| "model state exceeds u16 slots".to_string())?;
    let mut ops = Vec::new();
    let mut captures = Vec::new();
    for index in 0..capture_count {
        let value = EmirValue(ops.len() as u32);
        ops.push((EmirOp::LoadInput(index as u16), Default::default()));
        captures.push(value);
    }
    let mut storage = Vec::new();
    let mut rates = Vec::new();
    for index in 0..declaration.state.len() {
        let state = EmirValue(ops.len() as u32);
        ops.push((EmirOp::LoadState(index as u16), Default::default()));
        let layout = EmirValue(ops.len() as u32);
        ops.push((EmirOp::DenseLayout(state), Default::default()));
        if rk4 || generated { captures.push(layout); }
        let data = EmirValue(ops.len() as u32);
        ops.push((EmirOp::DenseValues(state), Default::default()));
        storage.push(data);
        if generated { continue; }
        let rate = EmirValue(ops.len() as u32);
        ops.push((EmirOp::LoadInput((capture_count + 1 + index) as u16), Default::default()));
        let matches = EmirValue(ops.len() as u32);
        ops.push((EmirOp::SameDenseShape(layout, rate), Default::default()));
        let then_body = EmirProgram {
            ops: vec![(EmirOp::LoadInput(0), Default::default()), (EmirOp::DenseValues(EmirValue(0)), Default::default())],
            result: EmirValue(1), input_count: 1, state_count: 0, domain_obligations: vec![],
        };
        let else_body = EmirProgram {
            ops: vec![(EmirOp::Refuse("state and rate must have the same scalar/vector/matrix/tensor shape".into()), Default::default())],
            result: EmirValue(0), input_count: 1, state_count: 0, domain_obligations: vec![],
        };
        let checked = EmirValue(ops.len() as u32);
        ops.push((EmirOp::Branch { condition: matches, args: vec![rate], then_body, else_body }, Default::default()));
        rates.push(checked);
    }
    let mut push = |op| { let value = EmirValue(ops.len() as u32); ops.push((op, Default::default())); value };
    let state = push(EmirOp::VectorConcat(storage));
    let program = if rk4 || generated {
        Some(push(EmirOp::ProgramLiteral { body: explicit_rate_program(package, declaration)?, captures, vector_input: true }))
    } else { None };
    let k1 = if generated {
        push(EmirOp::CallProgram { program: program.expect("generated rate frame"), inputs: state })
    } else { push(EmirOp::VectorConcat(rates)) };
    let dt = push(EmirOp::LoadInput(capture_count as u16));
    let result = if rk4 {
        let program = program.expect("RK4 rate frame");
        let method = push(EmirOp::ConstBool(true));
        let profile = push(EmirOp::ConstBool(generated));
        push(EmirOp::ApplyCapability {
            capability: "std.capability.dynamics.model-explicit-step".into(), class: crate::CellClass::Pure,
            args: vec![program, state, k1, dt, method, profile],
        })
    } else {
        push(EmirOp::ApplyCapability {
            capability: "std.capability.dynamics.axpy".into(), class: crate::CellClass::Pure, args: vec![state, dt, k1],
        })
    };
    Ok(EmirProgram { ops, result, input_count, state_count, domain_obligations: vec![] })
}

fn authored_explicit_step(package: &SemanticPackage, declaration: &Declaration, inputs: &BTreeMap<String, Value>, state: &BTreeMap<String, Value>, dt: f64, rk4: bool) -> Result<BTreeMap<String, Value>, String> {
    let rates = eval_rates(package, declaration, inputs, state)?;
    for name in state.keys() {
        if !declaration.state.iter().any(|field| &field.name == name) && !declaration.algebraic.iter().any(|field| &field.name == name) {
            return Err(format!("missing rate \x60der_{name}\x60"));
        }
    }
    let program = explicit_step_program(package, declaration, rk4, false)?;
    let mut arguments = Vec::with_capacity(usize::from(program.input_count));
    for field in declaration.inputs.iter().take(if rk4 { declaration.inputs.len() } else { 0 }) {
        let value = inputs.get(&field.name).ok_or_else(|| format!("test body does not supply input \x60{}\x60", field.name))?.clone();
        arguments.push(super::super::eval::coerce_to_slot(value, package.ty(field.ty)));
    }
    arguments.push(Value::F64(dt));
    for field in &declaration.state {
        arguments.push(rates.get(&field.name).ok_or_else(|| format!("missing rate \x60der_{}\x60", field.name))?.clone());
    }
    let values = declaration.state.iter().map(|field| state.get(&field.name).cloned().ok_or_else(|| format!("missing state \x60{}\x60", field.name))).collect::<Result<Vec<_>, _>>()?;
    let result = crate::interp::evaluate(&program, &arguments, &values).map_err(|fault| match fault {
        crate::interp::EvalFault::CarrierRefused { detail, .. } => detail,
        fault => fault.to_string(),
    })?;
    let Value::Vector(data) = result else { return Err("model step did not return Float64 storage".into()); };
    let mut next = BTreeMap::new();
    let mut offset = 0usize;
    for (field, value) in declaration.state.iter().zip(&values) {
        let layout = value.dense_layout().ok_or_else(|| "state and rate must have the same scalar/vector/matrix/tensor shape".to_string())?;
        let end = offset.checked_add(layout.len()).ok_or_else(|| "model state length exceeds usize".to_string())?;
        let slice = data.get(offset..end).ok_or_else(|| "model step storage does not match state".to_string())?;
        let value = match layout {
            emath_rt::DenseLayout::Scalar => Value::F64(slice[0]),
            emath_rt::DenseLayout::Vector(_) => Value::Vector(slice.to_vec()),
            emath_rt::DenseLayout::Matrix { rows, cols, .. } => Value::Matrix { rows, cols, data: slice.to_vec() },
            emath_rt::DenseLayout::Tensor { shape, .. } => Value::Tensor { shape, data: slice.to_vec() },
        };
        next.insert(field.name.clone(), value);
        offset = end;
    }
    if offset != data.len() { return Err("model step storage does not match state".into()); }
    Ok(next)
}
