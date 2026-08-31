//! Lagrangian/action thin slice (bead emath-r3-lagrangian-action-nf7s,
//! 04 section 2.5).
//!
//! Contracts:
//! - **Action-integral binder refuses naming the design of record**:
//!   `S = action integral t in t0..t1: L(...)` (and a row starting with
//!   the binder spelling) refuses `E-SYN-101` naming the Functional
//!   typing, the core-goal lowering, the boundary identity rule, and
//!   the C14 admitted-surface fix — previously the row half-parsed as
//!   `E-TYPE-002 unknown variable 'action'` plus a generic row-shape
//!   error that named nothing;
//! - **Variation goal refuses naming its lowering**: `variation <S>
//!   wrt q:` refuses `E-SYN-101` naming the Euler-Lagrange residual
//!   built from admitted derivatives, `yield euler_lagrange`, and the
//!   boundary-hash rule — previously the row died with generic
//!   `unexpected ':'` + `unexpected indent`;
//! - a bare `action` / `variation` stays a plain identifier (the
//!   fences fire only on the design spellings);
//! - ordinary definitions, equations, and goals admit unchanged.
//!
//! Design prose of record: ch.7 "Action integrals and variation goals
//! (04 section 2.5)".

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const ACTION_BINDER: &str = "\
emath model ActionProbe:
    inputs:
        m: Float64 = 1.0

    state:
        q: Float64
        v: Float64

    equations:
        der(q) = v
        der(v) = -2.0 * q / m

    definitions:
        S = action integral t in 0..1: 0.5 * m * v * v - 2.0 * q * q / 2

    goals:
        variation S wrt q:
            yield euler_lagrange
";

const VARIATION_GOAL: &str = "\
emath model VarProbe:
    state:
        q: Float64
        v: Float64

    equations:
        der(q) = v
        der(v) = -2.0 * q

    goals:
        variation q wrt t:
            yield euler_lagrange
";

const PLAIN_MODEL: &str = "\
emath model PlainProbe:
    state:
        q: Float64
        v: Float64

    equations:
        der(q) = v
        der(v) = -2.0 * q
";

#[test]
fn action_binder_refuses_naming_design() {
    let errors = check(ACTION_BINDER, "action-fence");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("action integral") && e.contains("Functional")),
        "`S = action integral ...` must refuse naming the Functional \
         design of record; got: {errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("C14")),
        "the action fence must name the C14 admitted-surface fix; got: \
         {errors:#?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("E-TYPE-002") && e.contains("`action`")),
        "the action spelling must never resurface as `unknown variable \
         action`; got: {errors:#?}"
    );
}

#[test]
fn variation_goal_refuses_naming_lowering() {
    let errors = check(VARIATION_GOAL, "variation-fence");
    assert!(
        errors.iter().any(|e| e.contains("variation")
            && e.contains("Euler-Lagrange")),
        "`variation <S> wrt q:` must refuse naming the core-goal \
         lowering; got: {errors:#?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("boundary: fixed_endpoints")),
        "the variation fence must name the boundary identity rule; got: \
         {errors:#?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("unexpected `:`")),
        "the variation goal must never die with a generic `unexpected \
         ':'` error; got: {errors:#?}"
    );
}

#[test]
fn bare_action_and_variation_stay_plain_idents() {
    // `action = 5` and a mention of `variation` as a name must NOT hit
    // the design fences — the fences fire only on the two-word binder
    // spelling and the goal spelling.
    let errors = check(
        "\
emath model IdentProbe:
    definitions:
        action = 5.0
        variation = action + 1.0
",
        "ident-guard",
    );
    assert!(
        errors.iter().all(|e| !e.contains("action/variation design")),
        "bare `action`/`variation` identifiers must not trip the design \
         fences; got: {errors:#?}"
    );
}

#[test]
fn plain_models_admit_unchanged() {
    let errors = check(PLAIN_MODEL, "plain-guard");
    assert!(
        errors.is_empty(),
        "the lagrangian fences must not affect ordinary models; got: \
         {errors:#?}"
    );
}
