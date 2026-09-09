//! WorldResultBundle — no naked answers.
//!
//! The law: every execution labels world, method, inputs,
//! assumptions, answer-or-disposition, evidence, and cost — the envelope
//! over the World ABI producer. A bare scalar never escapes a
//! public path: dispositions (answer/open/refused/fault) are first-class,
//! the bundle id is a deterministic content id (replay from IDs
//! reconstructs the labeled result), and a result without its world label
//! is a typed refusal.

use emath_genesis::{
    BooleanAlienWorld, Disposition, Environment, ModularAlienWorld, NakedResultRefusal,
    ResultBundle, WorldBudget, WorldResult, evaluate_labeled, reference_alien_term,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn mod17_environment() -> Environment<i64> {
    [
        (VariableId("a".into()), 2_i64),
        (VariableId("b".into()), 4_i64),
    ]
    .into_iter()
    .collect()
}

fn label_i64(value: &i64) -> String {
    value.to_string()
}

#[test]
fn intent() {
    let mut p = Probe::new("WorldResultBundle — no naked answers.");
    p.case("bundle_labels_every_execution", |p| {

    // Happy path: the World ABI producer wrapped in the envelope. The
    // bundle labels world, method, inputs, assumptions, disposition,
    // evidence, and cost — no naked scalar anywhere.
    let (_signature, term) = reference_alien_term();
    let result = evaluate_labeled(
        &term,
        &ModularAlienWorld,
        &mod17_environment(),
        WorldBudget { max_steps: 64 },
        label_i64,
    );
    p.demand("bundle_labels_every_execution#1", result.world == "modular-17", format!("expected {:?}, got {:?}", "modular-17", result.world));
    p.demand("bundle_labels_every_execution#2", result.origin == "seed", format!("expected {:?}, got {:?}", "seed", result.origin));
    p.demand("bundle_labels_every_execution#3", result.method == "evaluate-bounded", format!("expected {:?}, got {:?}", "evaluate-bounded", result.method));
    p.eq("bundle_labels_every_execution#4", result.inputs.len(), 2);
    p.demand("bundle_labels_every_execution#5", result.inputs["a"] == "2", format!("expected {:?}, got {:?}", "2", result.inputs["a"]));
    p.demand("seed worlds declare no effects", result.assumptions.is_empty(), "seed worlds declare no effects");
    p.demand("bundle_labels_every_execution#7", !result.evidence_laws.is_empty(), "bundle_labels_every_execution#7: !result.evidence_laws.is_empty()");
    p.demand("bundle_labels_every_execution#8", result.cost_steps > 0, "bundle_labels_every_execution#8: result.cost_steps > 0");
    match &result.disposition {
        Disposition::Answer { canonical } => { p.demand("bundle_labels_every_execution#9", canonical == "6", format!("expected {:?}, got {:?}", "6", canonical)); },
        other => { p.fail("bundle_labels_every_execution#10", format!("expected answer disposition, got {other:?}")); return; },
    }

    let bundle = ResultBundle::new(vec![result.clone()]).expect("labeled results bundle");
    let json = bundle.to_json();
    for key in [
        "\"world\"",
        "\"method\"",
        "\"inputs\"",
        "\"assumptions\"",
        "\"disposition\"",
        "\"evidence\"",
        "\"cost_steps\"",
        "\"bundle_id\"",
        "\"schema\"",
    ] {
        p.demand(format!("bundle JSON must label {key}: {json}"), json.contains(key), format!("bundle JSON must label {key}: {json}"));
    }
    // The answer value appears only inside the labeled disposition.
    p.demand("bundle_labels_every_execution#12", json.contains("\"kind\":\"answer\""), "bundle_labels_every_execution#12: json.contains(\"\\\"kind\\\":\\\"answer\\\"\")");

    });
    p.case("dispositions_are_first_class", |p| {

    // OPEN: an open term (missing valuation) is a first-class
    // disposition naming the missing variables — not a missing answer.
    let (_signature, term) = reference_alien_term();
    let open = evaluate_labeled(
        &term,
        &ModularAlienWorld,
        &Environment::new(),
        WorldBudget { max_steps: 64 },
        label_i64,
    );
    match &open.disposition {
        Disposition::Open { missing } => {
            // The metered evaluator fails fast (deterministic
            // left-to-right): the FIRST missing valuation is named.
            p.eq("dispositions_are_first_class#1", missing, &vec!["a".to_string()]);
        }
        other => { p.fail("dispositions_are_first_class#2", format!("expected open disposition, got {other:?}")); return; },
    }
    p.demand("open is bundleable", ResultBundle::new(vec![open]).is_ok(), "open is bundleable");

    // REFUSED: an unknown symbol is a typed refusal with a reason.
    let alien = Term::Constant(SymbolId("q".into()));
    let refused = evaluate_labeled(
        &alien,
        &ModularAlienWorld,
        &Environment::new(),
        WorldBudget { max_steps: 64 },
        label_i64,
    );
    match &refused.disposition {
        Disposition::Refused { reason } => {
            p.demand(format!("{reason}"), reason.contains("unknown symbol"), format!("{reason}"));
        }
        other => { p.fail("dispositions_are_first_class#5", format!("expected refused disposition, got {other:?}")); return; },
    }

    // Budget exhaustion is a refusal carrying the spent steps.
    let starved = evaluate_labeled(
        &term,
        &ModularAlienWorld,
        &mod17_environment(),
        WorldBudget { max_steps: 2 },
        label_i64,
    );
    match &starved.disposition {
        Disposition::Refused { reason } => { p.demand(format!("{reason}"), reason.contains("budget"), format!("{reason}")); },
        other => { p.fail("dispositions_are_first_class#7", format!("expected budget refusal, got {other:?}")); return; },
    }

    // FAULT: a custom world error is first-class (labeled detail), never
    // a silently dropped execution.
    let wrong_shape = Term::Apply {
        operator: SymbolId("⧖".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let fault = evaluate_labeled(
        &wrong_shape,
        &BooleanAlienWorld,
        &[
            (VariableId("a".into()), true),
            (VariableId("b".into()), false),
        ]
        .into_iter()
        .collect(),
        WorldBudget { max_steps: 64 },
        |value: &bool| value.to_string(),
    );
    match &fault.disposition {
        Disposition::Fault { detail } => { p.demand(format!("{detail}"), detail.contains("Arity"), format!("{detail}")); },
        other => { p.fail("dispositions_are_first_class#9", format!("expected fault disposition, got {other:?}")); return; },
    }

    });
    p.case("replay_from_bundle_ids_reconstructs", |p| {

    // Determinism contract: the same producer + inputs + budget rebuild
    // the SAME bundle id — replay reconstructs the labeled result from
    // the id alone.
    let (_signature, term) = reference_alien_term();
    let run = || {
        let result = evaluate_labeled(
            &term,
            &ModularAlienWorld,
            &mod17_environment(),
            WorldBudget { max_steps: 64 },
            label_i64,
        );
        ResultBundle::new(vec![result]).expect("bundle")
    };
    let first = run();
    let second = run();
    p.eq("replay_from_bundle_ids_reconstructs#1", first.bundle_id.clone(), second.bundle_id);
    p.demand("content id shape", first.bundle_id.starts_with("fnv1a64:"), "content id shape");

    // Different content (different budget → different cost label? No:
    // the bundle id is content over the RESULT labels; a different
    // world's evidence changes the id).
    let boolean_result = evaluate_labeled(
        &term,
        &BooleanAlienWorld,
        &[
            (VariableId("a".into()), true),
            (VariableId("b".into()), false),
        ]
        .into_iter()
        .collect(),
        WorldBudget { max_steps: 64 },
        |value: &bool| value.to_string(),
    );
    let other_bundle = ResultBundle::new(vec![boolean_result]).expect("bundle");
    p.ne("replay_from_bundle_ids_reconstructs#3", first.bundle_id, other_bundle.bundle_id);

    });
    p.case("naked_results_are_refused", |p| {

    // A result without its world label is a TYPED refusal: no public path
    // returns (or bundles) a naked answer.
    let naked = WorldResult {
        world: String::new(),
        origin: "seed".into(),
        method: "evaluate-bounded".into(),
        term_canonical: "const(zeta)".into(),
        inputs: Default::default(),
        assumptions: Vec::new(),
        disposition: Disposition::Answer {
            canonical: "42".into(),
        },
        evidence_laws: vec![],
        cost_steps: 1,
    };
    match naked.validate() {
        Err(NakedResultRefusal::MissingWorld) => {}
        other => { p.fail("naked_results_are_refused#1", format!("expected MissingWorld, got {other:?}")); return; },
    }
    p.demand("naked_results_are_refused#2", NakedResultRefusal::MissingWorld.code() == "E-WORLD-001", format!("expected {:?}, got {:?}", "E-WORLD-001", NakedResultRefusal::MissingWorld.code()));

    match ResultBundle::new(vec![naked.clone()]) {
        Err(NakedResultRefusal::MissingWorld) => {}
        other => { p.fail("naked_results_are_refused#3", format!("bundle must refuse a naked result, got {other:?}")); return; },
    }

    // Missing disposition / method are the same refusal family.
    let unlabeled = WorldResult {
        world: "modular-17".into(),
        method: String::new(),
        ..naked.clone()
    };
    p.eq("naked_results_are_refused#4", unlabeled.validate(), Err(NakedResultRefusal::MissingMethod));
    let mut open_result = WorldResult {
        method: "evaluate-bounded".into(),
        ..unlabeled
    };
    open_result.disposition = Disposition::Open { missing: vec![] };
    p.demand("open disposition is complete", open_result.validate().is_ok(), "open disposition is complete");

    // Negative seed: the naked-result scenario declares a typed
    // refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/result_bundles.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects a typed world refusal, found: {expect_line}"), expect_line.contains("E-WORLD"), format!("seed expects a typed world refusal, found: {expect_line}"));

    });
    p.finish();
}







