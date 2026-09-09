//! emath-dae-transitions-seq (ch7, transitions slice):
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
//! slot with a numeric value. SIR-lowers every action to a kept
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

use emath_exec_ir::interp::Value;
use emath_exec_ir::{
    DAEDisposition, SimulateOptions, StepMethod, Trajectory, simulate_continuous_dispositioned,
};
use emath_test_harness::{Probe, Source, boot};
use std::collections::BTreeMap;

const POSITIVE: &str = "\
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

const T_SIR: &str = "\
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

const T_NONMODEL: &str = "\
emath function F:
    inputs:
        x: Float64
    transitions:
        on Missing:
            x = 1
    define:
        y = x
";

const T_ALGEBRAIC: &str = "\
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

const T_NOCAPTURE: &str = "\
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

const T_UNDECLARED: &str = "\
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

fn t_spine(body: &str) -> String {
    format!(
        "emath model TModel:\n    inputs:\n        v: Float64\n        thr: Float64\n    state:\n        x: Float64\n        y: Float64\n    events:\n        event E(v: Float64)\n    transitions:\n{body}    equations:\n        der(x) = v\n        der(y) = 0\n"
    )
}

fn vars(pairs: &[(&str, f64)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, v)| (name.to_string(), Value::F64(*v)))
        .collect()
}

fn scalar(map: &BTreeMap<String, Value>, name: &str) -> Option<f64> {
    match map.get(name) {
        Some(Value::F64(v)) => Some(*v),
        Some(Value::I64(v)) => Some(*v as f64),
        _ => None,
    }
}

fn error_text(result: &emath_sema::admit::CheckResult) -> String {
    result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn run(
    probe: &mut Probe,
    name: &str,
    source: &str,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<(Trajectory, DAEDisposition), String> {
    let result = Source::from_str(name, source).must_admit(probe);
    if result.package.declarations.is_empty() {
        probe.fail(format!("{name}:decl"), "no declaration to simulate");
        return Err("admission failed; see diagnostics".to_string());
    }
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
}

fn check_switched(probe: &mut Probe, name: &str, trajectory: &Trajectory, event: &str) {
    probe.eq(format!("{name}:count"), trajectory.events.len(), 1);
    let firing = match trajectory.events.first() {
        Some(firing) => firing,
        None => {
            probe.fail(format!("{name}:fired"), "event must fire exactly once");
            return;
        }
    };
    probe.eq(format!("{name}:name"), firing.name.clone(), event.to_string());
    let fire_t = firing.t;
    let switched = trajectory
        .samples
        .iter()
        .filter(|s| s.t >= fire_t)
        .all(|s| scalar(&s.state, "y") == Some(5.0));
    probe.eq(format!("{name}:switched"), switched, true);
    probe.eq(
        format!("{name}:after-exists"),
        trajectory.samples.iter().any(|s| s.t >= fire_t),
        true,
    );
    match trajectory.samples.last().and_then(|s| scalar(&s.state, "y")) {
        Some(y) => {
            probe.close(format!("{name}:final"), y, 5.0, 1e-9);
        }
        None => {
            probe.fail(format!("{name}:final"), "final y must be scalar");
        }
    };
}

#[test]
fn dae_transitions() {
    boot();
    let mut p = Probe::new("transitions admit typed rules, lower to SIR, and dispatch in order");
    p.case("admit", |p| {
        Source::from_str("t", POSITIVE).must_admit(&mut *p);
    });
    p.case("sweep", |p| {
        let rows: Vec<(&str, String, &str, &[&str])> = vec![
            (
                "unknown-trigger",
                t_spine("        on Missing:\n            state.x = 1\n"),
                "E-TRANS-001",
                &["Missing"],
            ),
            (
                "bare-unknown",
                t_spine("        on E:\n            zzz = 1\n"),
                "E-TRANS-002",
                &["zzz"],
            ),
            (
                "dotted-unknown",
                t_spine("        on E:\n            state.zzz = 1\n"),
                "E-TRANS-002",
                &["state.zzz"],
            ),
            (
                "non-assignment",
                t_spine("        on E:\n            sub:\n                foo = 1\n"),
                "E-TRANS-003",
                &[],
            ),
            (
                "non-numeric",
                t_spine("        on E:\n            state.y = [1.0]\n"),
                "E-TRANS-003",
                &[],
            ),
            ("empty-body", t_spine("        on E:\n"), "E-TRANS-004", &[]),
            (
                "algebraic",
                T_ALGEBRAIC.to_string(),
                "E-TRANS-005",
                &["a"],
            ),
            (
                "param-no-capture",
                T_NOCAPTURE.to_string(),
                "E-TRANS-006",
                &["qq", "F"],
            ),
        ];
        for (name, source, code, entities) in &rows {
            let result = Source::from_str("t", source.as_str()).check();
            let text = error_text(&result);
            p.eq(format!("sweep/{name}:refused"), result.diagnostics.has_errors(), true);
            p.contains(format!("sweep/{name}:code"), &text, code);
            for needle in entities.iter() {
                p.contains(format!("sweep/{name}:names"), &text, needle);
            }
        }
    });
    p.case("nonmodel", |p| {
        Source::from_str("f", T_NONMODEL).must_refuse(&mut *p, &["E-TRANS-001"]);
    });
    p.case("undeclared-name", |p| {
        let result = Source::from_str("t", T_UNDECLARED).check();
        let text = error_text(&result);
        p.eq("undeclared-name:refused", result.diagnostics.has_errors(), true);
        p.eq("undeclared-name:code", text.contains("E-TYPE-002"), true);
        p.eq("undeclared-name:names-var", text.contains("zzz"), true);
    });
    p.case("sir", |p| {
        let result = Source::from_str("t", T_SIR).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("sir:decl", "no declaration to inspect");
            return;
        }
        let decl = &result.package.declarations[0];
        match result.package.transitions.get(&decl.id) {
            None => {
                p.fail("sir:rules", "transitions map must carry this declaration");
            }
            Some(rules) => {
                p.eq("sir:rule-count", rules.len(), 1);
                if rules.len() == 1 {
                    let rule = &rules[0];
                    p.eq("sir:trigger", rule.trigger.clone(), "E".to_string());
                    p.eq("sir:action-count", rule.actions.len(), 1);
                    if rule.actions.len() == 1 {
                        let action = &rule.actions[0];
                        p.eq("sir:target", action.target.clone(), "x".to_string());
                        p.eq("sir:is-state", action.is_state, true);
                        p.eq(
                            "sir:expr-arena",
                            result.package.expr(action.expr).is_some(),
                            true,
                        );
                    }
                }
            }
        };
        match result.package.events.get(&decl.id) {
            None => {
                p.fail("sir:events", "events map must carry the payload suite");
            }
            Some(events) => {
                p.eq("sir:event-count", events.len(), 1);
                if events.len() == 1 {
                    p.eq("sir:event-name", events[0].name.clone(), "E".to_string());
                    p.eq("sir:params", events[0].params.clone(), vec!["v".to_string()]);
                }
            }
        };
    });
    p.case("admission-replay", |p| {
        let a = Source::from_str("hy", HYBRID_FULL).check();
        let b = Source::from_str("hy", HYBRID_FULL).check();
        let (ta, tb) = (error_text(&a), error_text(&b));
        p.eq("admission-replay:a-clean", ta.clone(), String::new());
        p.eq("admission-replay:b-clean", tb.clone(), String::new());
        if ta.is_empty() && tb.is_empty() {
            if a.package.declarations.is_empty() || b.package.declarations.is_empty() {
                p.fail("admission-replay:decl", "both passes must admit a declaration");
                return;
            }
            let (da, db) = (&a.package.declarations[0], &b.package.declarations[0]);
            p.eq(
                "admission-replay:decl-count",
                a.package.declarations.len(),
                b.package.declarations.len(),
            );
            p.eq(
                "admission-replay:events",
                a.package.events.get(&da.id),
                b.package.events.get(&db.id),
            );
            p.eq(
                "admission-replay:transitions",
                a.package.transitions.get(&da.id),
                b.package.transitions.get(&db.id),
            );
            p.eq(
                "admission-replay:exprs",
                a.package.exprs.len(),
                b.package.exprs.len(),
            );
            p.eq("admission-replay:diag-text", ta, tb);
        }
    });
    p.case("switch", |p| {
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
        match run(
            &mut *p,
            "switch",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Err(error) => {
                p.fail("switch:run", format!("must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                check_switched(&mut *p, "switch", &trajectory, "Threshold");
                match trajectory.samples.iter().find(|s| {
                    s.t < trajectory.events.first().map(|f| f.t).unwrap_or(f64::NAN)
                }) {
                    None => {
                        p.fail("switch:before", "a pre-crossing sample must exist");
                    }
                    Some(before) => {
                        p.eq("switch:y0", scalar(&before.state, "y"), Some(0.0));
                    }
                };
            }
        };
    });
    p.case("bare", |p| {
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
        match run(
            &mut *p,
            "bare",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Err(error) => {
                p.fail("bare:run", format!("must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                check_switched(&mut *p, "bare", &trajectory, "Threshold");
            }
        };
    });
    p.case("t0", |p| {
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
        match run(
            &mut *p,
            "t0",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 6.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Err(error) => {
                p.fail("t0:run", format!("must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                p.eq("t0:count", trajectory.events.len(), 1);
                let firing = match trajectory.events.first() {
                    Some(firing) => firing,
                    None => {
                        p.fail("t0:fired", "t0-hold fires once");
                        return;
                    }
                };
                p.eq("t0:at-zero", firing.t, 0.0);
                match trajectory.samples.first().and_then(|s| scalar(&s.state, "y")) {
                    Some(y) => {
                        p.close("t0:first-sample", y, 5.0, 1e-9);
                    }
                    None => {
                        p.fail("t0:first-sample", "first sample must carry scalar y");
                    }
                };
            }
        };
    });
    p.case("order", |p| {
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
        match run(
            &mut *p,
            "order",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0), ("z", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Err(error) => {
                p.fail("order:run", format!("must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                p.eq("order:count", trajectory.events.len(), 1);
                let firing = match trajectory.events.first() {
                    Some(firing) => firing,
                    None => {
                        p.fail("order:fired", "only Threshold fires");
                        return;
                    }
                };
                p.eq("order:name", firing.name.clone(), "Threshold".to_string());
                match trajectory.samples.last() {
                    None => {
                        p.fail("order:last", "trajectory must have samples");
                    }
                    Some(last) => {
                        p.eq("order:last-wins", scalar(&last.state, "y"), Some(2.0));
                        p.eq("order:never-fired", scalar(&last.state, "z"), Some(0.0));
                    }
                };
            }
        };
    });
    p.case("capture", |p| {
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
        match run(
            &mut *p,
            "capture",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Err(error) => {
                p.fail("capture:run", format!("must integrate: {error}"));
            }
            Ok((trajectory, _)) => {
                p.eq("capture:count", trajectory.events.len(), 1);
                let firing = match trajectory.events.first() {
                    Some(firing) => firing,
                    None => {
                        p.fail("capture:fired", "Snap fires once at the crossing");
                        return;
                    }
                };
                p.eq("capture:name", firing.name.clone(), "Snap".to_string());
                let fire_t = firing.t;
                let captured = trajectory
                    .samples
                    .iter()
                    .find(|s| (s.t - fire_t).abs() < 1e-9)
                    .and_then(|s| scalar(&s.state, "y"));
                match captured {
                    None => {
                        p.fail("capture:value", "firing sample must carry scalar y");
                    }
                    Some(y) => {
                        p.close("capture:at-threshold", y, 5.0, 1e-4);
                        match trajectory.samples.last().and_then(|s| scalar(&s.state, "y")) {
                            Some(last) => {
                                p.close("capture:persists", (y - last).abs(), 0.0, 1e-4);
                            }
                            None => {
                                p.fail("capture:persists", "final y must be scalar");
                            }
                        };
                    }
                };
            }
        };
    });
    p.case("osc", |p| {
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
        let state = vars(&[("v", 8.5), ("y", 0.0)]);
        let inputs = BTreeMap::new();
        let a = run(&mut *p, "osc-a", source, &inputs, &state, 5.0, 0.1, StepMethod::Euler);
        let b = run(&mut *p, "osc-b", source, &inputs, &state, 5.0, 0.1, StepMethod::Euler);
        match (a, b) {
            (Ok((ta, _)), Ok((tb, _))) => {
                let names: Vec<String> = ta.events.iter().map(|f| f.name.clone()).collect();
                p.eq("osc:high", names.iter().any(|n| n == "High"), true);
                p.eq("osc:low", names.iter().any(|n| n == "Low"), true);
                p.eq("osc:rearmed", names.len() >= 3, true);
                match ta.samples.last().and_then(|s| scalar(&s.state, "y")) {
                    Some(y) => {
                        p.close("osc:counts-firings", y, names.len() as f64, 1e-9);
                    }
                    None => {
                        p.fail("osc:counts-firings", "final y must be scalar");
                    }
                };
                p.eq("osc:replay", ta, tb);
            }
            (first, second) => {
                p.fail("osc:run", format!("both runs must integrate: {first:?} vs {second:?}"));
            }
        };
    });
    p.case("nonfinite-trans", |p| {
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
        match run(
            &mut *p,
            "nonfinite-trans",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Ok(_) => {
                p.fail("nonfinite-trans:refuse", "a non-finite transition action must refuse");
            }
            Err(error) => {
                p.contains("nonfinite-trans:code", &error, "E-TRANS-008");
                p.contains("nonfinite-trans:target", &error, "y");
            }
        };
    });
    p.case("nonfinite-event", |p| {
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
        match run(
            &mut *p,
            "nonfinite-event",
            source,
            &vars(&[("v", 10.0), ("thr", 5.0)]),
            &vars(&[("x", 0.0), ("y", 0.0)]),
            1.0,
            0.1,
            StepMethod::Euler,
        ) {
            Ok(_) => {
                p.fail("nonfinite-event:refuse", "a non-finite event payload must refuse");
            }
            Err(error) => {
                p.contains("nonfinite-event:code", &error, "E-EVENT-009");
                p.contains("nonfinite-event:target", &error, "y");
                p.contains("nonfinite-event:event", &error, "E");
            }
        };
    });
    p.case("singular-switch", |p| {
        let source = "\
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
        let inputs = vars(&[
            ("voltage", 10.0),
            ("resistance", 5.0),
            ("capacitance", 1.0),
            ("threshold_voltage", 0.1),
            ("current", 10.0),
        ]);
        let state = vars(&[("charge", 0.2)]);
        match run(&mut *p, "singular-a", source, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler) {
            Ok(_) => {
                p.fail("singular-switch:refuse", "a switched-singular system must Err");
            }
            Err(error) => {
                let typed = error.contains("singular")
                    || error.contains("E-DAE-INIT")
                    || error.contains("regularize");
                p.eq("singular-switch:typed", typed, true);
            }
        };
        let first = run(&mut *p, "singular-b", source, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler).err();
        let second = run(&mut *p, "singular-c", source, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler).err();
        p.eq("singular-switch:replay", first, second);
    });
    p.case("mr-timestep", |p| {
        let source = "\
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
        let state = vars(&[("charge", 0.0)]);
        let inputs = vars(&[
            ("voltage", 10.0),
            ("resistance", 1.0),
            ("capacitance", 1.0),
            ("threshold_voltage", 5.0),
            ("current", 10.0),
        ]);
        let mut fires: Vec<(f64, f64)> = Vec::new();
        for dt in [0.2, 0.1, 0.05, 0.025] {
            match run(&mut *p, "mr-timestep", source, &inputs, &state, 2.0, dt, StepMethod::BackwardEuler) {
                Err(error) => {
                    p.fail(format!("mr-timestep:run[{dt}]"), format!("must integrate: {error}"));
                }
                Ok((trajectory, _)) => {
                    let firing = match trajectory.events.first() {
                        Some(firing) => firing,
                        None => {
                            p.fail(format!("mr-timestep:fired[{dt}]"), "must fire once");
                            continue;
                        }
                    };
                    let crossed = trajectory
                        .samples
                        .iter()
                        .find(|s| (s.t - firing.t).abs() < 1e-9)
                        .and_then(|s| scalar(&s.state, "charge"));
                    match crossed {
                        Some(q) => {
                            fires.push((firing.t, q));
                        }
                        None => {
                            p.fail(format!("mr-timestep:sample[{dt}]"), "firing sample must exist");
                        }
                    };
                }
            };
        }
        if fires.len() == 4 {
            let ln2 = 0.693_147_180_559_945_3;
            for (i, (t, q)) in fires.iter().enumerate() {
                p.eq(format!("mr-timestep:above-ln2[{i}]"), *t >= ln2 - 1e-9, true);
                p.close(format!("mr-timestep:on-threshold[{i}]"), *q, 5.0, 1e-6);
            }
            p.close("mr-timestep:finest", fires[3].0, ln2, 0.012);
            let d01 = (fires[1].0 - fires[0].0).abs();
            let d12 = (fires[2].0 - fires[1].0).abs();
            let d23 = (fires[3].0 - fires[2].0).abs();
            p.eq("mr-timestep:monotone", d01 > d12 && d12 > d23, true);
            p.eq("mr-timestep:shrink-1", d12 < d01 / 1.3, true);
            p.eq("mr-timestep:shrink-2", d23 < d12 / 1.3, true);
        }
    });
    p.case("mr-noevent", |p| {
        let plain = "\
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
        let never = "\
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
        let state = vars(&[("charge", 0.0)]);
        let inputs = vars(&[
            ("voltage", 10.0),
            ("resistance", 1.0),
            ("capacitance", 1.0),
            ("threshold_voltage", 50.0),
            ("current", 10.0),
        ]);
        let with_events = run(&mut *p, "mr-noevent-a", never, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler);
        let without = run(&mut *p, "mr-noevent-b", plain, &inputs, &state, 1.0, 0.1, StepMethod::BackwardEuler);
        match (with_events, without) {
            (Ok((a, _)), Ok((b, _))) => {
                p.eq("mr-noevent:empty", a.events.len(), 0);
                p.eq("mr-noevent:identity", a, b);
            }
            (first, second) => {
                p.fail("mr-noevent:run", format!("both runs must integrate: {first:?} vs {second:?}"));
            }
        };
    });
    p.case("mr-channels", |p| {
        let payload = "\
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
        let via_transition = "\
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
        let inputs = vars(&[("v", 4.0), ("thr", 2.0)]);
        let state = vars(&[("x", 0.0), ("y", 0.0)]);
        let a = run(&mut *p, "mr-channels-a", payload, &inputs, &state, 3.0, 0.1, StepMethod::Euler);
        let b = run(&mut *p, "mr-channels-b", via_transition, &inputs, &state, 3.0, 0.1, StepMethod::Euler);
        match (a, b) {
            (Ok((ta, _)), Ok((tb, _))) => {
                p.eq("mr-channels:count-a", ta.events.len(), 1);
                p.eq("mr-channels:count-b", tb.events.len(), 1);
                match ta.samples.last().and_then(|s| scalar(&s.state, "y")) {
                    Some(y) => {
                        p.close("mr-channels:value", y, 4.0, 1e-9);
                    }
                    None => {
                        p.fail("mr-channels:value", "final y must be scalar");
                    }
                };
                p.eq("mr-channels:identity", ta, tb);
            }
            (first, second) => {
                p.fail("mr-channels:run", format!("both runs must integrate: {first:?} vs {second:?}"));
            }
        };
    });
    p.case("mr-capture", |p| {
        let via_param = "\
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
        let via_state = "\
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
        let inputs = vars(&[("v", 10.0), ("thr", 5.0)]);
        let state = vars(&[("x", 0.0), ("y", 0.0)]);
        let a = run(&mut *p, "mr-capture-a", via_param, &inputs, &state, 1.0, 0.1, StepMethod::Euler);
        let b = run(&mut *p, "mr-capture-b", via_state, &inputs, &state, 1.0, 0.1, StepMethod::Euler);
        match (a, b) {
            (Ok((ta, _)), Ok((tb, _))) => {
                p.eq("mr-capture:count", ta.events.len(), 1);
                match ta.samples.last().and_then(|s| scalar(&s.state, "y")) {
                    Some(y) => {
                        p.close("mr-capture:value", y, 5.0, 1e-4);
                    }
                    None => {
                        p.fail("mr-capture:value", "final y must be scalar");
                    }
                };
                p.eq("mr-capture:identity", ta, tb);
            }
            (first, second) => {
                p.fail("mr-capture:run", format!("both runs must integrate: {first:?} vs {second:?}"));
            }
        };
    });
    p.case("mr-scaling", |p| {
        let source = "\
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
        let state = vars(&[("charge", 0.0)]);
        let fire = |v: f64, thr: f64| {
            let inputs = vars(&[
                ("voltage", v),
                ("resistance", 1.0),
                ("capacitance", 1.0),
                ("threshold_voltage", thr),
                ("current", v),
            ]);
            let result = Source::from_str("mr-scaling", source).check();
            if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
                return None;
            }
            let decl = &result.package.declarations[0];
            simulate_continuous_dispositioned(
                &result.package,
                decl,
                &inputs,
                &state,
                0.0,
                2.0,
                0.1,
                StepMethod::BackwardEuler,
                &SimulateOptions::default(),
            )
            .ok()
            .and_then(|(trajectory, _)| trajectory.events.first().map(|f| f.t))
        };
        match fire(10.0, 5.0) {
            None => {
                p.fail("mr-scaling:base", "base run must fire");
            }
            Some(base) => {
                for k in [0.5, 2.0, 10.0] {
                    match fire(10.0 * k, 5.0 * k) {
                        None => {
                            p.fail(format!("mr-scaling:fired[{k}]"), "scaled run must fire");
                        }
                        Some(t) => {
                            p.eq(format!("mr-scaling:invariant[{k}]"), (t - base).abs() <= 1e-6, true);
                        }
                    };
                }
            }
        };
    });
    p.case("mr-recount", |p| {
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
        let state = vars(&[("v", 8.5), ("y", 0.0)]);
        let mut counts: Vec<usize> = Vec::new();
        let mut ends: Vec<Option<f64>> = Vec::new();
        for dt in [0.1, 0.05, 0.02] {
            match run(&mut *p, "mr-recount", source, &inputs, &state, 5.0, dt, StepMethod::Euler) {
                Err(error) => {
                    p.fail(format!("mr-recount:run[{dt}]"), format!("must integrate: {error}"));
                }
                Ok((trajectory, _)) => {
                    counts.push(trajectory.events.len());
                    ends.push(trajectory.samples.last().and_then(|s| scalar(&s.state, "y")));
                }
            };
        }
        if counts.len() == 3 {
            p.eq("mr-recount:count-stable-1", counts[0], counts[1]);
            p.eq("mr-recount:count-stable-2", counts[1], counts[2]);
            p.eq("mr-recount:y-stable-1", ends[0], ends[1]);
            p.eq("mr-recount:y-stable-2", ends[1], ends[2]);
            p.eq("mr-recount:three-fires", counts[0], 3);
            match ends[0] {
                Some(y) => {
                    p.close("mr-recount:y-counts", y, counts[0] as f64, 1e-9);
                }
                None => {
                    p.fail("mr-recount:y-counts", "final y must be scalar");
                }
            };
        }
    });
    p.finish();
}
