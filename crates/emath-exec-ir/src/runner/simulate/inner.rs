//! The shared continuous-simulation loop.

use super::*;

/// The integration loop with generic hybrid event scheduling
/// (ch7, event-execution slice). Event actions are
/// applied to the LIVE input/state maps so later steps see them; the
/// caller's maps are never mutated. With no `events:` payloads this is
/// the pre-event loop verbatim.
pub(super) fn simulate_continuous_inner(
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
    if !t0.is_finite() || !t1.is_finite() {
        return Err("trajectory times must be finite Float64".to_string());
    }
    if t1 < t0 {
        return Err("trajectory end must be >= start".to_string());
    }
    if !dt.is_finite() || dt <= 0.0 {
        return Err("step size must be a positive finite Float64".to_string());
    }
    if let Some(atol) = options.atol {
        if !atol.is_finite() || atol <= 0.0 {
            return Err("atol must be a positive finite Float64".to_string());
        }
    }
    if let Some(rtol) = options.rtol {
        if !rtol.is_finite() || rtol <= 0.0 {
            return Err("rtol must be a positive finite Float64".to_string());
        }
    }
    if let Some(dt_max) = options.dt_max {
        if !dt_max.is_finite() || dt_max <= 0.0 {
            return Err("dt-max must be a positive finite Float64".to_string());
        }
    }
    if options.adaptive() && method != StepMethod::Rk45 {
        return Err("adaptive dt requires --method rk45".to_string());
    }
    // Event actions persist into subsequent steps, so the runner keeps a
    // live input map; the caller's map stays untouched.
    let mut live_inputs = inputs.clone();
    let mut events_fired: Vec<EventFiring> = Vec::new();
    let events: &[EventDecl] = package
        .events
        .get(&declaration.id)
        .map(|events| events.as_slice())
        .unwrap_or(&[]);
    let transitions: &[TransitionDecl] = package
        .transitions
        .get(&declaration.id)
        .map(|transitions| transitions.as_slice())
        .unwrap_or(&[]);
    let mut current = state.clone();
    project_algebraic_into(package, declaration, &live_inputs, &mut current)?;
    // Events whose condition already holds at the initial sample fire at
    // t0, in declaration order — a switch that is closed at start-up.
    fire_t0_events(
        package,
        declaration,
        events,
        transitions,
        &mut live_inputs,
        &mut current,
        t0,
        &mut events_fired,
    )?;
    let mut samples = vec![TrajectorySample {
        t: t0,
        state: current.clone(),
    }];
    let mut t = t0;
    let mut h = match options.dt_max {
        Some(dt_max) => dt.min(dt_max),
        None => dt,
    };
    let mut guard = 0_u32;
    const MAX_STEPS: u32 = 1_000_000;
    while t < t1 {
        let remaining = t1 - t;
        if remaining <= 0.0 {
            break;
        }
        let step = h.min(remaining);
        if step <= 0.0 {
            break;
        }
        let (next, used, err) = if options.adaptive() {
            adaptive_rk45_try(package, declaration, &live_inputs, &current, step, options)?
        } else {
            let next =
                step_continuous_values(package, declaration, &live_inputs, &current, step, method)?;
            (next, step, 0.0)
        };
        // Finite-state invariant, matching the adaptive path (which
        // checks `values_finite` inside `adaptive_rk45_try`): a step
        // that produces NaN/±Inf fails the run — it must never silently
        // poison every later sample of the trajectory.
        if !values_finite(&next) {
            return Err(format!(
                "step produced a non-finite state at t={t}: values diverged \
                 (reduce dt or use a stiff/implicit method)"
            ));
        }
        if options.adaptive() && used < step && used <= 0.0 {
            return Err("adaptive step collapsed to a non-positive dt".to_string());
        }
        if options.adaptive() && used < step * 0.999 {
            h = used;
            guard += 1;
            if guard > MAX_STEPS {
                return Err("trajectory exceeded 1_000_000 steps".to_string());
            }
            continue;
        }
        let mut next = next;
        if options.adaptive() {
            project_algebraic_into(package, declaration, &live_inputs, &mut next)?;
        }
        // Hybrid events (ch7): the FIRST event in
        // declaration order whose condition rises across this accepted
        // step fires at its bisected crossing; the action applies to the
        // live maps and later steps see it. At most one event per step —
        // the deterministic tie-break and the per-step budget.
        if let Some(fire_t) = try_fire_rising_event(
            package,
            declaration,
            events,
            transitions,
            &mut live_inputs,
            &mut current,
            &next,
            t,
            used,
            method,
            &mut events_fired,
        )? {
            t = fire_t;
            samples.push(TrajectorySample {
                t,
                state: current.clone(),
            });
            guard += 1;
            if guard > MAX_STEPS {
                return Err("trajectory exceeded 1_000_000 steps".to_string());
            }
            continue;
        }
        if let Some((name, value)) = &options.event {
            if let Some((event_t, event_state)) = locate_event(
                package,
                declaration,
                &live_inputs,
                &current,
                &next,
                t,
                used,
                name,
                *value,
                method,
            )? {
                samples.push(TrajectorySample {
                    t: event_t,
                    state: event_state,
                });
                return Ok(Trajectory {
                    method,
                    dt: h,
                    samples,
                    events: events_fired,
                });
            }
        }
        current = next;
        t += used;
        samples.push(TrajectorySample {
            t,
            state: current.clone(),
        });
        if options.adaptive() {
            h = grow_step(h, err, options, remaining);
        }
        guard += 1;
        if guard > MAX_STEPS {
            return Err("trajectory exceeded 1_000_000 steps".to_string());
        }
        if (t - t1).abs() <= h * 1e-12 {
            break;
        }
    }
    Ok(Trajectory {
        method,
        dt: h,
        samples,
        events: events_fired,
    })
}
