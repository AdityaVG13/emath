//! Pedagogic diagnostics with rendered witnesses.

use emath_cli::diagnostics::{
    E_LAW_001, EXPLANATION_SCHEMA, ExplainKind, TutorCheckError, check_and_explain, e_law_001_demo,
    e_law_001_demo_table, every_failure_has_witness, explanation_json, render_cayley_ascii,
    tutor_check_v1,
};
use emath_law_check::{Law, WorldObligation};
use emath_term::SymbolId;
use emath_world_ir::WorldId;

#[test]
fn commutative_refutation_emits_cayley_witness() {
    let (report, explanations) = e_law_001_demo();
    assert!(!report.passed, "demo table must falsify commutativity");
    let explanation = explanations.first().expect("witness");
    assert_eq!(explanation.code, E_LAW_001);
    assert_eq!(explanation.kind, ExplainKind::LawFalsified);
    tutor_check_v1(explanation).expect("faithful explanation");
    let witness = explanation.witness.as_ref().expect("rendered witness");
    assert_eq!(witness.counterexample_tuple, ["0", "1"]);
    let ascii = render_cayley_ascii(witness);
    assert!(ascii.contains("0 0"), "{ascii}");
    assert!(ascii.contains("counterexample: 0,1"), "{ascii}");
    let json = explanation_json(explanation);
    assert!(json.contains(EXPLANATION_SCHEMA), "{json}");
    assert!(json.contains(E_LAW_001), "{json}");
}

#[test]
fn tutor_check_rejects_claimed_green_without_witness() {
    let claimed =
        include_str!("../../../tests/invalid/pedagogic_diagnostics_false_green");
    let explanation: emath_cli::diagnostics::Explanation = claimed_green_from_fixture(claimed);
    let error = tutor_check_v1(&explanation).expect_err("claimed green must fail");
    assert_eq!(error, TutorCheckError::ClaimedGreenWithoutWitness);
}

#[test]
fn every_finite_checker_refutation_emits_witness() {
    let table = e_law_001_demo_table();
    let obligations = [
        WorldObligation {
            id: 1,
            law: Law::Commutative(SymbolId("op".to_string())),
        },
        WorldObligation {
            id: 2,
            law: Law::Associative(SymbolId("op".to_string())),
        },
    ];
    let (report, explanations) =
        check_and_explain(WorldId(1), &table, &obligations).expect("table is total");
    assert!(!report.passed);
    assert!(
        every_failure_has_witness(&report, &explanations),
        "failed verdicts={}, explanations={}",
        report.verdicts.iter().filter(|v| !v.passed).count(),
        explanations.len()
    );
    for explanation in &explanations {
        let witness = explanation.witness.as_ref().expect("witness");
        assert!(!witness.counterexample_tuple.is_empty());
    }
}

fn claimed_green_from_fixture(body: &str) -> emath_cli::diagnostics::Explanation {
    assert!(body.contains("epic claimed green"));
    emath_cli::diagnostics::Explanation {
        code: "E-LAW-001".into(),
        kind: emath_cli::diagnostics::ExplainKind::LawFalsified,
        witness: None,
        structured_narrative: "epic claimed green without a checker receipt".into(),
        documentation_links: Vec::new(),
        receipt_id: None,
    }
}
