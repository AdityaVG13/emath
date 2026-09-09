//! User-defined and law-synthesized worlds.
//!
//! The law: users author worlds (from language kinds, at the
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
    WorldDecl, WorldDeclError, WorldEvidence, WorldLaw, WorldName, WorldOrigin, WorldSourceClass,
    evaluate_labeled, reference_alien_term, select_world, synthesize_world, user_defined_world,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

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
    operations.insert(
        "⧖".to_string(),
        emath_genesis::OperationTable::new(1, square),
    );
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
fn intent() {
    let mut p = Probe::new("User-defined and law-synthesized worlds.");
    p.case("mod17_portfolio_is_labeled", |p| {

    // The mod-17 seed world returns a LABELED portfolio: the reference
    // alien term evaluates through the World ABI and the answer lands in
    // a bundle whose evidence names the world, origin, and laws.
    let (signature, term) = reference_alien_term();
    p.demand("mod17_portfolio_is_labeled#1", ModularAlienWorld.admits(&signature), "mod17_portfolio_is_labeled#1: ModularAlienWorld.admits(&signature)");
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
    p.demand("mod17_portfolio_is_labeled#2", matches!(result.disposition, Disposition::Answer { .. }), "mod17_portfolio_is_labeled#2: matches!(result.disposition, Disposition::Answer { .. })");
    p.demand("mod17_portfolio_is_labeled#3", result.world == "modular-17", format!("expected {:?}, got {:?}", "modular-17", result.world));
    p.demand("mod17_portfolio_is_labeled#4", result.origin == "seed", format!("expected {:?}, got {:?}", "seed", result.origin));
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("mod17_portfolio_is_labeled#5", bundle.bundle_id.starts_with("fnv1a64:"), "mod17_portfolio_is_labeled#5: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    // The portfolio disposition trail records EVERY candidate verdict.
    // Doctrine order: free-symbolic (always applicable) and the boolean
    // alien apply first; demanding the modular carrier excludes them and
    // selects modular-17 — every verdict stays on the trail.
    let disposition = select_world(
        &signature,
        &[WorldName::FreeSymbolic, WorldName::BooleanAlien],
    );
    p.eq("mod17_portfolio_is_labeled#6", disposition.selected, Some(WorldName::ModularAlien));
    p.eq("mod17_portfolio_is_labeled#7", disposition.trail.len(), 3);
    p.demand("mod17_portfolio_is_labeled#8", disposition.trail[0].contains("excluded"), "mod17_portfolio_is_labeled#8: disposition.trail[0].contains(\"excluded\")");
    p.demand("mod17_portfolio_is_labeled#9", disposition.trail[2].contains("applicable"), "mod17_portfolio_is_labeled#9: disposition.trail[2].contains(\"applicable\")");

    });
    p.case("user_defined_world_is_labeled_and_checked", |p| {

    // A user-declared world (language kind at the execution layer)
    // constructs only when its declaration is internally consistent:
    // total operation tables over the declared carrier, constants in the
    // carrier. It evaluates the reference term and every answer is
    // labeled origin=user-defined.
    let world = user_defined_world(modular_five_decl()).expect("consistent declaration");
    let (signature, term) = reference_alien_term();
    p.demand("user_defined_world_is_labeled_and_checked#1", world.admits(&signature), "user_defined_world_is_labeled_and_checked#1: world.admits(&signature)");

    // ⊛(⧖(⋈(2,3)), ζ) = (2+3)² · 3 = 25·3 = 75 ≡ 0 (mod 5).
    let value = emath_genesis::evaluate(&term, &world, &alien_environment())
        .expect("user-defined world evaluates");
    p.demand("user_defined_world_is_labeled_and_checked#2", value == "0", format!("expected {:?}, got {:?}", "0", value));

    let result = evaluate_labeled(
        &term,
        &world,
        &alien_environment(),
        WorldBudget { max_steps: 16 },
        |element: &String| element.clone(),
    );
    p.demand("user_defined_world_is_labeled_and_checked#3", matches!(result.disposition, Disposition::Answer { .. }), "user_defined_world_is_labeled_and_checked#3: matches!(result.disposition, Disposition::Answer { .. })");
    p.demand("user_defined_world_is_labeled_and_checked#4", result.world == "modular-five", format!("expected {:?}, got {:?}", "modular-five", result.world));
    p.demand("user_defined_world_is_labeled_and_checked#5", result.origin == "user-defined", format!("expected {:?}, got {:?}", "user-defined", result.origin));
    let bundle = ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("user_defined_world_is_labeled_and_checked#6", bundle.bundle_id.starts_with("fnv1a64:"), "user_defined_world_is_labeled_and_checked#6: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    // Malformed declarations refuse typed: a constant outside the
    // carrier, an incomplete table.
    let mut bad_constant = modular_five_decl();
    bad_constant
        .constants
        .insert("δ".to_string(), "9".to_string());
    match user_defined_world(bad_constant) {
        Err(WorldDeclError::UnknownElement { element, .. }) => { p.demand("user_defined_world_is_labeled_and_checked#7", element == "9", format!("expected {:?}, got {:?}", "9", element)); },
        other => { p.fail("user_defined_world_is_labeled_and_checked#8", format!("expected UnknownElement, got {other:?}")); return; },
    }
    let mut incomplete = modular_five_decl();
    incomplete
        .operations
        .get_mut("⊛")
        .expect("table present")
        .rows
        .remove(&vec!["0".to_string(), "0".to_string()]);
    p.demand("user_defined_world_is_labeled_and_checked#9", matches!(
        user_defined_world(incomplete),
        Err(WorldDeclError::IncompleteTable { .. })
    ), "user_defined_world_is_labeled_and_checked#9: matches!(\n        user_defined_world(incomplete),\n        Err(WorldDeclError::IncompleteTable { .. }");

    });
    p.case("law_synthesis_is_bounded_and_labeled", |p| {

    // Law-synthesized worlds: the canonical model of the law over the
    // declared carrier (toy size ≤ 6), labeled origin=synthesized, and
    // the law VERIFIED over the whole carrier (independently checked).
    let domain: Vec<String> = (0..6).map(|v| v.to_string()).collect();

    let commutative = synthesize_world("synth-comm", &WorldLaw::Commutative, domain.clone())
        .expect("commutative synthesis");
    p.demand("law_synthesis_is_bounded_and_labeled#1", commutative.evidence().origin == "synthesized", format!("expected {:?}, got {:?}", "synthesized", commutative.evidence().origin));
    let table = commutative.table("⋈").expect("synthesized operation");
    for left in &domain {
        for right in &domain {
            let forward = table
                .row(&[left.clone(), right.clone()])
                .expect("total table");
            let backward = table
                .row(&[right.clone(), left.clone()])
                .expect("total table");
            p.eq(format!("commutativity holds: {left}⋈{right}"), forward, backward);
        }
    }

    // Idempotent: t(x,x) = x over the whole carrier.
    let idempotent = synthesize_world("synth-idem", &WorldLaw::Idempotent, domain.clone())
        .expect("idempotent synthesis");
    let table = idempotent.table("⋈").expect("synthesized operation");
    for value in &domain {
        p.eq(format!("idempotence holds for {value}"), table.row(&[value.clone(), value.clone()]).expect("total"), value);
    }

    // Identity element: row/col of the declared identity are the identity.
    let with_identity = synthesize_world(
        "synth-identity",
        &WorldLaw::IdentityElement {
            element: "1".to_string(),
        },
        domain.clone(),
    )
    .expect("identity synthesis");
    let table = with_identity.table("⋈").expect("synthesized operation");
    for value in &domain {
        p.eq("law_synthesis_is_bounded_and_labeled#4", table.row(&["1".to_string(), value.clone()]).expect("total"), value);
        p.eq("law_synthesis_is_bounded_and_labeled#5", table.row(&[value.clone(), "1".to_string()]).expect("total"), value);
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
    signature
        .insert(SymbolId("⋈".into()), 2)
        .expect("conflict-free");
    p.demand("law_synthesis_is_bounded_and_labeled#6", commutative.admits(&signature), "law_synthesis_is_bounded_and_labeled#6: commutative.admits(&signature)");
    let result = evaluate_labeled(
        &term,
        &commutative,
        &alien_environment(),
        WorldBudget { max_steps: 16 },
        |element: &String| element.clone(),
    );
    p.demand("law_synthesis_is_bounded_and_labeled#7", matches!(result.disposition, Disposition::Answer { .. }), "law_synthesis_is_bounded_and_labeled#7: matches!(result.disposition, Disposition::Answer { .. })");
    p.demand("law_synthesis_is_bounded_and_labeled#8", result.world == "synth-comm", format!("expected {:?}, got {:?}", "synth-comm", result.world));
    p.demand("law_synthesis_is_bounded_and_labeled#9", result.origin == "synthesized", format!("expected {:?}, got {:?}", "synthesized", result.origin));

    // Size bound: a carrier over the toy bound refuses typed.
    let oversized: Vec<String> = (0..7).map(|v| v.to_string()).collect();
    match synthesize_world("synth-big", &WorldLaw::Commutative, oversized) {
        Err(emath_genesis::SynthesisError::SizeBoundExceeded { size, max }) => {
            p.eq("law_synthesis_is_bounded_and_labeled#10", size, 7);
            p.eq("law_synthesis_is_bounded_and_labeled#11", max, 6);
        }
        other => { p.fail("law_synthesis_is_bounded_and_labeled#12", format!("expected SizeBoundExceeded, got {other:?}")); return; },
    }

    });
    p.case("false_models_are_rejected_typed", |p| {

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
        Err(WorldDeclError::FalseModel { actual, .. }) => { p.demand("false_models_are_rejected_typed#1", actual == "4", format!("expected {:?}, got {:?}", "4", actual)); },
        other => { p.fail("false_models_are_rejected_typed#2", format!("expected FalseModel, got {other:?}")); return; },
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
    p.demand("false_models_are_rejected_typed#3", matches!(
        world.check_model(&unknown),
        Err(WorldDeclError::UnknownElement { .. })
    ), "false_models_are_rejected_typed#3: matches!(\n        world.check_model(&unknown),\n        Err(WorldDeclError::UnknownElement { .. })\n  ");

    });
    p.case("strict_source_refuses_world_attachment", |p| {

    // The firewall clause: a STRICT source never carries a custom world.
    // The attachment seam refuses typed (E-WORLD-006); a custom-lane
    // source attaches the same declaration fine.
    let decl = modular_five_decl();
    match emath_genesis::attach_world(WorldSourceClass::Strict, "gaussian-model", decl.clone()) {
        Err(WorldDeclError::StrictFirewall { source }) => { p.demand("strict_source_refuses_world_attachment#1", source == "gaussian-model", format!("expected {:?}, got {:?}", "gaussian-model", source)); },
        other => { p.fail("strict_source_refuses_world_attachment#2", format!("expected StrictFirewall, got {other:?}")); return; },
    }
    let attached = emath_genesis::attach_world(WorldSourceClass::Custom, "alien-model", decl)
        .expect("custom source attaches its world");
    p.demand("strict_source_refuses_world_attachment#3", attached.name() == "modular-five", format!("expected {:?}, got {:?}", "modular-five", attached.name()));
    p.demand("strict_source_refuses_world_attachment#4", attached.evidence().origin == "user-defined", format!("expected {:?}, got {:?}", "user-defined", attached.evidence().origin));

    // Negative seed: the seeded silent-success scenario declares the
    // typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/user_defined_worlds.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects the strict-firewall refusal, found: {expect_line}"), expect_line.contains("E-WORLD-006"), format!("seed expects the strict-firewall refusal, found: {expect_line}"));

    });
    p.case("evidence_is_owned_for_runtime_worlds", |p| {

    // Runtime-authored worlds cannot borrow 'static names: the evidence
    // record is OWNED, and static seed worlds keep their one-line shape
    // through the seed constructor.
    let seed = WorldEvidence::seed("modular-17", &["ring-mod-17-table"]);
    p.demand("evidence_is_owned_for_runtime_worlds#1", seed.world == "modular-17", format!("expected {:?}, got {:?}", "modular-17", seed.world));
    p.demand("evidence_is_owned_for_runtime_worlds#2", seed.origin == "seed", format!("expected {:?}, got {:?}", "seed", seed.origin));
    let world = user_defined_world(modular_five_decl()).expect("consistent declaration");
    let evidence: WorldEvidence = world.evidence();
    p.demand("evidence_is_owned_for_runtime_worlds#3", evidence.world == "modular-five", format!("expected {:?}, got {:?}", "modular-five", evidence.world));
    p.demand("evidence_is_owned_for_runtime_worlds#4", evidence.origin == "user-defined", format!("expected {:?}, got {:?}", "user-defined", evidence.origin));
    p.eq("evidence_is_owned_for_runtime_worlds#5", evidence.laws, vec!["ring-mod-5-table".to_string()]);
    let _ = EvalError::UnknownSymbol(SymbolId("unused".into()));

    });
    p.finish();
}











