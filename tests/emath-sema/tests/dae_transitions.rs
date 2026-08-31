//! emath-dae-transitions-seq (r3-dynamical-03lh ch7, transitions slice):
//! failure-first evidence for the `transitions:` section admission.
//!
//! A transition rule declares how a declared event re-assigns state or
//! input slots when it fires:
//!
//! ```emath
//! transitions:
//!     on EventName:
//!         state.x = v
//!         voltage = 0
//! ```
//!
//! Each rule must trigger a named event from the same declaration's
//! `events:` section, and each action must target a declared input/state
//! slot with a numeric value. Pass 3 SIR-lowers every action to a kept
//! `TransitionDecl` (the runner owns execution). Refusals are typed:
//!
//! - `E-TRANS-001` — `on <Event>:` names an event not declared in
//!   `events:` (or there is no `events:` section).
//! - `E-TRANS-002` — the action target is not a declared input/state
//!   slot (bare unknown name, dotted `state.<missing>`, deep path).
//! - `E-TRANS-003` — a rule body is not an assignment (nested section,
//!   bare expression), or the action value is non-numeric.
//! - `E-TRANS-004` — `on <Event>:` has an empty body.
//! - `E-TRANS-005` — the action targets an `algebraic:` unknown (the
//!   Newton projection owns those).
//! - `E-TRANS-006` — an event parameter matches NO declared
//!   input/state/algebraic variable, so no payload value can be captured
//!   at firing (binding is undefined).

mod common;

use crate::common::{check_source, error_text};
use emath_exec_ir::interp::Value;
use emath_exec_ir::{
    DAEDisposition, SimulateOptions, StepMethod, Trajectory, simulate_continuous_dispositioned,
};
use std::collections::BTreeMap;

/// Positive: a `transitions:` section with a declared event, a
/// `state.<name>` target, and a value that references the event's own
/// parameter admits with no errors. Event parameters are in scope only
/// inside the matching `on <Event>:` rule. The parameter `v` is also a
/// declared input, so it has a runtime capture source (E-TRANS-006).
#[test]
fn transitions_section_admits_with_declared_event() {
    let source = "\
emath model TModel:
    inputs:
        v: Float64
    state:
        x: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            state.x = v
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    let text = error_text(&result);
    assert!(
        !result.diagnostics.has_errors(),
        "declared-event transition must admit, got: {text}"
    );
}

/// Refusal: `on Missing:` triggers an event that is not declared → the
/// typed refusal names the event.
#[test]
fn transition_unknown_event_trigger_refuses() {
    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on Missing:
            state.x = 5
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "an undeclared event trigger must refuse at admission"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-001"), "must carry E-TRANS-001, got: {text}");
    assert!(text.contains("Missing"), "must name the missing event, got: {text}");
}

/// Refusal: a bare target that is not a declared input or state refuses;
/// so does a `state.<name>` path for a name that is not a state (the
/// refusal carries the full dotted name).
#[test]
fn transition_action_target_must_be_state_or_input() {
    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
            ghost = 1
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "a bare unknown target must refuse at admission"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-002"), "must carry E-TRANS-002, got: {text}");

    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
            state.ghost = 1
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "a `state.<name>` target for a non-state must refuse at admission"
    );
    let text = error_text(&result);
    assert!(
        text.contains("E-TRANS-002"),
        "must carry E-TRANS-002, got: {text}"
    );
    assert!(
        text.contains("state.ghost"),
        "must carry the full dotted target name, got: {text}"
    );
}

/// Refusal: a rule body with a non-assignment (here a nested section)
/// refuses, as does an `on <Event>:` with no body at all.
#[test]
fn transition_malformed_action_refuses() {
    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
            sub:
                foo = 1
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "a non-assignment rule body must refuse at admission"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-003"), "must carry E-TRANS-003, got: {text}");

    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "an empty `on <Event>:` rule must refuse at admission"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-004"), "must carry E-TRANS-004, got: {text}");
}

/// Refusal: an action targeting an `algebraic:` unknown refuses — the
/// Newton projection owns algebraic variables (same rule as events).
#[test]
fn transition_algebraic_target_refuses() {
    let source = "\
emath model TModel:
    algebraic:
        s: Float64
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
            s = 1
    equations:
        der(x) = 1.0
        s - 2.0 == 0
";
    let result = check_source("t", source);
    let text = error_text(&result);
    assert!(
        text.contains("E-TRANS-005"),
        "must carry E-TRANS-005 for an algebraic target, got: {text}"
    );
}

/// Transitions mirror the events admitter's kind policy: no kind gate, so
/// a non-model declaration that carries `transitions:` still validates its
/// rules. With no `events:` on a function, any `on <Event>:` is the
/// unknown-event refusal, not a crash.
#[test]
fn transition_non_model_kind_refuses_unknown_event() {
    let source = "\
emath function F:
    inputs:
        x: Float64
    transitions:
        on Missing:
            x = 1
    define:
        y = x
";
    let result = check_source("f", source);
    assert!(
        result.diagnostics.has_errors(),
        "a function without declared events must refuse an `on` trigger"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-001"), "must carry E-TRANS-001, got: {text}");
}

/// Pass 3 SIR lowering: an `on E:` rule with a `state.<name>` target and
/// an action value referencing the event's captured parameter lowers to a
/// `TransitionDecl` on the package. The kept action `expr` must index the
/// package's expression arena.
#[test]
fn transitions_lower_to_sir() {
    let source = "\
emath model TModel:
    inputs:
        v: Float64
    state:
        x: Float64
    events:
        event E(v: Float64):
            if x > 1.0:
                x = 0
    transitions:
        on E:
            state.x = v
    equations:
        der(x) = v
";
    let result = check_source("t", source);
    assert!(
        !result.diagnostics.has_errors(),
        "SIR-lowerable transition must admit, got: {}",
        error_text(&result)
    );
    let decl = &result.package.declarations[0];
    let rules = result
        .package
        .transitions
        .get(&decl.id)
        .expect("transitions map must carry this declaration");
    assert_eq!(rules.len(), 1, "exactly one `on E:` rule expected");
    let rule = &rules[0];
    assert_eq!(rule.trigger, "E");
    assert_eq!(rule.actions.len(), 1, "one action per rule expected");
    let action = &rule.actions[0];
    assert_eq!(action.target, "x");
    assert!(action.is_state, "`state.x` target must lower with is_state=true");
    assert!(
        result.package.expr(action.expr).is_some(),
        "kept action expr must index the package expression arena"
    );
}

/// The event declaration records its runtime-captured parameters in
/// declaration order; the runner binds them to same-named model variables
/// at firing.
#[test]
fn event_declaration_records_params() {
    let source = "\
emath model TModel:
    inputs:
        v: Float64
    state:
        x: Float64
    events:
        event E(v: Float64):
            if x > 1.0:
                x = 0
    transitions:
        on E:
            state.x = v
    equations:
        der(x) = v
";
    let result = check_source("t", source);
    assert!(
        !result.diagnostics.has_errors(),
        "event declaration must admit, got: {}",
        error_text(&result)
    );
    let decl = &result.package.declarations[0];
    let events = result
        .package
        .events
        .get(&decl.id)
        .expect("events map must carry the payload suite");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "E");
    assert_eq!(events[0].params, vec!["v".to_string()]);
}

/// Refusal: an event parameter that matches NO declared model variable has
/// no payload value to capture → E-TRANS-006 names the parameter and the
/// event.
#[test]
fn event_param_without_capture_source_refuses() {
    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E(qq: Float64)
    transitions:
        on E:
            state.x = qq
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "a param with no capture source must refuse at admission"
    );
    let text = error_text(&result);
    assert!(text.contains("E-TRANS-006"), "must carry E-TRANS-006, got: {text}");
    assert!(text.contains("qq"), "must name the parameter, got: {text}");
    assert!(text.contains("E"), "must name the event, got: {text}");
}

// ---------------------------------------------------------------------------
// Pass 7: negative-diagnostics sweep. Each row forces one refusal path and
// asserts the typed code AND every entity the diagnostic names appear in the
// error text. One assert fails the test, naming the missing needle.
// ---------------------------------------------------------------------------

/// Table-driven sweep over every negative admission path in the transitions
/// slice. One shared model spine (event E + inputs/state/equations) with a
/// per-row `transitions:` block; special rows add the requisite sections.
#[test]
fn transition_refusal_sweep_names_the_entity() {
    let unknown_trigger = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on Missing:
            state.x = 1
    equations:
        der(x) = v
        der(y) = 0
";
    let bare_unknown = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            zzz = 1
    equations:
        der(x) = v
        der(y) = 0
";
    let dotted_unknown = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            state.zzz = 1
    equations:
        der(x) = v
        der(y) = 0
";
    let non_assignment = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            sub:
                foo = 1
    equations:
        der(x) = v
        der(y) = 0
";
    let non_numeric = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            state.y = [1.0]
    equations:
        der(x) = v
        der(y) = 0
";
    let empty_body = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
    equations:
        der(x) = v
        der(y) = 0
";
    let algebraic = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    algebraic:
        a: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
    transitions:
        on E:
            a = 1
    equations:
        der(x) = v
        der(y) = 0
        v - a == 0
";
    let param_no_capture = "\
emath model TModel:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64)
        event F(qq: Float64)
    transitions:
        on F:
            state.x = qq
    equations:
        der(x) = v
        der(y) = 0
";
    let rows: Vec<(&str, &str, &str, &[&str])> = vec![
        ("unknown_trigger", unknown_trigger, "E-TRANS-001", &["Missing"]),
        ("bare_unknown", bare_unknown, "E-TRANS-002", &["zzz"]),
        ("dotted_unknown", dotted_unknown, "E-TRANS-002", &["state.zzz"]),
        ("non_assignment", non_assignment, "E-TRANS-003", &[]),
        ("non_numeric", non_numeric, "E-TRANS-003", &[]),
        ("empty_body", empty_body, "E-TRANS-004", &[]),
        ("algebraic", algebraic, "E-TRANS-005", &["a"]),
        ("param_no_capture", param_no_capture, "E-TRANS-006", &["qq", "F"]),
    ];
    let mut saw_errors = true;
    let mut failures: Vec<String> = Vec::new();
    for (name, source, code, entities) in rows {
        let result = check_source("t", source);
        if !result.diagnostics.has_errors() {
            saw_errors = false;
            failures.push(format!("[{name}] expected a refusal but model admitted"));
            continue;
        }
        let text = error_text(&result);
        let missing: Vec<&str> = std::iter::once(code)
            .chain(entities.iter().copied())
            .filter(|needle| !text.contains(needle))
            .collect();
        if !missing.is_empty() {
            failures.push(format!(
                "[{name}] expected {code:?}+{entities:?} in text, missing {missing:?}: {text}"
            ));
        }
    }
    assert!(
        saw_errors && failures.is_empty(),
        "sweep failures: {}",
        failures.join(" ; ")
    );
}

/// Admission determinism: check the same full hybrid model (events +
/// transitions + equations + algebraic) TWICE and assert the two
/// CheckResults agree on a deterministic projection. Catches arena
/// ordering or hash-map seeding nondeterminism in the SIR build.
const HYBRID_FULL: &str = "\
emath model HybridFull:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
        threshold_voltage: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
        accum: Float64
    events:
        event ThresholdCrossed(voltage: Float64):
            if charge >= capacitance * threshold_voltage:
                voltage = 0
    transitions:
        on ThresholdCrossed:
            state.accum = charge
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
        der(accum) = 0
";

#[test]
fn admission_is_replay_deterministic() {
    let a = check_source("hy", HYBRID_FULL);
    let b = check_source("hy", HYBRID_FULL);
    let ea = error_text(&a);
    let eb = error_text(&b);
    assert!(
        !a.diagnostics.has_errors() && !b.diagnostics.has_errors(),
        "hybrid model must admit on both passes, got a={ea} b={eb}"
    );
    let (da, db) = (&a.package.declarations[0], &b.package.declarations[0]);
    assert_eq!(
        a.package.declarations.len(),
        b.package.declarations.len(),
        "declaration count must be replay-stable"
    );
    assert_eq!(
        a.package.events.get(&da.id),
        b.package.events.get(&db.id),
        "events SIR must be replay-equal"
    );
    assert_eq!(
        a.package.transitions.get(&da.id),
        b.package.transitions.get(&db.id),
        "transitions SIR must be replay-equal"
    );
    assert_eq!(
        a.package.exprs.len(),
        b.package.exprs.len(),
        "expr arena size must be replay-stable"
    );
    assert_eq!(ea, eb, "diagnostic text must be replay-equal");
}

/// Refusal: an action value naming a variable that is neither a declared
/// input nor an event parameter refuses at admission (E-TYPE-002, naming
/// the unknown variable).
#[test]
fn transition_action_undeclared_name_refuses() {
    let source = "\
emath model TModel:
    state:
        x: Float64
    events:
        event E
    transitions:
        on E:
            state.x = zzz
    equations:
        der(x) = 1.0
";
    let result = check_source("t", source);
    assert!(
        result.diagnostics.has_errors(),
        "an undeclared action name must refuse at admission"
    );
    let text = error_text(&result);
    assert!(
        text.contains("zzz"),
        "must name the undeclared variable, got: {text}"
    );
}

// ---------------------------------------------------------------------------
// Pass 4: runner dispatch. When an admitted event fires (t0-hold or rising
// edge), the runner applies the event's OWN action, then dispatches every
// `on <Event>:` rule that triggers the fired event in declaration order.
// These are integration runs through `simulate_continuous_dispositioned`.
// ---------------------------------------------------------------------------

fn scalar(map: &BTreeMap<String, Value>, name: &str) -> f64 {
    match map.get(name) {
        Some(Value::F64(value)) => *value,
        Some(Value::I64(value)) => *value as f64,
        other => panic!("state `{name}` must be scalar, got {other:?}"),
    }
}

fn run_euler(
    source: &str,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t1: f64,
    dt: f64,
) -> (Trajectory, DAEDisposition) {
    let result = check_source("tr", source);
    assert!(
        !result.diagnostics.has_errors(),
        "transition model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    simulate_continuous_dispositioned(
        &result.package,
        decl,
        inputs,
        state,
        0.0,
        t1,
        dt,
        StepMethod::Euler,
        &SimulateOptions::default(),
    )
    .expect("ODE with transitions must integrate")
}

/// THE named contract: an event that fires on a rising edge dispatches a
/// `state.<name>` transition, and the state stays switched afterward.
#[test]
fn transition_fires_and_switches_state() {
    let source = "\
emath model SwitchOnEvent:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Threshold(v: Float64):
            if x >= thr:
                v = 0
    transitions:
        on Threshold:
            state.y = 5
    equations:
        der(x) = v
        der(y) = 0
";
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let (trajectory, _) = run_euler(&source, &inputs, &state, 1.0, 0.1);
    let fire = &trajectory.events;
    assert_eq!(fire.len(), 1, "event must fire exactly once");
    assert_eq!(fire[0].name, "Threshold");
    let fire_t = fire[0].t;
    // Before the crossing y is 0.
    let before = trajectory
        .samples
        .iter()
        .find(|sample| sample.t < fire_t)
        .expect("a pre-crossing sample must exist");
    assert_eq!(scalar(&before.state, "y"), 0.0, "y starts at 0");
    // The firing sample (and every later sample) carries y == 5: the
    // transition switched it and it persists.
    let after: Vec<&emath_exec_ir::TrajectorySample> = trajectory
        .samples
        .iter()
        .filter(|sample| sample.t >= fire_t)
        .collect();
    assert!(!after.is_empty(), "must be a sample at/after the firing");
    for sample in &after {
        assert!(
            (scalar(&sample.state, "y") - 5.0).abs() < 1e-9,
            "y must sit at 5 after the switch, got {} at t={}",
            scalar(&sample.state, "y"),
            sample.t
        );
    }
    let final_y = scalar(&trajectory.samples.last().unwrap().state, "y");
    assert!((final_y - 5.0).abs() < 1e-9, "final y must be 5, got {final_y}");
}

/// A transition action may target a state variable with its BARE name
/// (`y = 5`, no `state.` prefix): the semantics layer accepts a bare
/// target that names a state (inputs checked first, then states), so
/// the runner must apply it to the state map exactly like
/// `state.y = 5`. Regression: the runner used to demand the bare name
/// in the live inputs map and refuse at runtime with E-TRANS-007 even
/// though the model had already been admitted.
#[test]
fn transition_bare_state_name_target_applies() {
    let source = "\
emath model BareStateTarget:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Threshold(v: Float64):
            if x >= thr:
                v = 0
    transitions:
        on Threshold:
            y = 5
    equations:
        der(x) = v
        der(y) = 0
";
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let (trajectory, _) = run_euler(&source, &inputs, &state, 1.0, 0.1);
    assert_eq!(trajectory.events.len(), 1, "event must fire exactly once");
    let fire_t = trajectory.events[0].t;
    // The firing sample and every later sample carry y == 5: the bare
    // `y = 5` transition landed in the state map and persists.
    for sample in trajectory.samples.iter().filter(|sample| sample.t >= fire_t) {
        assert!(
            (scalar(&sample.state, "y") - 5.0).abs() < 1e-9,
            "bare `y = 5` transition must set state y to 5, got {} at t={}",
            scalar(&sample.state, "y"),
            sample.t
        );
    }
    let final_y = scalar(&trajectory.samples.last().unwrap().state, "y");
    assert!(
        (final_y - 5.0).abs() < 1e-9,
        "final y must be 5 via the bare-name transition, got {final_y}"
    );
}

/// A condition already true at t0 fires through `fire_t0_events` (before
/// the first sample is pushed), so the FIRST sample already carries the
/// transitioned state.
#[test]
fn transition_t0_hold_fires_and_applies() {
    let source = "\
emath model TackT0:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Threshold(v: Float64):
            if x >= thr:
                v = 0
    transitions:
        on Threshold:
            state.y = 5
    equations:
        der(x) = 0
        der(y) = 0
";
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(6.0)); // 6 >= 5 -> held at t0.
    state.insert("y".into(), Value::F64(0.0));
    let (trajectory, _) = run_euler(&source, &inputs, &state, 1.0, 0.1);
    assert_eq!(trajectory.events.len(), 1, "t0-hold fires once");
    assert_eq!(trajectory.events[0].t, 0.0, "hold fires at t0");
    assert!(
        (scalar(&trajectory.samples[0].state, "y") - 5.0).abs() < 1e-9,
        "first sample must already carry the transition (fire_t0 runs before the push)"
    );
}

/// Transitions dispatch in `transitions:` declaration order (last action
/// wins for the same target); a rule on an event that never fires never
/// applies.
#[test]
fn transitions_dispatch_in_declaration_order() {
    let source = "\
emath model OrderedDispatch:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
        z: Float64
    events:
        event Threshold(v: Float64):
            if x >= thr:
                v = 0
        event Other(v: Float64):
            if x > 10000:
                v = 0
    transitions:
        on Threshold:
            state.y = 1
            state.y = 2
        on Other:
            state.z = 9
    equations:
        der(x) = v
        der(y) = 0
        der(z) = 0
";
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    state.insert("z".into(), Value::F64(0.0));
    let (trajectory, _) = run_euler(&source, &inputs, &state, 1.0, 0.1);
    // Only Threshold fires; Other's condition never holds.
    assert_eq!(trajectory.events.len(), 1, "only Threshold fires");
    assert_eq!(trajectory.events[0].name, "Threshold");
    let last = trajectory.samples.last().unwrap();
    assert_eq!(
        scalar(&last.state, "y"),
        2.0,
        "the two `on Threshold` actions apply in order, so the last wins"
    );
    assert_eq!(
        scalar(&last.state, "z"),
        0.0,
        "a transition on a never-fired event must never apply"
    );
}

/// An event param is captured at the crossing state and referenced by a
/// transition action: the state takes the captured value (here the
/// crossing value of the state variable itself).
#[test]
fn event_param_captured_at_crossing_into_state() {
    let source = "\
emath model CaptureParam:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Snap(x: Float64):
            if x >= thr:
                v = 1
    transitions:
        on Snap:
            state.y = x
    equations:
        der(x) = v
        der(y) = 0
";
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let (trajectory, _) = run_euler(&source, &inputs, &state, 1.0, 0.1);
    assert_eq!(trajectory.events.len(), 1, "Snap fires once at the crossing");
    assert_eq!(trajectory.events[0].name, "Snap");
    let fire_t = trajectory.events[0].t;
    let firing = trajectory
        .samples
        .iter()
        .find(|sample| (sample.t - fire_t).abs() < 1e-9)
        .expect("firing sample must be on the trajectory");
    assert!(
        (scalar(&firing.state, "y") - 5.0).abs() < 1e-4,
        "captured x at crossing is ~thr, so y becomes thr; got y={}",
        scalar(&firing.state, "y")
    );
    // y is never rewritten after the capture: it stays constant.
    let last = trajectory.samples.last().unwrap();
    assert!(
        (scalar(&firing.state, "y") - scalar(&last.state, "y")).abs() < 1e-4,
        "captured y must persist"
    );
}

/// A small deterministic oscillator (state `v` resets at thresholds)
/// re-arms events; each firing re-dispatches the transition so `y`
/// counts every firing. Same model + inputs + policy replays identically.
#[test]
fn re_armed_event_redispatches_transition() {
    let source = "\
emath model Oscillator:
    state:
        v: Float64
        y: Float64
    events:
        event High(v: Float64):
            if v >= 7:
                v = 0.5
        event Low(v: Float64):
            if v <= 2:
                v = 6.5
    transitions:
        on High:
            state.y = y + 1
        on Low:
            state.y = y + 1
    equations:
        der(v) = -1.0
        der(y) = 0
";
    let inputs = BTreeMap::new();
    let mut state = BTreeMap::new();
    state.insert("v".into(), Value::F64(8.5));
    state.insert("y".into(), Value::F64(0.0));
    let run = |state: &BTreeMap<String, Value>| run_euler(&source, &inputs, state, 5.0, 0.1).0;
    let a = run(&state);
    let b = run(&state);
    // Determinism: same source + inputs + policy -> identical trajectory.
    assert_eq!(a, b, "transition dispatch must be replay-deterministic");
    // Both named events fire, at least three times in total.
    let names: Vec<&String> = a.events.iter().map(|f| &f.name).collect();
    assert!(
        names.iter().any(|n| n.as_str() == "High"),
        "High must fire at the t0 hold (v=8.5 >= 7)"
    );
    assert!(
        names.iter().any(|n| n.as_str() == "Low"),
        "Low must fire on resets"
    );
    assert!(
        names.len() >= 3,
        "the oscillator must re-arm and fire at least three times, got {}",
        names.len()
    );
    // Every firing increments y by exactly one, so the final y equals the
    // firing count.
    let y_final = scalar(&a.samples.last().unwrap().state, "y");
    assert!(
        (y_final - names.len() as f64).abs() < 1e-9,
        "y counts the firings: {y_final} == {}",
        names.len()
    );
}

// ---------------------------------------------------------------------------
// Pass 5: action safety. A transition/event action that evaluates to a
// NON-FINITE value (NaN/±Inf) must refuse the run with a typed code
// AFTER the event fired — never poison the trajectory. A switch that
// makes the residual DAE singular must refuse through the Newton
// projection (no trajectory pretending success), and that refusal must
// replay deterministically.
// ---------------------------------------------------------------------------

/// Refusal: a `state.<name>` transition action whose RHS evaluates to
/// ±Inf at firing refuses `E-TRANS-008` naming the target, and the run
/// returns Err (no poisoned trajectory).
#[test]
fn transition_action_non_finite_refuses() {
    let source = "\
emath model NonFiniteTrans:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(x: Float64):
            if x >= thr:
                v = 0
    transitions:
        on E:
            state.y = 1e308 * 10
    equations:
        der(x) = v
        der(y) = 0
";
    let result = check_source("t", source);
    assert!(
        !result.diagnostics.has_errors(),
        "a finite-syntax transition must admit, got: {}",
        error_text(&result)
    );
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let err = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Euler,
        &SimulateOptions::default(),
    )
    .err()
    .expect("a non-finite transition action must refuse the run");
    assert!(
        err.contains("E-TRANS-008"),
        "must carry E-TRANS-008, got: {err}"
    );
    assert!(err.contains("y"), "must name the target `y`, got: {err}");
}

/// Refusal: an event payload action targeting a `state:` slot whose RHS
/// evaluates to ±Inf refuses `E-EVENT-009` naming the target and event.
#[test]
fn event_payload_action_non_finite_refuses() {
    let source = "\
emath model NonFiniteEvent:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(x: Float64):
            if x >= thr:
                y = 1e308 * 10
    equations:
        der(x) = v
        der(y) = 0
";
    let result = check_source("t", source);
    assert!(
        !result.diagnostics.has_errors(),
        "a finite-syntax event payload must admit, got: {}",
        error_text(&result)
    );
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let err = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Euler,
        &SimulateOptions::default(),
    )
    .err()
    .expect("a non-finite event payload action must refuse the run");
    assert!(
        err.contains("E-EVENT-009"),
        "must carry E-EVENT-009, got: {err}"
    );
    assert!(err.contains("y"), "must name the target `y`, got: {err}");
    assert!(err.contains("E"), "must name the event `E`, got: {err}");
}

/// Canonical RC whose event rewrites `resistance = 0` (already closed at
/// t0), making the causalized residual `voltage - 0*current - charge/c ==
/// 0` independent of the algebraic unknown `current`. The follow-up
/// `project_algebraic_into` refuses via the Newton trial: the run must
/// return Err with NO trajectory (a switched system never pretends
/// success), not an Ok-with-partial-trajectory.
const SINGULAR_SWITCH: &str = "\
emath model SingularSwitch:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
        threshold_voltage: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
    events:
        event T(voltage: Float64):
            if charge >= capacitance * threshold_voltage:
                resistance = 0
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
";

fn run_singular_switch() -> Result<(Trajectory, DAEDisposition), String> {
    let result = check_source("sc", SINGULAR_SWITCH);
    assert!(
        !result.diagnostics.has_errors(),
        "the canonical RC switch must admit, got: {}",
        error_text(&result)
    );
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("voltage".into(), Value::F64(10.0));
    inputs.insert("resistance".into(), Value::F64(5.0));
    inputs.insert("capacitance".into(), Value::F64(1.0));
    inputs.insert("threshold_voltage".into(), Value::F64(0.1));
    inputs.insert("current".into(), Value::F64(10.0)); // algebraic guess
    let mut state = BTreeMap::new();
    state.insert("charge".into(), Value::F64(0.2)); // 0.2 >= 0.1: switch is closed at t0
    simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::BackwardEuler,
        &SimulateOptions::default(),
    )
}

#[test]
fn switched_singular_system_refuses_typed() {
    match run_singular_switch() {
        Err(err) => {
            assert!(
                err.contains("singular") || err.contains("E-DAE-INIT") || err.contains("regularize"),
                "must be the typed DAE/Newton refusal (E-DAE-INIT / regularize / singular), got: {err}"
            );
        }
        Ok((trajectory, _)) => {
            panic!(
                "a resistance=0 switch makes the residual solve singular: the run must Err, \
                 but returned a trajectory with {} samples",
                trajectory.samples.len()
            );
        }
    }
}

/// The switched-singular refusal must be byte-identical across replays
/// (same source + inputs + policy → same typed refusal text).
#[test]
fn refusal_is_replay_deterministic_for_switched_singular() {
    let first = run_singular_switch().err().expect("first run must refuse");
    let second = run_singular_switch().err().expect("second run must refuse");
    assert_eq!(
        first, second,
        "the switched-singular refusal must be byte-identical on replay"
    );
}

// ---------------------------------------------------------------------------
// Pass 6: metamorphic relations (oracle-free). A relation links two outputs
// under an input transformation and needs NO golden truth. All tests sweep
// small fixed value sets — no RNG (the harness is deterministic-by-contract).
// The relations discriminate the hybrid dispatch: mutation validation (below
// in the report) proved each FAILS against a planted runner bug, so a passing
// relation is evidence, not tautology.
//
// Metamorphic family / strength kept (F×I/C ≥ 2.0; see report):
//   MR-1 F(I)/C≈4.30  MR-2 F(E)/C≈3.00  MR-3 F(C)/C≈2.45
//   MR-4 F(C)/C≈2.45  MR-5 F(C)/C≈2.20  MR-6 F′2(I)/C≈3.33
// Kept: all six. Dropped: none.
// ---------------------------------------------------------------------------

const MR_RC: &str = "\
emath model MR_RC:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
        threshold_voltage: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
    events:
        event ThresholdCrossed(voltage: Float64):
            if charge >= capacitance * threshold_voltage:
                voltage = 0
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
";

fn run_sim_params(
    source: &str,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> (Trajectory, DAEDisposition) {
    let result = check_source("tr", source);
    assert!(
        !result.diagnostics.has_errors(),
        "model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    simulate_continuous_dispositioned(
        &result.package,
        decl,
        inputs,
        state,
        0.0,
        t1,
        dt,
        method,
        &SimulateOptions::default(),
    )
    .expect("hybrid model must integrate")
}

fn mr_rc_state(charge: f64) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    state.insert("charge".into(), Value::F64(charge));
    state
}

fn mr_rc_inputs(v: f64, thr: f64) -> BTreeMap<String, Value> {
    let mut inputs = BTreeMap::new();
    inputs.insert("voltage".into(), Value::F64(v));
    inputs.insert("resistance".into(), Value::F64(1.0));
    inputs.insert("capacitance".into(), Value::F64(1.0));
    inputs.insert("threshold_voltage".into(), Value::F64(thr));
    inputs.insert("current".into(), Value::F64(v));
    inputs
}

const MR_PLAIN_RC: &str = "\
emath model PlainRC:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
        threshold_voltage: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
";

const MR_NEVER_RC: &str = "\
emath model EvRC:
    inputs:
        voltage: Float64
        resistance: Float64
        capacitance: Float64
        threshold_voltage: Float64
    algebraic:
        current: Float64
    state:
        charge: Float64
    events:
        event ThresholdCrossed(voltage: Float64):
            if charge >= capacitance * threshold_voltage:
                voltage = 0
    transitions:
        on ThresholdCrossed:
            voltage = 999
    equations:
        voltage - resistance * current - charge / capacitance == 0
        der(charge) = current
";

const MR_PAYLOAD_A: &str = "\
emath model PayloadA:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64):
            if x >= thr:
                y = v
    equations:
        der(x) = 1.0
        der(y) = 0
";

const MR_PAYLOAD_B: &str = "\
emath model PayloadB:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event E(v: Float64):
            if x >= thr:
                x = x
    transitions:
        on E:
            state.y = v
    equations:
        der(x) = 1.0
        der(y) = 0
";

const MR_PARAM_CAP_A: &str = "\
emath model CapA:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Snap(x: Float64):
            if x >= thr:
                v = 1
    transitions:
        on Snap:
            state.y = x
    equations:
        der(x) = v
        der(y) = 0
";

const MR_PARAM_CAP_B: &str = "\
emath model CapB:
    inputs:
        v: Float64
        thr: Float64
    state:
        x: Float64
        y: Float64
    events:
        event Snap(x: Float64):
            if x >= thr:
                v = 1
    transitions:
        on Snap:
            state.y = state.x
    equations:
        der(x) = v
        der(y) = 0
";

const MR_REARMED_OSC: &str = "\
emath model Oscillator:
    state:
        v: Float64
        y: Float64
    events:
        event High(v: Float64):
            if v >= 7:
                v = 0.5
        event Low(v: Float64):
            if v <= 2:
                v = 6.5
    transitions:
        on High:
            state.y = y + 1
        on Low:
            state.y = y + 1
    equations:
        der(v) = -1.0
        der(y) = 0
";

/// MR-1: timestep-refinement convergence of the firing time
/// (equivalence-with-limit). Backward Euler is first-order, so
/// t_fire(h) − t* ≈ c·h (c>0 measured) and the absolute differences
/// |t(h/2) − t(h)| ≈ c·h/2 halve as h halves — a weak-Cauchy monotone
/// shrink toward the analytic crossing t* = −ln(1 − thr/V) = ln 2.
/// Measured (before this assert): t(0.2)=0.75208, t(0.1)=0.72628,
/// t(0.05)=0.71011, t(0.025)=0.70176; differences 2.58e-2, 1.62e-2,
/// 8.35e-3 → shrink factors 1.60, 1.94. Assert halving with a >1.3 factor.
#[test]
fn metamorphic_timestep_refinement_converges_firing_time() {
    let state = mr_rc_state(0.0);
    let inputs = mr_rc_inputs(10.0, 5.0);
    let t_fire = |dt: f64| {
        run_sim_params(MR_RC, &inputs, &state, 2.0, dt, StepMethod::BackwardEuler)
            .0
            .events[0]
            .t
    };
    let t0 = t_fire(0.2);
    let t1 = t_fire(0.1);
    let t2 = t_fire(0.05);
    let t3 = t_fire(0.025);
    let ln2 = 0.693_147_180_559_945_3;
    // BE firing times lie at/after the analytic crossing and close on it.
    // The firing sample's charge must sit ON the threshold within bisection
    // accuracy. The fixed 40-iteration budget hunts the bracket to dt/2^40,
    // so the crossing lands within ~1e-11 of thr=5; a reduced budget (the
    // planted 4-iteration mutant) overshoots by ~O(bracket) — 5.05 at dt=0.2,
    // 5.02 at dt=0.1, 5.007 at dt=0.025 — and this tight assertion kills it.
    let q_at = |dt: f64| -> f64 {
        let tr = run_sim_params(MR_RC, &inputs, &state, 2.0, dt, StepMethod::BackwardEuler).0;
        let ft = tr.events[0].t;
        tr.samples
            .iter()
            .find(|s| (s.t - ft).abs() < 1e-9)
            .and_then(|s| match s.state.get("charge") {
                Some(Value::F64(v)) => Some(*v),
                _ => None,
            })
            .expect("firing sample must exist")
    };
    let thr = 5.0;
    for (dt, t) in [(0.2, t0), (0.1, t1), (0.05, t2), (0.025, t3)] {
        assert!(t >= ln2 - 1e-9, "BE firing time must sit at/above ln2, got {t}");
        let charge = q_at(dt);
        let overshoot = charge - thr;
        assert!(
            overshoot.abs() < 1e-6,
            "firing sample must sit on the threshold within bisection accuracy (40-iter budget): \
             charge={charge} at dt={dt}, off by {overshoot:.2e}"
        );
    }
    assert!(
        (t3 - ln2).abs() < 0.012,
        "finest dt must be within 1.2e-2 of ln2, got t(0.025)={t3} (convergence limit)"
    );
    let d01 = (t1 - t0).abs();
    let d12 = (t2 - t1).abs();
    let d23 = (t3 - t2).abs();
    // Monotone shrink, and each step shrinks the prior gap by >1.3.
    assert!(
        d01 > d12 && d12 > d23,
        "firing-time diffs must decrease monotonically, got d01={d01} d12={d12} d23={d23}"
    );
    assert!(d12 < d01 / 1.3, "d12={d12} must shrink d01={d01} by >1.3x (BE first-order halving)");
    assert!(d23 < d12 / 1.3, "d23={d23} must shrink d12={d12} by >1.3x (BE first-order halving)");
}

/// MR-2: no-event identity (equivalence). A model whose event condition
/// NEVER rises (threshold 50 on an RC that maxes at V=10) must produce a
/// Trajectory bit-identical (PartialEq, samples + event log) to the plain
/// model with no `events:`/`transitions:` sections, with an empty firing
/// log. The transition `voltage = 999` would corrupt the live input if it
/// were ever dispatched, so `assert_eq!` also proves the never-firing rule
/// never applied.
#[test]
fn metamorphic_no_event_identity() {
    let state = mr_rc_state(0.0);
    let inputs = mr_rc_inputs(10.0, 50.0);
    let (with_events, _) =
        run_sim_params(MR_NEVER_RC, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler);
    let (plain, _) =
        run_sim_params(MR_PLAIN_RC, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler);
    assert!(
        with_events.events.is_empty(),
        "an unreachable threshold must never fire, got {:?}",
        with_events.events
    );
    assert_eq!(
        with_events, plain,
        "never-firing event+transition model must equal the plain run (no stray dispatch)"
    );
}

/// MR-3: payload-vs-transition equivalence (commutativity of the two action
/// channels). The SAME write `state.y = v` lands once via the event payload
/// (MR_PAYLOAD_A) and once via `on E: state.y = v` (MR_PAYLOAD_B) → identical
/// Trajectory. `v` is a declared INPUT in both models so the event-param
/// capture is well-defined. Catches a double-apply or ordering bug between
/// the payload and the transition dispatch paths.
#[test]
fn metamorphic_payload_vs_transition_equiv() {
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(4.0));
    inputs.insert("thr".into(), Value::F64(2.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let a = run_sim_params(MR_PAYLOAD_A, &inputs, &state, 3.0, 0.1, StepMethod::Euler).0;
    let b = run_sim_params(MR_PAYLOAD_B, &inputs, &state, 3.0, 0.1, StepMethod::Euler).0;
    assert_eq!(a.events.len(), 1, "payload model must fire exactly once");
    assert_eq!(a.events.len(), b.events.len(), "both channels fire the same event");
    assert_eq!(a, b, "payload-write and transition-write trajectories must be identical");
    let y_final = scalar(&a.samples.last().unwrap().state, "y");
    assert!(
        (y_final - 4.0).abs() < 1e-9,
        "`state.y = v` must land the value v=4 via either channel, got y_final={y_final}"
    );
}

/// MR-4: param-capture vs direct-state-read equivalence. Transition
/// `state.y = x` (x captured via the event param Snap(x)) vs
/// `state.y = state.x` (direct state read, param unused) → identical
/// Trajectory. Catches a capture-map binding divergence from the live
/// state lane.
#[test]
fn metamorphic_param_capture_vs_direct_state_read() {
    let mut inputs = BTreeMap::new();
    inputs.insert("v".into(), Value::F64(10.0));
    inputs.insert("thr".into(), Value::F64(5.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(0.0));
    state.insert("y".into(), Value::F64(0.0));
    let a = run_sim_params(MR_PARAM_CAP_A, &inputs, &state, 1.0, 0.1, StepMethod::Euler).0;
    let b = run_sim_params(MR_PARAM_CAP_B, &inputs, &state, 1.0, 0.1, StepMethod::Euler).0;
    assert_eq!(a.events.len(), 1, "capture model must fire exactly once");
    assert_eq!(a, b, "captured-param read and direct-state read must agree");
    let y_final = scalar(&a.samples.last().unwrap().state, "y");
    assert!(
        (y_final - 5.0).abs() < 1e-4,
        "y must take the crossing x ≈ thr=5, got {y_final}"
    );
}

/// MR-5: input-scaling invariance of the firing time (equivalence under
/// scaling). t* = RC·ln(V/(V−thr)) is homogeneous of degree 0 in (V,thr);
/// the normalized charge u = q/V obeys u' = 1 − u identically for every k,
/// so t_fire is invariant to machine precision. Catches threshold
/// comparisons that do not scale, or absolute-value bugs in the condition.
#[test]
fn metamorphic_input_scaling_invariant_firing_time() {
    let state = mr_rc_state(0.0);
    let base = run_sim_params(MR_RC, &mr_rc_inputs(10.0, 5.0), &state, 2.0, 0.1, StepMethod::BackwardEuler)
        .0
        .events[0]
        .t;
    for k in [0.5, 2.0, 10.0] {
        let scaled = run_sim_params(MR_RC, &mr_rc_inputs(10.0 * k, 5.0 * k), &state, 2.0, 0.1, StepMethod::BackwardEuler)
            .0
            .events[0]
            .t;
        let delta = (scaled - base).abs();
        assert!(
            delta <= 1e-6,
            "t_fire must be invariant under (V,thr)·{k}: got {scaled} vs base {base} ({delta})"
        );
    }
}

/// MR-6: re-armed-event transition-count invariance under step refinement
/// (equivalence-with-limit). The oscillator's deterministic thresholds yield
/// an exact firing count for fixed t1; refining dt must not change the total
/// count or the final y (both equal the number of firings). Measured:
/// firings = 3 and y_final = 3 for dt ∈ {0.1, 0.05, 0.02, 0.01} — strong
/// invariant, so assert count AND y_final equality.
#[test]
fn metamorphic_rearmed_count_invariant_under_refinement() {
    let inputs = BTreeMap::new();
    let mut state = BTreeMap::new();
    state.insert("v".into(), Value::F64(8.5));
    state.insert("y".into(), Value::F64(0.0));
    let mut counts = Vec::new();
    let mut y_end = Vec::new();
    for dt in [0.1, 0.05, 0.02] {
        let tr = run_sim_params(MR_REARMED_OSC, &inputs, &state, 5.0, dt, StepMethod::Euler).0;
        counts.push(tr.events.len());
        y_end.push(scalar(&tr.samples.last().unwrap().state, "y"));
    }
    assert_eq!(counts[0], counts[1], "firing count must not change as dt halves");
    assert_eq!(counts[1], counts[2], "firing count must not change as dt halves");
    assert_eq!(y_end[0], y_end[1], "final y must not change as dt halves");
    assert_eq!(y_end[1], y_end[2], "final y must not change as dt halves");
    assert_eq!(
        counts[0], 3,
        "for t1=5 the oscillator must fire exactly 3 times (High at t0, Low at t0, Low at t=4.5), got {}",
        counts[0]
    );
    assert!(
        (y_end[0] - counts[0] as f64).abs() < 1e-9,
        "y counts the firings: y_final={} must equal {}",
        y_end[0],
        counts[0]
    );
}

