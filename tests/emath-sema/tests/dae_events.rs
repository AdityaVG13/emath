//! emath-dae-events-seq (ch7, event-execution slice):
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

use emath_core::{id::FileId, limits::Limits};
use emath_exec_ir::interp::Value;
use emath_exec_ir::{SimulateOptions, StepMethod, simulate_continuous_dispositioned};
use emath_syntax::{format_lossless, parse_lossless};
use emath_test_harness::{Probe, Source, boot};
use std::collections::BTreeMap;

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

const THERMOSTAT: &str = "\
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

const INT_SLOT: &str = "\
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

fn switch_inputs(threshold: f64, guess: f64) -> BTreeMap<String, Value> {
    let mut inputs = BTreeMap::new();
    inputs.insert("voltage".into(), Value::F64(10.0));
    inputs.insert("resistance".into(), Value::F64(1.0));
    inputs.insert("capacitance".into(), Value::F64(1.0));
    inputs.insert("threshold_voltage".into(), Value::F64(threshold));
    inputs.insert("current".into(), Value::F64(guess));
    inputs
}

fn switch_state(charge: f64) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    state.insert("charge".into(), Value::F64(charge));
    state
}

fn charge_of(sample: &emath_exec_ir::TrajectorySample) -> Option<f64> {
    match sample.state.get("charge") {
        Some(Value::F64(v)) => Some(*v),
        _ => None,
    }
}

#[test]
fn dae_events() {
    boot();
    let mut p = Probe::new("event payloads fire once per rising edge and switch the trajectory");
    p.case("fire-switch", |p| {
        let result = Source::from_str("sw", SWITCH_RC).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("fire-switch:decl", "no declaration to simulate");
            return;
        }
        let decl = &result.package.declarations[0];
        let out = simulate_continuous_dispositioned(
            &result.package,
            decl,
            &switch_inputs(5.0, 10.0),
            &switch_state(0.0),
            0.0,
            1.0,
            0.1,
            StepMethod::BackwardEuler,
            &SimulateOptions::default(),
        );
        match out {
            Err(error) => {
                p.fail("fire-switch:run", format!("hybrid DAE must integrate: {error}"));
            }
            Ok((trajectory, disposition)) => {
                p.eq("fire-switch:index", disposition.index, emath_exec_ir::DAEIndex::One);
                p.eq("fire-switch:continuation", disposition.continuation.is_none(), true);
                p.eq("fire-switch:count", trajectory.events.len(), 1);
                let firing = match trajectory.events.first() {
                    Some(firing) => firing,
                    None => {
                        p.fail("fire-switch:fired", "event must fire once");
                        return;
                    }
                };
                p.eq("fire-switch:name", firing.name.clone(), "ThresholdCrossed".to_string());
                let fire_t = firing.t;
                p.eq("fire-switch:bracket", (0.7..0.8).contains(&fire_t), true);
                match trajectory.samples.iter().find(|s| (s.t - fire_t).abs() < 1e-9) {
                    None => {
                        p.fail("fire-switch:sample", "firing sample must sit in the trajectory");
                    }
                    Some(sample) => match charge_of(sample) {
                        None => {
                            p.fail("fire-switch:charge", "crossing charge must be scalar");
                        }
                        Some(q) => {
                            p.close("fire-switch:on-threshold", q, 5.0, 1e-4);
                        }
                    },
                };
                let after: Vec<f64> = trajectory
                    .samples
                    .iter()
                    .filter(|s| s.t >= fire_t)
                    .filter_map(charge_of)
                    .collect();
                p.eq("fire-switch:after-nonempty", after.is_empty(), false);
                p.eq(
                    "fire-switch:discharge",
                    after.windows(2).all(|w| w[0] >= w[1] - 1e-9),
                    true,
                );
                let q_final = after.last().copied().unwrap_or(f64::NAN);
                p.eq("fire-switch:below-threshold", q_final < 5.0, true);
                let control = 10.0 * (1.0 - 1.1f64.powf(-10.0));
                p.eq("fire-switch:switched", q_final < control - 1.0, true);
            }
        };
    });
    p.case("never-fires", |p| {
        let result = Source::from_str("sw", SWITCH_RC).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("never-fires:decl", "no declaration to simulate");
            return;
        }
        let decl = &result.package.declarations[0];
        let out = simulate_continuous_dispositioned(
            &result.package,
            decl,
            &switch_inputs(20.0, 10.0),
            &switch_state(0.0),
            0.0,
            1.0,
            0.1,
            StepMethod::BackwardEuler,
            &SimulateOptions::default(),
        );
        match out {
            Err(error) => {
                p.fail("never-fires:run", format!("non-firing run must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                p.eq("never-fires:count", trajectory.events.len(), 0);
                match trajectory.samples.last().and_then(charge_of) {
                    None => {
                        p.fail("never-fires:charge", "final charge must be scalar");
                    }
                    Some(q) => {
                        let expected = 10.0 * (1.0 - 1.1f64.powf(-10.0));
                        p.close("never-fires:plain-run", q, expected, 0.01);
                    }
                };
            }
        };
    });
    p.case("replay", |p| {
        let result = Source::from_str("sw", SWITCH_RC).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("replay:decl", "no declaration to simulate");
            return;
        }
        let decl = &result.package.declarations[0];
        let run = || {
            simulate_continuous_dispositioned(
                &result.package,
                decl,
                &switch_inputs(5.0, 10.0),
                &switch_state(0.0),
                0.0,
                1.0,
                0.1,
                StepMethod::BackwardEuler,
                &SimulateOptions::default(),
            )
            .map(|(trajectory, _)| trajectory)
        };
        match (run(), run()) {
            (Ok(a), Ok(b)) => {
                p.eq("replay:trajectory", a, b);
            }
            (first, second) => {
                p.fail("replay:run", format!("replay runs must succeed: {first:?} vs {second:?}"));
            }
        };
    });
    p.case("refuse-unknown-target", |p| {
        Source::from_str("sw", &SWITCH_RC.replace("voltage = 0", "ghost_slot = 0"))
            .must_refuse(&mut *p, &["E-EVENT-001"]);
    });
    p.case("refuse-non-boolean", |p| {
        Source::from_str(
            "sw",
            &SWITCH_RC.replace("if charge >= capacitance * threshold_voltage:", "if charge + 1.0:"),
        )
        .must_refuse(&mut *p, &["E-EVENT-002"]);
    });
    p.case("refuse-else-arm", |p| {
        Source::from_str(
            "sw",
            &SWITCH_RC.replace(
                "if charge >= capacitance * threshold_voltage:\n                voltage = 0",
                "if charge >= capacitance * threshold_voltage:\n                    voltage = 0\n                else:\n                    voltage = 10",
            ),
        )
        .must_refuse(&mut *p, &["E-EVENT-003"]);
    });
    p.case("refuse-non-numeric", |p| {
        Source::from_str("sw", &SWITCH_RC.replace("voltage = 0", "voltage = [1.0, 2.0]"))
            .must_refuse(&mut *p, &["E-EVENT-004"]);
    });
    p.case("refuse-int-slot", |p| {
        Source::from_str("sw", INT_SLOT).must_refuse(&mut *p, &["E-EVENT-005"]);
    });
    p.case("format-identity", |p| {
        let original = Source::from_str("sw", SWITCH_RC).must_admit(&mut *p);
        let parsed = parse_lossless(SWITCH_RC, FileId(0), &Limits::default());
        p.eq("format-identity:parse", parsed.diagnostics.errors().next().is_none(), true);
        let formatted = format_lossless(&parsed);
        let round = Source::from_str("sw-format", formatted.as_str()).must_admit(&mut *p);
        if original.package.declarations.is_empty() || round.package.declarations.is_empty() {
            p.fail("format-identity:decl", "both passes must admit a declaration");
            return;
        }
        let (first, second) = (&original.package.declarations[0], &round.package.declarations[0]);
        p.eq(
            "format-identity:decl-count",
            original.package.declarations.len(),
            round.package.declarations.len(),
        );
        p.eq(
            "format-identity:events",
            original.package.events.get(&first.id),
            round.package.events.get(&second.id),
        );
        p.eq(
            "format-identity:transitions",
            original.package.transitions.get(&first.id),
            round.package.transitions.get(&second.id),
        );
        p.eq(
            "format-identity:exprs",
            original.package.exprs.len(),
            round.package.exprs.len(),
        );
    });
    p.case("rearm", |p| {
        let result = Source::from_str("th", THERMOSTAT).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("rearm:decl", "no declaration to simulate");
            return;
        }
        let decl = &result.package.declarations[0];
        let inputs = BTreeMap::new();
        let mut state = BTreeMap::new();
        state.insert("temp".into(), Value::F64(0.0));
        let out = simulate_continuous_dispositioned(
            &result.package,
            decl,
            &inputs,
            &state,
            0.0,
            5.0,
            0.1,
            StepMethod::Euler,
            &SimulateOptions::default(),
        );
        match out {
            Err(error) => {
                p.fail("rearm:run", format!("thermostat run must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                p.eq("rearm:count", trajectory.events.len(), 2);
                if trajectory.events.len() == 2 {
                    p.eq("rearm:first", trajectory.events[0].name.clone(), "Reset".to_string());
                    p.eq("rearm:second", trajectory.events[1].name.clone(), "Reset".to_string());
                    p.eq("rearm:t0", (1.85..=2.15).contains(&trajectory.events[0].t), true);
                    let period = trajectory.events[1].t - trajectory.events[0].t;
                    p.close("rearm:period", period, 2.0, 0.05);
                } else {
                    p.fail("rearm:two-firings", "period check needs exactly two firings");
                }
                let temp_of = |sample: &emath_exec_ir::TrajectorySample| match sample.state.get("temp") {
                    Some(Value::F64(v)) => Some(*v),
                    _ => None,
                };
                match trajectory.samples.last().and_then(temp_of) {
                    None => {
                        p.fail("rearm:temp", "final temp must be scalar");
                    }
                    Some(temp) => {
                        p.eq("rearm:mid-climb", (0.0..1.2).contains(&temp), true);
                    }
                };
            }
        };
    });
    p.finish();
}
