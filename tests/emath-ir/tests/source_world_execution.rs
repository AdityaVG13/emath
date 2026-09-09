//! capstone — source-first-worlds.
//!
//! The capstone contract: a STRICT file runs through the strict lane and
//! selects NO invented world (its source carries no custom section and
//! the strict VM path takes no world argument); a CUSTOM file's term is
//! interpreted through the World ABI portfolio with EVERY world labeled
//! (free symbolic, Boolean, modular, plus a brand-new world defined HERE
//! — adding a world touches no parser/sema/backend code); every result is
//! a labeled `WorldResultBundle` entry (no naked answers), and the bundle
//! id is a deterministic replay anchor.

use emath_cli::{run, run_check};
use emath_genesis::{
    BooleanAlienWorld, Disposition, Environment, EvalError, FirstOrderWorld, ModularAlienWorld,
    NakedResultRefusal, ResultBundle, WorldBudget, WorldEvidence, WorldResult, evaluate_labeled,
    reference_alien_term,
};
use emath_syntax::install_source_parser;
use emath_term::{SymbolId, Term, VariableId};
use std::path::PathBuf;
use emath_test_harness::{Probe, boot};

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn label_i64(value: &i64) -> String {
    value.to_string()
}

/// A brand-new world, defined in the TEST: adding it touched zero crates
/// (the "adding a world does not touch parser/sema/backend" proof).
struct ModularFiveWorld;

impl FirstOrderWorld for ModularFiveWorld {
    type Value = i64;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        match symbol.0.as_str() {
            "ζ" => Ok(2),
            _ => Err(EvalError::UnknownSymbol(symbol.clone())),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let modulo = |value: i64| value.rem_euclid(5);
        match (operator.0.as_str(), arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(modulo(left + right)),
            ("⧖", [value]) => Ok(modulo(value * value)),
            ("⊛", [left, right]) => Ok(modulo(left * right)),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.clone())),
        }
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence {
            world: "modular-5".to_string(),
            origin: "user-defined".to_string(),
            laws: vec!["ring-mod-5-table".to_string()],
        }
    }
}

fn labeled_custom_bundle(p: &mut Probe) -> ResultBundle {
    let (signature, term) = reference_alien_term();
    p.demand("signature.validate(&term).is_ok()", signature.validate(&term).is_ok(), "signature.validate(&term).is_ok()");
    let budget = WorldBudget { max_steps: 64 };
    let i64_environment: Environment<i64> = [
        (VariableId("a".into()), 2_i64),
        (VariableId("b".into()), 4_i64),
    ]
    .into_iter()
    .collect();
    let bool_environment: Environment<bool> = [
        (VariableId("a".into()), true),
        (VariableId("b".into()), false),
    ]
    .into_iter()
    .collect();
    let results = vec![
        evaluate_labeled(
            &term,
            &ModularAlienWorld,
            &i64_environment,
            budget,
            label_i64,
        ),
        evaluate_labeled(
            &term,
            &BooleanAlienWorld,
            &bool_environment,
            budget,
            |value: &bool| value.to_string(),
        ),
        evaluate_labeled(
            &term,
            &ModularFiveWorld,
            &i64_environment,
            budget,
            label_i64,
        ),
    ];
    ResultBundle::new(results).expect("all entries labeled")
}

#[test]
fn intent() {
    boot();
    let mut p = Probe::new("capstone — source-first-worlds.");
    p.case("strict_file_runs_and_selects_no_invented_world", |p| {

    install_source_parser();
    let source = repo_file("tests/fixtures/language/intro/hello-square.emath");
    let (diagnostics, _, _) = run_check(&source);
    let errors = diagnostics
        .items()
        .iter()
        .filter(|item| item.severity == emath_core::Severity::Error && item.code.starts_with("E-"))
        .count();
    p.eq("strict example must admit with no E-* errors", errors, 0);

    // Firewall: the strict file carries no custom section, so the strict
    // lane selected no invented world; its provenance is the reference VM.
    let text = std::fs::read_to_string(&source).expect("read strict source");
    p.demand("a strict source must not carry a custom section", !text.contains("emath custom"), "a strict source must not carry a custom section");

    // The strict run path exits ok through the CLI (production path):
    // `emath run <file>` returns Ok for the hello-square example.
    let exit = run(&["run".to_string(), source.display().to_string()]);
    p.eq("strict run exits ok", exit, emath_cli::EXIT_OK);

    });
    p.case("custom_file_interprets_with_labeled_worlds", |p| {

    let bundle = labeled_custom_bundle(p, );
    p.eq("custom_file_interprets_with_labeled_worlds#1", bundle.results.len(), 3);
    let worlds: Vec<&str> = bundle
        .results
        .iter()
        .map(|result| result.world.as_str())
        .collect();
    p.eq("custom_file_interprets_with_labeled_worlds#2", worlds, ["modular-17", "boolean-alien", "modular-5"].to_vec());

    // Every entry is a labeled answer with evidence — no naked scalars.
    for result in &bundle.results {
        p.demand("custom_file_interprets_with_labeled_worlds#3", !result.world.is_empty(), "custom_file_interprets_with_labeled_worlds#3: !result.world.is_empty()");
        p.demand("custom_file_interprets_with_labeled_worlds#4", !result.method.is_empty(), "custom_file_interprets_with_labeled_worlds#4: !result.method.is_empty()");
        p.demand("custom_file_interprets_with_labeled_worlds#5", !result.evidence_laws.is_empty(), "custom_file_interprets_with_labeled_worlds#5: !result.evidence_laws.is_empty()");
        match &result.disposition {
            Disposition::Answer { canonical } => { p.demand("custom_file_interprets_with_labeled_worlds#6", !canonical.is_empty(), "custom_file_interprets_with_labeled_worlds#6: !canonical.is_empty()"); },
            other => { p.fail("custom_file_interprets_with_labeled_worlds#7", format!("expected labeled answer, got {other:?}")); return; },
        }
    }
    // Modular values from the same term differ per world (labeled, not
    // one hidden number): mod17 answer 6, mod5 answer 2, boolean false
    // (xor -> not -> and over a=true, b=false, zeta=true).
    match &bundle.results[0].disposition {
        Disposition::Answer { canonical } => { p.demand("custom_file_interprets_with_labeled_worlds#8", canonical == "6", format!("expected {:?}, got {:?}", "6", canonical)); },
        other => { p.fail("custom_file_interprets_with_labeled_worlds#9", format!("mod17 answer expected, got {other:?}")); return; },
    }
    match &bundle.results[2].disposition {
        Disposition::Answer { canonical } => { p.demand("custom_file_interprets_with_labeled_worlds#10", canonical == "2", format!("expected {:?}, got {:?}", "2", canonical)); },
        other => { p.fail("custom_file_interprets_with_labeled_worlds#11", format!("mod5 answer expected, got {other:?}")); return; },
    }
    match &bundle.results[1].disposition {
        Disposition::Answer { canonical } => { p.demand("custom_file_interprets_with_labeled_worlds#12", canonical == "false", format!("expected {:?}, got {:?}", "false", canonical)); },
        other => { p.fail("custom_file_interprets_with_labeled_worlds#13", format!("boolean answer expected, got {other:?}")); return; },
    }

    });
    p.case("bundle_id_replays_and_json_carries_labels", |p| {

    let first = labeled_custom_bundle(p, );
    let second = labeled_custom_bundle(p, );
    p.eq("deterministic replay id", first.bundle_id.clone(), second.bundle_id);
    let json = first.to_json();
    for key in [
        "\"bundle_id\"",
        "\"world\"",
        "\"method\"",
        "\"disposition\"",
        "\"evidence\"",
        "\"cost_steps\"",
        "\"schema\"",
    ] {
        p.demand(format!("bundle JSON must carry {key}: {json}"), json.contains(key), format!("bundle JSON must carry {key}: {json}"));
    }
    p.demand("the new world is labeled", json.contains("\"modular-5\""), "the new world is labeled");

    });
    p.case("naked_result_refused_and_negative_seed", |p| {

    // A bundle entry stripped of its world label is a typed refusal —
    // the provenance swap cannot pass silently.
    let stripped = WorldResult {
        world: String::new(),
        origin: "seed".into(),
        method: "evaluate-bounded".into(),
        term_canonical: "apply(apply)".into(),
        inputs: Default::default(),
        assumptions: Vec::new(),
        disposition: Disposition::Answer {
            canonical: "42".into(),
        },
        evidence_laws: vec!["ring-mod-17-table".into()],
        cost_steps: 3,
    };
    p.eq("naked_result_refused_and_negative_seed#1", stripped.validate(), Err(NakedResultRefusal::MissingWorld));
    p.demand("naked_result_refused_and_negative_seed#2", ResultBundle::new(vec![stripped]).is_err(), "naked_result_refused_and_negative_seed#2: ResultBundle::new(vec![stripped]).is_err()");

    // Negative seed: the provenance-swap scenario declares a typed
    // refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/source_world_execution.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects a typed provenance refusal, found: {expect_line}"), expect_line.contains("E-WORLD"), format!("seed expects a typed provenance refusal, found: {expect_line}"));

    });
    p.finish();
}







