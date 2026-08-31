//! emath-epic-machine-fjxh.13: User-defined and law-synthesized worlds.
//!
//! The bead's law: users author worlds (from language kinds, at the
//! execution layer — the parser lands later) or worlds are synthesized
//! from laws (toy size ≤ 6); every world is LABELED (evidence: name,
//! origin class, claimed laws), independently checked, and false models
//! are rejected typed. Synthesized worlds are never claimed as Real
//! meaning: origin `synthesized` rides the evidence into every bundle.
//! The strict source lane refuses world attachments typed — the strict
//! vs Genesis/custom firewall holds (a strict Gaussian never runs Mod17).

use std::collections::BTreeMap;

use emath_genesis::{
    Disposition, EvalError, FirstOrderWorld, ModularAlienWorld, ResultBundle, WorldBudget,
    WorldDeclError, WorldEvidence, WorldOrigin, WorldName, evaluate_labeled, reference_alien_term,
    select_world, synthesize_world, user_defined_world, WorldDecl, WorldLaw, WorldSourceClass,
};
use emath_term::{SymbolId, Term, VariableId};

/// The reference alien declaration over the toy carrier {0..4}: the
/// mod-17 seed semantics scaled to mod 5 (⋈ add, ⧖ square, ⊛ mul, ζ=3).
fn modular_five_decl() -> WorldDecl {
    let mut constants = BTreeMap::new();
    constants.insert("ζ".to_string(), "3".to_string());
    let mut operations = BTreeMap::new();
    let mut join = BTreeMap::new();
    let mut meet = BTreeMap::new();
    for left in 0..5_i64 {
        for right in 0..5_i64 {
            join.insert(
                vec![left.to_string(), right.to_string()],
                ((left + right).rem_euclid(5)).to_string(),
            );
            meet.insert(
                vec![left.to_string(), right.to_string()],
                ((left * right).rem_euclid(5)).to_string(),
            );
        }
    }
    let mut square = BTreeMap::new();
    for value in 0..5_i64 {
        square.insert(vec![value.to_string()], ((value * value) % 5).to_string());
    }
    operations.insert("⋈".to_string(), emath_genesis::OperationTable::new(2, join));
    operations.insert("⊛".to_string(), emath_genesis::OperationTable::new(2, meet));
    operations.insert("⧖".to_string(), emath_genesis::OperationTable::new(1, square));
    WorldDecl {
        name: "modular-five".to_string(),
        origin: WorldOrigin::UserDefined,
        laws: vec!["ring-mod-5-table".to_string()],
        domain: (0..5).map(|v| v.to_string()).collect(),
        constants,
        operations,
    }
}

fn alien_environment() -> BTreeMap<VariableId, String> {
    let mut environment = BTreeMap::new();
    environment.insert(VariableId("a".into()), "2".to_string());
    environment.insert(VariableId("b".into()), "3".to_string());
    environment
}

#[test]
fn mod17_portfolio_is_labeled() {
    // The mod-17 seed world returns a LABELED portfolio: the reference
    // alien term evaluates through the World ABI and the answer lands in
    // a bundle whose evidence names the world, origin, and laws.
    let (signature, term) = reference_alien_term();
    assert!(ModularAlienWorld.admits(&signature));
    let environment = BTreeMap::from([
        (VariableId("a".into()), 2_i64),
        (VariableId("b".into()), 3_i64),
    ]);
    let result = evaluate_labeled(
        &term,
        &ModularAlienWorld,
        &environment,
        WorldBudget { max_steps: 16 },
        |answer: &i64| answer.to_string(),
    );
    assert!(matches!(result.disposition, Disposition::Answer { .. }));
    assert_eq!(result.world, "modular-17");
    assert_eq!(result.origin, "seed");
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));

    // The portfolio disposition trail records EVERY candidate verdict.
    // Doctrine order: free-symbolic (always applicable) and the boolean
    // alien apply first; demanding the modular carrier excludes them and
    // selects modular-17 — every verdict stays on the trail.
    let disposition = select_world(&signature, &[WorldName::FreeSymbolic, WorldName::BooleanAlien]);
    assert_eq!(disposition.selected, Some(WorldName::ModularAlien));
    assert_eq!(disposition.trail.len(), 3);
    assert!(disposition.trail[0].contains("excluded"));
    assert!(disposition.trail[2].contains("applicable"));
}

#[test]
fn user_defined_world_is_labeled_and_checked() {
    // A user-declared world (language kind at the execution layer)
    // constructs only when its declaration is internally consistent:
    // total operation tables over the declared carrier, constants in the
    // carrier. It evaluates the reference term and every answer is
    // labeled origin=user-defined.
    let world = user_defined_world(modular_five_decl()).expect("consistent declaration");
    let (signature, term) = reference_alien_term();
    assert!(world.admits(&signature));

    // ⊛(⧖(⋈(2,3)), ζ) = (2+3)² · 3 = 25·3 = 75 ≡ 0 (mod 5).
    let value = emath_genesis::evaluate(&term, &world, &alien_environment())
        .expect("user-defined world evaluates");
    assert_eq!(value, "0");

    let result = evaluate_labeled(
        &term,
        &world,
        &alien_environment(),
        WorldBudget { max_steps: 16 },
        |element: &String| element.clone(),
    );
    assert!(matches!(result.disposition, Disposition::Answer { .. }));
    assert_eq!(result.world, "modular-five");
    assert_eq!(result.origin, "user-defined");
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));

    // Malformed declarations refuse typed: a constant outside the
    // carrier, an incomplete table.
    let mut bad_constant = modular_five_decl();
    bad_constant.constants.insert("δ".to_string(), "9".to_string());
    match user_defined_world(bad_constant) {
        Err(WorldDeclError::UnknownElement { element, .. }) => assert_eq!(element, "9"),
        other => panic!("expected UnknownElement, got {other:?}"),
    }
    let mut incomplete = modular_five_decl();
    incomplete
        .operations
        .get_mut("⊛")
        .expect("table present")
        .rows
        .remove(&vec!["0".to_string(), "0".to_string()]);
    assert!(matches!(
        user_defined_world(incomplete),
        Err(WorldDeclError::IncompleteTable { .. })
    ));
}

#[test]
fn law_synthesis_is_bounded_and_labeled() {
    // Law-synthesized worlds: the canonical model of the law over the
    // declared carrier (toy size ≤ 6), labeled origin=synthesized, and
    // the law VERIFIED over the whole carrier (independently checked).
    let domain: Vec<String> = (0..6).map(|v| v.to_string()).collect();

    let commutative = synthesize_world("synth-comm", &WorldLaw::Commutative, domain.clone())
        .expect("commutative synthesis");
    assert_eq!(commutative.evidence().origin, "synthesized");
    let table = commutative.table("⋈").expect("synthesized operation");
    for left in &domain {
        for right in &domain {
            let forward = table
                .row(&[left.clone(), right.clone()])
                .expect("total table");
            let backward = table
                .row(&[right.clone(), left.clone()])
                .expect("total table");
            assert_eq!(forward, backward, "commutativity holds: {left}⋈{right}");
        }
    }

    // Idempotent: t(x,x) = x over the whole carrier.
    let idempotent = synthesize_world("synth-idem", &WorldLaw::Idempotent, domain.clone())
        .expect("idempotent synthesis");
    let table = idempotent.table("⋈").expect("synthesized operation");
    for value in &domain {
        assert_eq!(
            table.row(&[value.clone(), value.clone()]).expect("total"),
            value,
            "idempotence holds for {value}"
        );
    }

    // Identity element: row/col of the declared identity are the identity.
    let with_identity = synthesize_world(
        "synth-identity",
        &WorldLaw::IdentityElement { element: "1".to_string() },
        domain.clone(),
    )
    .expect("identity synthesis");
    let table = with_identity.table("⋈").expect("synthesized operation");
    for value in &domain {
        assert_eq!(table.row(&["1".to_string(), value.clone()]).expect("total"), value);
        assert_eq!(table.row(&[value.clone(), "1".to_string()]).expect("total"), value);
    }

    // Every synthesized world returns a labeled portfolio.
    let term = Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let mut signature = emath_term::Signature::default();
    signature.insert(SymbolId("⋈".into()), 2).expect("conflict-free");
    assert!(commutative.admits(&signature));
    let result = evaluate_labeled(
        &term,
        &commutative,
        &alien_environment(),
        WorldBudget { max_steps: 16 },
        |element: &String| element.clone(),
    );
    assert!(matches!(result.disposition, Disposition::Answer { .. }));
    assert_eq!(result.world, "synth-comm");
    assert_eq!(result.origin, "synthesized");

    // Size bound: a carrier over the toy bound refuses typed.
    let oversized: Vec<String> = (0..7).map(|v| v.to_string()).collect();
    match synthesize_world("synth-big", &WorldLaw::Commutative, oversized) {
        Err(emath_genesis::SynthesisError::SizeBoundExceeded { size, max }) => {
            assert_eq!(size, 7);
            assert_eq!(max, 6);
        }
        other => panic!("expected SizeBoundExceeded, got {other:?}"),
    }
}

#[test]
fn false_models_are_rejected_typed() {
    // A model CLAIM about the world is checked against the world's own
    // table: the claim ⊛(3,3) = 0 is FALSE (9 mod 5 = 4) — typed
    // rejection, never a silent agreement with a wrong model.
    let world = user_defined_world(modular_five_decl()).expect("consistent declaration");

    let false_claim = emath_genesis::ModelClaim {
        symbol: "⊛".to_string(),
        arguments: vec!["3".to_string(), "3".to_string()],
        expected: "0".to_string(),
    };
    match world.check_model(&false_claim) {
        Err(WorldDeclError::FalseModel { actual, .. }) => assert_eq!(actual, "4"),
        other => panic!("expected FalseModel, got {other:?}"),
    }

    // The TRUE claim passes (independent check, not a tautology: the
    // same check rejected the false one).
    let true_claim = emath_genesis::ModelClaim {
        symbol: "⊛".to_string(),
        arguments: vec!["3".to_string(), "3".to_string()],
        expected: "4".to_string(),
    };
    world.check_model(&true_claim).expect("true claim accepted");

    // A claim about an undeclared symbol is typed too.
    let unknown = emath_genesis::ModelClaim {
        symbol: "δ".to_string(),
        arguments: vec!["1".to_string()],
        expected: "1".to_string(),
    };
    assert!(matches!(
        world.check_model(&unknown),
        Err(WorldDeclError::UnknownElement { .. })
    ));
}

#[test]
fn strict_source_refuses_world_attachment() {
    // The firewall clause: a STRICT source never carries a custom world.
    // The attachment seam refuses typed (E-WORLD-006); a custom-lane
    // source attaches the same declaration fine.
    let decl = modular_five_decl();
    match emath_genesis::attach_world(WorldSourceClass::Strict, "gaussian-model", decl.clone()) {
        Err(WorldDeclError::StrictFirewall { source }) => assert_eq!(source, "gaussian-model"),
        other => panic!("expected StrictFirewall, got {other:?}"),
    }
    let attached = emath_genesis::attach_world(WorldSourceClass::Custom, "alien-model", decl)
        .expect("custom source attaches its world");
    assert_eq!(attached.name(), "modular-five");
    assert_eq!(attached.evidence().origin, "user-defined");

    // Negative seed: the seeded silent-success scenario declares the
    // typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/user_defined_worlds.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-WORLD-006"),
        "seed expects the strict-firewall refusal, found: {expect_line}"
    );
}

#[test]
fn evidence_is_owned_for_runtime_worlds() {
    // Runtime-authored worlds cannot borrow 'static names: the evidence
    // record is OWNED, and static seed worlds keep their one-line shape
    // through the seed constructor.
    let seed = WorldEvidence::seed("modular-17", &["ring-mod-17-table"]);
    assert_eq!(seed.world, "modular-17");
    assert_eq!(seed.origin, "seed");
    let world = user_defined_world(modular_five_decl()).expect("consistent declaration");
    let evidence: WorldEvidence = world.evidence();
    assert_eq!(evidence.world, "modular-five");
    assert_eq!(evidence.origin, "user-defined");
    assert_eq!(evidence.laws, vec!["ring-mod-5-table".to_string()]);
    let _ = EvalError::UnknownSymbol(SymbolId("unused".into()));
}
