//! emath-epic-machine-fjxh.8: WorldResultBundle — no naked answers.
//!
//! The bead's law: every execution labels world, method, inputs,
//! assumptions, answer-or-disposition, evidence, and cost — the envelope
//! over the World ABI producer (fjxh.7). A bare scalar never escapes a
//! public path: dispositions (answer/open/refused/fault) are first-class,
//! the bundle id is a deterministic content id (replay from IDs
//! reconstructs the labeled result), and a result without its world label
//! is a typed refusal.

use emath_genesis::{
    BooleanAlienWorld, Disposition, Environment, ModularAlienWorld, NakedResultRefusal,
    ResultBundle, WorldBudget, WorldResult, evaluate_labeled, reference_alien_term,
};
use emath_term::{SymbolId, Term, VariableId};

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
fn bundle_labels_every_execution() {
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
    assert_eq!(result.world, "modular-17");
    assert_eq!(result.origin, "seed");
    assert_eq!(result.method, "evaluate-bounded");
    assert_eq!(result.inputs.len(), 2);
    assert_eq!(result.inputs["a"], "2");
    assert!(result.assumptions.is_empty(), "seed worlds declare no effects");
    assert!(!result.evidence_laws.is_empty());
    assert!(result.cost_steps > 0);
    match &result.disposition {
        Disposition::Answer { canonical } => assert_eq!(canonical, "6"),
        other => panic!("expected answer disposition, got {other:?}"),
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
        assert!(json.contains(key), "bundle JSON must label {key}: {json}");
    }
    // The answer value appears only inside the labeled disposition.
    assert!(json.contains("\"kind\":\"answer\""));
}

#[test]
fn dispositions_are_first_class() {
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
            assert_eq!(missing, &["a".to_string()]);
        }
        other => panic!("expected open disposition, got {other:?}"),
    }
    assert!(ResultBundle::new(vec![open]).is_ok(), "open is bundleable");

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
            assert!(reason.contains("unknown symbol"), "{reason}");
        }
        other => panic!("expected refused disposition, got {other:?}"),
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
        Disposition::Refused { reason } => assert!(reason.contains("budget"), "{reason}"),
        other => panic!("expected budget refusal, got {other:?}"),
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
        Disposition::Fault { detail } => assert!(detail.contains("Arity"), "{detail}"),
        other => panic!("expected fault disposition, got {other:?}"),
    }
}

#[test]
fn replay_from_bundle_ids_reconstructs() {
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
    assert_eq!(first.bundle_id, second.bundle_id);
    assert!(first.bundle_id.starts_with("fnv1a64:"), "content id shape");

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
    assert_ne!(first.bundle_id, other_bundle.bundle_id);
}

#[test]
fn naked_results_are_refused() {
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
        other => panic!("expected MissingWorld, got {other:?}"),
    }
    assert_eq!(NakedResultRefusal::MissingWorld.code(), "E-WORLD-001");

    match ResultBundle::new(vec![naked.clone()]) {
        Err(NakedResultRefusal::MissingWorld) => {}
        other => panic!("bundle must refuse a naked result, got {other:?}"),
    }

    // Missing disposition / method are the same refusal family.
    let unlabeled = WorldResult {
        world: "modular-17".into(),
        method: String::new(),
        ..naked.clone()
    };
    assert_eq!(unlabeled.validate(), Err(NakedResultRefusal::MissingMethod));
    let mut open_result = WorldResult {
        method: "evaluate-bounded".into(),
        ..unlabeled
    };
    open_result.disposition = Disposition::Open { missing: vec![] };
    assert!(open_result.validate().is_ok(), "open disposition is complete");

    // Bead negative seed: the naked-result scenario declares a typed
    // refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/result_bundles.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-WORLD"),
        "seed expects a typed world refusal, found: {expect_line}"
    );
}
