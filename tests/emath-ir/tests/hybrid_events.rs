//! (B42, thin slice): `events:` section
//! admission for hybrid/dynamical declarations.
//!
//! The law, sliced to the admission seam (file-disjoint from the
//! parser and interpreter lanes):
//! - ch7's `events:` section parses today (event declarations are
//!   FnDecl heads / commands) but was refused as an unknown section —
//!   this slice admits it with CLOSED shapes: `event Name(field: Type)`
//!   declarations or no-arg `event Name` commands; the same event name
//!   twice refuses through the duplicate lane (`E-NAME-022`); anything
//!   else in the section refuses typed (`E-SYN-101`).
//! - `transitions:` (`on <trigger>:` rule suites) does NOT parse yet —
//!   parser lane (another agent's active zone). The fence test pins the
//!   no-half-admit rule: the section stays refused until the parser
//!   slice lands, and the WRITTEN seed (`tests/invalid/
//! r3_dynamical_03lh.emath`, expecting `E-EVENT-001` for a transition
//!   wired to an undeclared event) is that slice's failure-first input.
//! - Event-triggering simulation (zero-crossing detection, reset
//!   application during ODE stepping) is the interpreter lane — the
//!   docs fence states admitting an events section never claims
//!   event-driven simulation computes.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn session() -> CompilerSession {
    install_source_parser();
    CompilerSession::new(Limits::default())
}

fn error_codes(result: &emath_sema::CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

#[test]
fn dynamical_events_section_admits() {
    // `events:` with typed event declarations admits on a model.
    let source = "emath model thermostat:\n    state:\n        heat: Float64\n    definitions:\n        limit = 1.0\n    events:\n        event ThresholdCrossed(value: Float64)\n        event Switched\n".to_string();
    let result = session().check_owned("thermostat", &source);
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "events section admits, got {codes:?} (messages: {:?})",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_dynamical_event_refuses() {
    // Events are named surface: the same event name twice in one
    // `events:` section refuses typed (E-NAME-022 lane) — never
    // silently shadowed.
    let source = "emath model thermostat:\n    state:\n        heat: Float64\n    definitions:\n        limit = 1.0\n    events:\n        event ThresholdCrossed(value: Float64)\n        event ThresholdCrossed\n".to_string();
    let result = session().check_owned("dup-events", &source);
    let codes = error_codes(&result);
    assert!(
        codes.contains(&"E-NAME-022".to_string()),
        "duplicate event name must refuse E-NAME-022, got {codes:?}"
    );
}

#[test]
fn non_event_statement_in_events_section_refuses() {
    // The events section is CLOSED: a statement that is not an event
    // declaration (here a bare assignment) refuses typed — the section
    // cannot smuggle effectful surface in under an events label.
    let source = "emath model thermostat:\n    state:\n        heat: Float64\n    definitions:\n        limit = 1.0\n    events:\n        heat = 0.0\n".to_string();
    let result = session().check_owned("non-event", &source);
    let codes = error_codes(&result);
    assert!(
        codes.contains(&"E-SYN-101".to_string()),
        "a non-event statement in `events:` must refuse E-SYN-101, got {codes:?}"
    );
}

#[test]
fn unsupported_dynamical_transition_refuses() {
    // Fence (no-half-admit law): `transitions:` stays refused while the
    // `on <trigger>:` rule suite does not parse (parser lane). If this
    // test starts failing because transitions began admitting WITHOUT
    // parseable rule bodies, that is the silent half-admit this fence
    // exists to block — land the parser slice, then flip this test to
    // the E-EVENT-001 trigger check (the seed is written and waiting).
    let source = "emath model thermostat:\n    state:\n        heat: Float64\n    definitions:\n        limit = 1.0\n    transitions:\n        on ThresholdCrossed(value):\n            heat = 1.0\n".to_string();
    let result = session().check_owned("fence", &source);
    let codes = error_codes(&result);
    assert!(
        codes.contains(&"E-SEC-101".to_string()),
        "transitions stays fenced until the parser slice; got {codes:?}"
    );
}

#[test]
fn bouncing_ball_events_admit() {
    // E2E (admission bar): the bouncing-ball hybrid model
    // compiles — continuous state + event surface. (The bounce
    // TRANSITION rule and event-driven simulation are the named next
    // slices; this pins the declared-events half.)
    let source = "emath model bouncing_ball:\n    state:\n        height: Float64\n        velocity: Float64\n    definitions:\n        gravity = 9.81\n    events:\n        event Bounce\n".to_string();
    let result = session().check_owned("bouncing-ball", &source);
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "the bouncing-ball hybrid model compiles, got {codes:?} (messages: {:?})",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
}
