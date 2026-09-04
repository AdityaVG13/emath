//! Event detection, firing, and action application.

use super::*;

/// Fire every declared event whose condition is true at the initial
/// sample, in declaration order. Each action applies to the live input
/// map (persists) or the initial state map; after all t0 actions the
/// algebraic projection re-runs so the consistent-initialization
/// verdict stays honest under the switched inputs.
pub(super) fn fire_t0_events(
    package: &SemanticPackage,
    declaration: &Declaration,
    events: &[EventDecl],
    transitions: &[TransitionDecl],
    live_inputs: &mut BTreeMap<String, Value>,
    current: &mut BTreeMap<String, Value>,
    t0: f64,
    events_fired: &mut Vec<EventFiring>,
) -> Result<(), String> {
    for event in events {
        let held = event_condition(package, declaration, live_inputs, current, event)?;
        if !held {
            continue;
        }
        apply_event_action(package, declaration, live_inputs, current, event, t0)?;
        apply_transition_actions(
            package,
            declaration,
            live_inputs,
            current,
            transitions,
            event,
            t0,
        )?;
        events_fired.push(EventFiring {
            name: event.name.clone(),
            t: t0,
        });
    }
    if !events_fired.is_empty() {
        project_algebraic_into(package, declaration, live_inputs, current)?;
    }
    Ok(())
}

/// Detect the first declared event whose condition rises (false → true)
/// across the accepted step. On a rising edge the crossing time is
/// bisected within the fixed 40-iteration budget — the same budget as
/// the `--event` variable locator — and the action applies at the
/// crossing state. Returns `Some(fire_t)` when an event fired (the loop
/// then continues from the crossing state; `current`, `events_fired`,
/// and the live input map carry the mutation).
pub(super) fn try_fire_rising_event(
    package: &SemanticPackage,
    declaration: &Declaration,
    events: &[EventDecl],
    transitions: &[TransitionDecl],
    live_inputs: &mut BTreeMap<String, Value>,
    current: &mut BTreeMap<String, Value>,
    next: &BTreeMap<String, Value>,
    t: f64,
    dt: f64,
    method: StepMethod,
    events_fired: &mut Vec<EventFiring>,
) -> Result<Option<f64>, String> {
    for event in events {
        let before = event_condition(package, declaration, live_inputs, current, event)?;
        if before {
            continue;
        }
        let after = event_condition(package, declaration, live_inputs, next, event)?;
        if !after {
            continue;
        }
        // Rising edge: bisect between current and next.
        let mut lo_t = t;
        let mut hi_t = t + dt;
        let mut lo = current.clone();
        let mut hi = next.clone();
        for _ in 0..EVENT_LOCATE_ITERATIONS {
            let mid_t = 0.5 * (lo_t + hi_t);
            let mid = step_continuous_values(
                package,
                declaration,
                live_inputs,
                &lo,
                mid_t - lo_t,
                method,
            )?;
            let held = event_condition(package, declaration, live_inputs, &mid, event)?;
            if held {
                hi_t = mid_t;
                hi = mid;
            } else {
                lo_t = mid_t;
                lo = mid;
            }
            if hi_t - lo_t <= EVENT_LOCATE_TOLERANCE {
                break;
            }
        }
        let fire_t = 0.5 * (lo_t + hi_t);
        let mut fired_state = hi.clone();
        apply_event_action(
            package,
            declaration,
            live_inputs,
            &mut fired_state,
            event,
            fire_t,
        )?;
        apply_transition_actions(
            package,
            declaration,
            live_inputs,
            &mut fired_state,
            transitions,
            event,
            fire_t,
        )?;
        project_algebraic_into(package, declaration, live_inputs, &mut fired_state)?;
        *current = fired_state;
        events_fired.push(EventFiring {
            name: event.name.clone(),
            t: fire_t,
        });
        return Ok(Some(fire_t));
    }
    Ok(None)
}

/// Evaluate an event's condition at a state point. The condition
/// expression was admitted Boolean with definitions inlined; runtime
/// evaluation binds inputs (+ algebraic unknowns from the state map, so
/// the projected values participate) and state fields through the same
/// lowering path as definitions.
pub(super) fn event_condition(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    event: &EventDecl,
) -> Result<bool, String> {
    match eval_event_expr(package, declaration, live_inputs, state, event.condition)? {
        Value::Bool(flag) => Ok(flag),
        other => Err(format!(
            "E-EVENT-006: event `{}` condition must evaluate to Bool, got {other:?}",
            event.name
        )),
    }
}

/// Refuse a non-finite / non-scalar action value: an event/transition
/// action writing NaN/±Inf (or a non-numeric carrier) into a slot would
/// violate the finite-state invariant the runner enforces everywhere
/// else (same discipline as `values_finite` in adaptive stepping). The
/// write is abandoned BEFORE it lands, so a poisoned value never enters
/// the live input or state map. `code` distinguishes the event payload
/// refusal (E-EVENT-009) from the transition refusal (E-TRANS-008);
/// `who` names the event/rule and `target` the slot, both in the text.
pub(super) fn check_finite_action_value(
    code: &str,
    who: &str,
    target: &str,
    value: &Value,
) -> Result<(), String> {
    match value {
        Value::F64(number) if !number.is_finite() => Err(format!(
            "{code}: {who} must write a finite numeric scalar, got {number}; non-finite action \
             value — target `{target}` was NOT written"
        )),
        Value::F64(_) | Value::I64(_) => Ok(()),
        other => Err(format!(
            "{code}: {who} must write a finite numeric scalar, got {other:?}; non-numeric \
             action value — target `{target}` was NOT written"
        )),
    }
}

/// Evaluate an event's action right-hand side at a state point and
/// write the target slot: an `inputs:` name goes into the live input
/// map (all later steps see it), a `state:` name into the state map.
pub(super) fn apply_event_action(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &mut BTreeMap<String, Value>,
    state: &mut BTreeMap<String, Value>,
    event: &EventDecl,
    fire_t: f64,
) -> Result<(), String> {
    let value = eval_event_expr(package, declaration, live_inputs, state, event.action.expr)?;
    let target = &event.action.target;
    check_finite_action_value(
        "E-EVENT-009",
        &format!("event `{}` action", event.name),
        target,
        &value,
    )?;
    let is_input = declaration.inputs.iter().any(|field| &field.name == target);
    if is_input {
        if live_inputs.contains_key(target) {
            live_inputs.insert(target.clone(), value);
        } else {
            return Err(format!(
                "E-EVENT-007: event `{}` action target `{target}` is not bound in the \
                 simulate inputs map (pass --set {target}=... so switching can rewrite it)",
                event.name
            ));
        }
    } else if state.contains_key(target) {
        state.insert(target.clone(), value);
    } else {
        return Err(format!(
            "E-EVENT-007: event `{}` action target `{target}` is neither a bound input nor \
             a state slot at t={fire_t}",
            event.name
        ));
    }
    Ok(())
}

/// After an event fires, dispatch every transition rule whose `trigger`
/// names that event, in `transitions:` declaration order, applying each
/// rule's actions in rule order. A `state.<name>` action overwrites the
/// state map; a bare action target writes the live input map when the
/// name is bound there and the state map when it is a declared state
/// (mirroring the semantics layer's inputs-then-states lookup; anything
/// else refuses `E-TRANS-007`). The caller owns the single algebraic
/// re-projection after all transitions.
pub(super) fn apply_transition_actions(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &mut BTreeMap<String, Value>,
    state: &mut BTreeMap<String, Value>,
    transitions: &[TransitionDecl],
    fired_event: &EventDecl,
    fire_t: f64,
) -> Result<(), String> {
    // Capture the fired event's parameters at the crossing state: each
    // param binds the live value of the SAME-NAMED variable. Algebraic
    // unknowns bind from the state map (already projected); inputs from
    // the live input map; states from the state map.
    let mut capture: BTreeMap<String, Value> = BTreeMap::new();
    for param in &fired_event.params {
        let value = if declaration
            .algebraic
            .iter()
            .any(|field| &field.name == param)
        {
            state.get(param)
        } else if let Some(value) = live_inputs.get(param) {
            Some(value)
        } else {
            state.get(param)
        };
        let Some(value) = value else {
            return Err(format!(
                "E-TRANS-007: event `{}` parameter `{param}` has no capture value at t={fire_t}",
                fired_event.name
            ));
        };
        capture.insert(param.clone(), value.clone());
    }
    for rule in transitions {
        if rule.trigger != fired_event.name {
            continue;
        }
        for action in &rule.actions {
            let value = eval_event_expr_with_capture(
                package,
                declaration,
                live_inputs,
                state,
                action.expr,
                &capture,
            )?;
            check_finite_action_value(
                "E-TRANS-008",
                &format!("transition on `{}` action", fired_event.name),
                &action.target,
                &value,
            )?;
            if action.is_state {
                if state.contains_key(&action.target) {
                    state.insert(action.target.clone(), value);
                } else {
                    return Err(format!(
                        "E-TRANS-007: transition on `{}` targets non-state `{}` at t={fire_t}",
                        fired_event.name, action.target
                    ));
                }
            } else if live_inputs.contains_key(&action.target) {
                // Bare name bound as an input: write the live input map
                // (inputs win, matching the semantics lookup order; every
                // declared input is bound here because the event
                // condition already evaluated over all of them).
                live_inputs.insert(action.target.clone(), value);
            } else if state.contains_key(&action.target) {
                // Bare name that is a declared STATE: the semantics layer
                // accepts `inputs.or_else(states)` for bare targets, so
                // the bare form lands in the state map exactly like
                // `state.<name>` would.
                state.insert(action.target.clone(), value);
            } else {
                return Err(format!(
                    "E-TRANS-007: transition on `{}` target `{}` is neither a bound input \
                     nor a state slot at t={fire_t}",
                    fired_event.name, action.target
                ));
            }
        }
    }
    Ok(())
}

/// Lane one evaluation of a declared event expression over the model's
/// inputs, algebraic unknowns, and state. Mirrors
/// `eval_definitions_values`' binding order (inputs + algebraic guesses,
/// then states) so `lower_definition` sees identical registers.
pub(super) fn eval_event_expr(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    expr: EmirExprRef,
) -> Result<Value, String> {
    let capture = BTreeMap::new();
    eval_event_expr_with_capture(package, declaration, live_inputs, state, expr, &capture)
}

/// Evaluation lane shared by event conditions/actions and transition
/// actions. `capture` overrides the natural bindings: for each key the
/// captured value (an event parameter bound at the crossing sample) wins
/// over the input/state/ algebraic-derived value of that name.
pub(super) fn eval_event_expr_with_capture(
    package: &SemanticPackage,
    declaration: &Declaration,
    live_inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    expr: EmirExprRef,
    capture: &BTreeMap<String, Value>,
) -> Result<Value, String> {
    let mut bind_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    if let Some(residuals) = package.residuals.get(&declaration.id) {
        if let Some(first) = residuals.first() {
            for name in &first.algebraic {
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                }
            }
        }
    }
    let mut bind_values: Vec<Value> = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        // Algebraic unknowns bind from the STATE map at lane points: the
        // Newton projection has already written their solved values
        // there, so conditions/actions see the consistent manifold.
        let value = if let Some(captured) = capture.get(name) {
            Some(captured.clone())
        } else if declaration
            .algebraic
            .iter()
            .any(|field| &field.name == name)
        {
            state.get(name).cloned()
        } else {
            live_inputs.get(name).cloned()
        };
        let Some(value) = value else {
            return Err(format!(
                "E-EVENT-007: event expression needs `{name}` bound (pass --set {name}=...)"
            ));
        };
        bind_values.push(value);
    }
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    // A bare reference to a STATE name (no `state.` prefix) lowers as an
    // input slot. That happens when an event parameter name shadows the
    // state and admission keeps the bare spelling (the parameter is the
    // in-scope binding). Bind those state-named slots from the state map
    // so the condition/action sees the live state value during detection;
    // a captured parameter value still overrides it at firing.
    for field in &declaration.state {
        let name = &field.name;
        if bind_names.iter().any(|existing| existing == name) {
            continue;
        }
        bind_names.push(name.clone());
        bind_values.push(
            capture
                .get(name)
                .cloned()
                .or_else(|| state.get(name).cloned())
                .unwrap_or(Value::F64(f64::NAN)),
        );
    }
    let state_values: Vec<Value> = state_names
        .iter()
        .map(|name| {
            capture
                .get(name)
                .cloned()
                .or_else(|| state.get(name).cloned())
                .unwrap_or(Value::F64(f64::NAN))
        })
        .collect();
    let program = crate::lower_definition(package, expr, &bind_names, &state_names)
        .map_err(|detail| format!("E-EVENT-008: event expression refused: {detail}"))?;
    crate::interp::evaluate(&program, &bind_values, &state_values)
        .map_err(|fault| format!("E-EVENT-008: event expression fault: {fault}"))
}
