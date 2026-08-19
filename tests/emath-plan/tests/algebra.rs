//! Resolution-algebra witnesses: serial associativity, identity
//! neutrality, left-biased alternatives, commutative parallel join,
//! lifted refusal collection and explicit fallback degradation.

use emath_plan::{Facet, Lifted, QState, Step, fallback, parallel, serial};

fn capability(provider: &str, facets: &[Facet]) -> Step {
    Step::Capability {
        provider: provider.to_string(),
        discharges: facets.iter().copied().collect(),
        refusals: Vec::new(),
    }
}

#[test]
fn serial_composition_is_associative() {
    let a = capability("a", &[Facet::Kind]);
    let b = capability("b", &[Facet::Target]);
    let c = capability("c", &[Facet::Determinism]);
    let left = serial(serial(a.clone(), b.clone()), c.clone());
    let right = serial(a, serial(b, c));
    let start = QState::full();
    assert_eq!(left.apply(&start), right.apply(&start));
}

#[test]
fn identity_is_neutral_for_serial_composition() {
    let step = capability("a", &[Facet::Kind, Facet::Target]);
    let start = QState::full();
    assert_eq!(
        serial(Step::Id, step.clone()).apply(&start),
        step.apply(&start)
    );
    assert_eq!(
        serial(step.clone(), Step::Id).apply(&start),
        step.apply(&start)
    );
}

#[test]
fn alternatives_are_left_biased_and_skip_inapplicable_arms() {
    let refused = Step::refused("dead", vec!["E-PROV-512: no capability".into()]);
    let live = Step::compatible("live");
    let other = Step::compatible("other");
    let application = Step::Alt(vec![refused, live, other])
        .apply(&QState::full())
        .expect("a live arm must apply");
    assert_eq!(application.trace, vec!["live".to_string()]);
    assert!(application.state.is_resolved());
}

#[test]
fn parallel_composition_state_is_commutative() {
    let a = capability("a", &[Facet::Kind, Facet::Evidence]);
    let b = capability("b", &[Facet::Target]);
    let start = QState::full();
    let left = parallel(a.clone(), b.clone())
        .apply(&start)
        .expect("applies");
    let right = parallel(b, a).apply(&start).expect("applies");
    assert_eq!(left.state, right.state);
    assert_eq!(
        left.state.open_facets(),
        vec![Facet::Exactness, Facet::Determinism]
    );
}

#[test]
fn lifting_turns_inapplicability_into_explicit_refusal() {
    let step = Step::Alt(vec![
        Step::refused("p1", vec!["E-PROV-515: no exactness".into()]),
        Step::refused("p2", vec!["E-PROV-514: wrong target".into()]),
    ]);
    match step.apply_total(&QState::full()) {
        Lifted::Refused { reasons } => {
            assert_eq!(
                reasons,
                vec![
                    "p1: E-PROV-515: no exactness".to_string(),
                    "p2: E-PROV-514: wrong target".to_string(),
                ]
            );
        }
        Lifted::Applied(application) => {
            panic!("refused arms must not apply: {application:?}")
        }
    }
}

#[test]
fn fallback_arm_is_marked_degraded() {
    let step = fallback(
        Step::refused("primary", vec!["E-PROV-513: evidence ceiling".into()]),
        Step::compatible("secondary"),
    );
    let application = step.apply(&QState::full()).expect("fallback applies");
    assert!(
        application.degraded,
        "fallback must be explicit degradation"
    );
    assert_eq!(application.trace, vec!["secondary".to_string()]);
}
