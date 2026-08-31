//! emath-dae-events-seq (r3-dynamical-03lh ch7, event-execution slice):
//! failure-first evidence for generic event-triggered execution of
//! admitted `.emath` event payloads.
//!
//! Contracts (each must FAIL against the pre-glue surface — payload
//! suites were silently ignored and simulation never fired events):
//! - A model with an event payload (`if <condition>:` action on a
//!   declared input/state slot) FIRES the event exactly once per
//!   rising edge of its condition, snaps a trajectory sample at the
//!   bisected crossing, and persists the action into later steps so
//!   the trajectory's behavior switches at the threshold.
//! - Ties break deterministically (declaration order, one event per
//!   accepted step) and same source + inputs + policy replays the
//!   same firing log (determinism class).
//! - Conditions that never rise never fire; the trajectory then equals
//!   the plain non-event run.
//! - Malformed payloads refuse typed at admission: unknown/non-slot
//!   targets (E-EVENT-001), non-Boolean conditions (E-EVENT-002),
//!   `else` arms (E-EVENT-003), non-numeric actions (E-EVENT-004),
//!   non-Float64 slots (E-EVENT-005). Bare `event Name(...)` surface
//!   declarations stay admitted (surface-only, never scheduled).

mod common;

use crate::common::{check_source, error_text};
use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::{SimulateOptions, StepMethod, simulate_continuous_dispositioned};
use std::collections::BTreeMap;

/// The hybrid RC circuit from `language/examples/numerical/`:
/// charge builds while the source is connected; when the capacitor
/// reaches `capacitance * threshold_voltage`, the event action
/// disconnects the source (`voltage = 0`) and the circuit discharges
/// through the resistor.
const SWITCH_RC: &str = "\
emath model SwitchRC:
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

fn switch_inputs(threshold: f64, current_guess: f64) -> BTreeMap<String, Value> {
    let mut inputs = BTreeMap::new();
    inputs.insert("voltage".into(), Value::F64(10.0));
    inputs.insert("resistance".into(), Value::F64(1.0));
    inputs.insert("capacitance".into(), Value::F64(1.0));
    inputs.insert("threshold_voltage".into(), Value::F64(threshold));
    inputs.insert("current".into(), Value::F64(current_guess));
    inputs
}

fn switch_state(charge: f64) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    state.insert("charge".into(), Value::F64(charge));
    state
}

fn run_sim(
    source: &str,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> (emath_exec_ir::Trajectory, emath_exec_ir::DAEDisposition) {
    let result = check_source("sw", source);
    assert!(
        !result.diagnostics.has_errors(),
        "event model must admit, got: {:?}",
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
        1.0,
        0.1,
        StepMethod::BackwardEuler,
        &SimulateOptions::default(),
    )
    .expect("hybrid DAE must integrate with a disposition")
}

/// THE named contract: the event fires exactly once at the bisected
/// crossing of `charge >= capacitance * threshold_voltage`, and the
/// action (`voltage = 0`) switches the circuit from charging to
/// discharging — the final charge sits BELOW the no-event control.
#[test]
fn events_fire_and_switch() {
    let (trajectory, disposition) = run_sim(
        SWITCH_RC,
        &switch_inputs(5.0, 10.0),
        &switch_state(0.0),
    );
    // Still an index-1 DAE with a consistent initialization.
    assert_eq!(disposition.index, emath_exec_ir::DAEIndex::One);
    assert_eq!(disposition.continuation, None);
    // Exactly one firing, at the charge=5 crossing (BE dt=0.1:
    // q(0.7)=4.8684 < 5, q(0.8)=5.3349 >= 5).
    assert_eq!(trajectory.events.len(), 1, "event must fire once");
    assert_eq!(trajectory.events[0].name, "ThresholdCrossed");
    let fire_t = trajectory.events[0].t;
    assert!(
        (0.7..0.8).contains(&fire_t),
        "crossing must bisect into the 0.7..0.8 bracket, got t={fire_t}"
    );
    // The firing sample sits on the threshold within bisection accuracy.
    let crossed_charge = trajectory
        .samples
        .iter()
        .find(|sample| (sample.t - fire_t).abs() < 1e-9)
        .and_then(|sample| match sample.state.get("charge") {
            Some(Value::F64(v)) => Some(*v),
            _ => None,
        })
        .expect("firing sample must exist in the trajectory");
    assert!(
        (crossed_charge - 5.0).abs() < 1e-4,
        "crossing sample must sit on the threshold, got charge={crossed_charge}"
    );
    // After the crossing the source is disconnected: charge decreases
    // monotonically (discharge through R), so the final charge is
    // strictly below the threshold AND below the no-event control
    // (no-event q(1) = 10(1 - 1.1^-10) ≈ 6.1446).
    let samples_after: Vec<&emath_exec_ir::TrajectorySample> = trajectory
        .samples
        .iter()
        .filter(|sample| sample.t >= fire_t)
        .collect();
    for pair in samples_after.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (qa, qb) = match (
            a.state.get("charge"),
            b.state.get("charge"),
        ) {
            (Some(Value::F64(x)), Some(Value::F64(y))) => (*x, *y),
            other => panic!("non-scalar charge in samples: {other:?}"),
        };
        assert!(
            qa >= qb - 1e-9,
            "charge must not rise after the switch: {qa} -> {qb}"
        );
    }
    let q_final = match trajectory.samples.last().unwrap().state.get("charge") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    assert!(
        q_final < 5.0,
        "switched circuit must discharge below the threshold, got q(1)={q_final}"
    );
    let control = 10.0 * (1.0 - 1.1f64.powf(-10.0));
    assert!(
        q_final < control - 1.0,
        "switching must change behavior vs the no-event control: q(1)={q_final} ~= control {control}"
    );
}

/// A threshold the charge never reaches never fires: the trajectory is
/// the plain non-event run (firing log empty, unchanging physics).
#[test]
fn event_below_reachable_threshold_never_fires() {
    let (trajectory, _) = run_sim(
        SWITCH_RC,
        &switch_inputs(20.0, 10.0),
        &switch_state(0.0),
    );
    assert!(
        trajectory.events.is_empty(),
        "an unreachable threshold must never fire"
    );
    let q_final = match trajectory.samples.last().unwrap().state.get("charge") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    let expected = 10.0 * (1.0 - 1.1f64.powf(-10.0));
    assert!(
        (q_final - expected).abs() < 0.01,
        "non-firing run must equal the plain RC run: q(1)={q_final}, expected {expected}"
    );
}

/// Replay: same source + inputs + policy → identical trajectory
/// INCLUDING the firing log (determinism class).
#[test]
fn event_firing_is_replay_deterministic() {
    let run = || {
        run_sim(SWITCH_RC, &switch_inputs(5.0, 10.0), &switch_state(0.0)).0
    };
    let a = run();
    let b = run();
    assert_eq!(a, b, "same source + inputs + policy must replay the same firing log");
}

/// Refusal: the action target must be a declared input or state slot.
#[test]
fn event_unknown_action_target_refuses() {
    let source = SWITCH_RC.replace("voltage = 0", "ghost_slot = 0");
    let result = check_source("sw", &source);
    assert!(
        result.diagnostics.has_errors(),
        "unknown action target must refuse at admission"
    );
    let text = result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(text.contains("E-EVENT-001"), "must carry E-EVENT-001, got: {text}");
}

/// Refusal: the condition must be Boolean.
#[test]
fn event_non_boolean_condition_refuses() {
    let source = SWITCH_RC.replace(
        "if charge >= capacitance * threshold_voltage:",
        "if charge + 1.0:",
    );
    let result = check_source("sw", &source);
    assert!(
        result.diagnostics.has_errors(),
        "non-Boolean condition must refuse at admission"
    );
    let text = result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(text.contains("E-EVENT-002"), "must carry E-EVENT-002, got: {text}");
}

/// Refusal: a deterministic hybrid event has ONE arm — `else` refuses.
#[test]
fn event_else_arm_refuses() {
    let source = SWITCH_RC.replace(
        "if charge >= capacitance * threshold_voltage:\n                voltage = 0",
        "if charge >= capacitance * threshold_voltage:\n                    voltage = 0\n                else:\n                    voltage = 10",
    );
    let result = check_source("sw", &source);
    assert!(
        result.diagnostics.has_errors(),
        "else arms must refuse at admission"
    );
    let text = result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(text.contains("E-EVENT-003"), "must carry E-EVENT-003, got: {text}");
}

/// Refusal: the action value must be a numeric scalar.
#[test]
fn event_non_numeric_action_refuses() {
    let source = SWITCH_RC.replace("voltage = 0", "voltage = [1.0, 2.0]");
    let result = check_source("sw", &source);
    assert!(
        result.diagnostics.has_errors(),
        "a vector action value must refuse at admission"
    );
    let text = result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(text.contains("E-EVENT-004"), "must carry E-EVENT-004, got: {text}");
}

/// Parse → format → reparse identity. The lossless tree formatter round-trips
/// the canonical SWITCH_RC source; re-admitting the formatted text must give
/// the same verdict (clean) and the same events/transition SIR (replay that
/// the formatter did not perturb the admitted model).
#[test]
fn parse_format_reparse_identity() {
    use emath_core::id::FileId;
    use emath_syntax::{format_lossless, parse_lossless};
    // Original admission.
    let original = check_source("sw", SWITCH_RC);
    assert!(
        !original.diagnostics.has_errors(),
        "canonical SWITCH_RC must admit clean"
    );
    // Parse → format via the lossless tree.
    let pl = parse_lossless(SWITCH_RC, FileId(0), &Limits::default());
    assert!(
        pl.diagnostics.errors().next().is_none(),
        "lossless parse must be error-free"
    );
    let formatted = format_lossless(&pl);
    // Reparse + re-admit the formatted text.
    let round = check_source("sw", &formatted);
    let text = error_text(&round);
    assert!(
        !round.diagnostics.has_errors(),
        "formatted source must still admit, got: {text}"
    );
    let (do0, dr) = (
        &original.package.declarations[0],
        &round.package.declarations[0],
    );
    assert_eq!(
        original.package.declarations.len(),
        round.package.declarations.len(),
        "declaration count must survive format round-trip"
    );
    assert_eq!(
        original.package.events.get(&do0.id),
        round.package.events.get(&dr.id),
        "events SIR must survive format round-trip"
    );
    assert_eq!(
        original.package.transitions.get(&do0.id),
        round.package.transitions.get(&dr.id),
        "transitions SIR must survive format round-trip"
    );
    assert_eq!(
        original.package.exprs.len(),
        round.package.exprs.len(),
        "expr arena size must survive format round-trip"
    );
}

/// Refusal: the action targets an Int-typed slot (non-Float64) → E-EVENT-005.
/// A declared input Int slot is refused as an event payload write.
#[test]
fn event_non_float64_slot_refuses() {
    let source = "\
emath model IntSlot:
    inputs:
        v: Float64
        thr: Float64
        count: Int
    state:
        x: Float64
    events:
        event E(v: Float64):
            if x >= thr:
                count = 1
    equations:
        der(x) = v
";
    let result = check_source("sw", &source);
    assert!(
        result.diagnostics.has_errors(),
        "an Int-typed slot must refuse as an event payload target"
    );
    let text = result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(text.contains("E-EVENT-005"), "must carry E-EVENT-005, got: {text}");
}

/// Re-arm/period check: the event fires AGAIN each time its condition
/// re-enters from below — the rise-gate resets when the action drives
/// the condition false. `temp` climbs at rate 1; the event resets it to
/// 0 at 2.0, so the run is a period-2 sawtooth: exactly two firings by
/// t=5 (near t=2 and t=4), none at t0, and none between firings (no
/// chatter while the condition reads false after each reset).
#[test]
fn event_rearms_after_action_resets_condition() {
    let source = "\
emath model Thermostat:
    state:
        temp: Float64
    events:
        event Reset(temp: Float64):
            if temp >= 2:
                temp = 0.0
    equations:
        der(temp) = 1.0
";
    let result = check_source("th", source);
    assert!(
        !result.diagnostics.has_errors(),
        "thermostat model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    let inputs = BTreeMap::new();
    let mut state = BTreeMap::new();
    state.insert("temp".into(), Value::F64(0.0));
    let (trajectory, _) = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &state,
        0.0,
        5.0,
        0.1,
        StepMethod::Euler,
        &SimulateOptions::default(),
    )
    .expect("thermostat run must integrate");
    assert_eq!(
        trajectory.events.len(),
        2,
        "exactly two firings by t=5 (period-2 sawtooth), got {:?}",
        trajectory.events
    );
    assert_eq!(trajectory.events[0].name, "Reset");
    assert_eq!(trajectory.events[1].name, "Reset");
    // First fire at the first threshold crossing (t≈2, within one step).
    assert!(
        (1.85..=2.15).contains(&trajectory.events[0].t),
        "first fire must land at the t≈2 crossing, got {}",
        trajectory.events[0].t
    );
    // The period is exactly the reset cycle: rise from 0 to 2 at rate 1.
    let period = trajectory.events[1].t - trajectory.events[0].t;
    assert!(
        (period - 2.0).abs() < 0.05,
        "re-arm period must be 2 (rise 0→2 at rate 1), got {period}"
    );
    // After the second reset the state is mid-climb: past 0, below the
    // next threshold (no third fire, no stuck-at-zero, no runaway).
    let last_temp = match trajectory.samples.last().unwrap().state.get("temp") {
        Some(Value::F64(v)) => *v,
        other => panic!("non-scalar temp: {other:?}"),
    };
    assert!(
        (0.0..1.2).contains(&last_temp),
        "after the reset the temperature must be mid-climb, got {last_temp}"
    );
}
