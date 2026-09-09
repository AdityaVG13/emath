//! Pedagogic diagnostics with rendered witnesses.

use emath_cli::diagnostics::{
    E_LAW_001, EXPLANATION_SCHEMA, ExplainKind, TutorCheckError, check_and_explain, e_law_001_demo,
    e_law_001_demo_table, every_failure_has_witness, explanation_json, render_cayley_ascii,
    tutor_check_v1,
};
use emath_lab_core::law_check::{Law, WorldObligation};
use emath_term::SymbolId;
use emath_test_harness::Probe;
use emath_world_ir::WorldId;

#[test]
fn pedagogic_diagnostics_carry_witnesses() {
    let mut p = Probe::new("every falsified law ships a rendered witness and witness-free green claims are rejected");
    p.case("cayley-witness", |p| {
        let (report, explanations) = e_law_001_demo();
        p.demand("falsified", !report.passed, "demo table must falsify commutativity");
        match explanations.first() {
            Some(explanation) => {
                p.eq("code", explanation.code.as_str(), E_LAW_001);
                p.demand(
                    "kind",
                    explanation.kind == ExplainKind::LawFalsified,
                    format!("expected LawFalsified, got {:?}", explanation.kind),
                );
                let faithful = tutor_check_v1(explanation);
                p.demand(
                    "faithful",
                    faithful.is_ok(),
                    format!("faithful explanation must pass tutor check, got {faithful:?}"),
                );
                match explanation.witness.as_ref() {
                    Some(witness) => {
                        p.eq(
                            "tuple",
                            format!("{:?}", witness.counterexample_tuple),
                            format!("{:?}", ["0", "1"]),
                        );
                        let ascii = render_cayley_ascii(witness);
                        p.contains("cayley", &ascii, "0 0");
                        p.contains("counterexample", &ascii, "counterexample: 0,1");
                    }
                    None => {
                        p.fail("witness", "explanation must carry a rendered witness");
                    }
                }
                let json = explanation_json(explanation);
                p.contains("schema", &json, EXPLANATION_SCHEMA);
                p.contains("code", &json, E_LAW_001);
            }
            None => {
                p.fail("witness", "demo must produce at least one explanation");
            }
        }
    });
    p.case("false-green", |p| {
        let body = include_str!("../../../tests/invalid/pedagogic_diagnostics_false_green");
        p.contains("fixture", body, "epic claimed green");
        let explanation = emath_cli::diagnostics::Explanation {
            code: "E-LAW-001".into(),
            kind: ExplainKind::LawFalsified,
            witness: None,
            structured_narrative: "epic claimed green without a checker receipt".into(),
            documentation_links: Vec::new(),
            receipt_id: None,
        };
        match tutor_check_v1(&explanation) {
            Err(error) => {
                p.eq("rejected", error, TutorCheckError::ClaimedGreenWithoutWitness);
            }
            Ok(()) => {
                p.fail("rejected", "claimed green without a witness must fail tutor check");
            }
        }
    });
    p.case("all-refutations", |p| {
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
        match check_and_explain(WorldId(1), &table, &obligations) {
            Ok((report, explanations)) => {
                p.demand("falsified", !report.passed, "table must falsify at least one law");
                p.demand(
                    "witnessed",
                    every_failure_has_witness(&report, &explanations),
                    format!(
                        "every failure needs a witness: failed verdicts={}, explanations={}",
                        report.verdicts.iter().filter(|v| !v.passed).count(),
                        explanations.len()
                    ),
                );
                for (index, explanation) in explanations.iter().enumerate() {
                    match explanation.witness.as_ref() {
                        Some(witness) => {
                            p.demand(
                                format!("witness-{index}"),
                                witness.counterexample_tuple.is_empty() == false,
                                "witness must name its counterexample",
                            );
                        }
                        None => {
                            p.fail(format!("witness-{index}"), "refutation must carry a witness");
                        }
                    }
                }
            }
            Err(error) => {
                p.fail("total", format!("table is total, got {error:?}"));
            }
        }
    });
    p.finish();
}
