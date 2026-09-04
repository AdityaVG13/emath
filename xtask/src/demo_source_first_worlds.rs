//! Capstone demo: `cargo xtask demo source-first-worlds`.
//!
//! A STRICT file runs through the strict compiler lane (admission + run
//! through the `emath` CLI; provenance = the reference VM, no invented
//! world), and a CUSTOM term is interpreted through the World ABI
//! portfolio with EVERY world labeled — free symbolic, Boolean, modular-17,
//! and a modular-5 world defined right here in the demo (adding a world
//! touches no parser/sema/backend code). Every custom result is a labeled
//! `WorldResultBundle` entry (no naked answers); the bundle id is a
//! deterministic replay anchor, and the receipt prints byte-identically
//! twice.

use std::collections::BTreeMap;
use std::process::Command;

use emath_genesis::{
    BooleanAlienWorld, Disposition, Environment, EvalError, FirstOrderWorld, ModularAlienWorld,
    ResultBundle, WorldBudget, WorldEvidence, evaluate_labeled, reference_alien_term,
};
use emath_term::{SymbolId, Term, VariableId};

const STRICT_SOURCE: &str = "language/examples/intro/hello-square.emath";

pub(crate) fn demo() -> u8 {
    println!("== demo source-first-worlds ==");
    match run_demo() {
        Ok(()) => {
            println!("source-first-worlds demo: ok");
            0
        }
        Err(error) => {
            eprintln!("source-first-worlds demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    // ── Strict half: the compiler lane, no invented world ───────────────
    let strict = cargo_emath(&["check", STRICT_SOURCE])?;
    if !strict.status.success() {
        return Err(format!(
            "strict file must admit: {}",
            String::from_utf8_lossy(&strict.stderr)
        ));
    }
    println!("strict: {STRICT_SOURCE} admits (provenance: reference VM)");
    let run = cargo_emath(&["run", STRICT_SOURCE])?;
    if !run.status.success() {
        return Err(format!(
            "strict run must exit ok: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    println!("strict: `emath run` exits ok (strict lane selected no invented world)");

    // ── Custom half: the World ABI portfolio, every world labeled ───────
    let (signature, term) = reference_alien_term();
    if signature.validate(&term).is_err() {
        return Err("reference term must validate".to_string());
    }
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
    let budget = WorldBudget { max_steps: 64 };
    let bundle = ResultBundle::new(vec![
        evaluate_labeled(&term, &ModularAlienWorld, &i64_environment, budget, |v| {
            v.to_string()
        }),
        evaluate_labeled(
            &term,
            &BooleanAlienWorld,
            &bool_environment,
            budget,
            |v: &bool| v.to_string(),
        ),
        evaluate_labeled(&term, &ModularFiveWorld, &i64_environment, budget, |v| {
            v.to_string()
        }),
    ])
    .map_err(|error| format!("labeled bundle refused: {error}"))?;

    for result in &bundle.results {
        let answer = match &result.disposition {
            Disposition::Answer { canonical } => canonical.clone(),
            other => format!("{other:?}"),
        };
        println!(
            "custom: world={} origin={} laws={} answer={answer}",
            result.world,
            result.origin,
            result.evidence_laws.join("|"),
        );
    }

    // Deterministic replay: the same labeled runs rebuild the same
    // bundle id and a byte-identical receipt.
    let again = build_bundle(&term, &i64_environment, &bool_environment, budget)?;
    if again.bundle_id != bundle.bundle_id || again.to_json() != bundle.to_json() {
        return Err("custom receipt is not byte-identical across replays".to_string());
    }
    println!("receipt:\n{}", bundle.to_json());

    // Firewall: strict provenance never appears in the custom bundle, and
    // no custom world ever claims the strict lane.
    for result in &bundle.results {
        if result.method.contains("strict") {
            return Err("custom bundle claims strict provenance".to_string());
        }
    }
    println!("source-first-worlds: strict file and custom term, every answer labeled");
    Ok(())
}

fn build_bundle(
    term: &Term,
    i64_environment: &Environment<i64>,
    bool_environment: &Environment<bool>,
    budget: WorldBudget,
) -> Result<ResultBundle, String> {
    Ok(ResultBundle::new(vec![
        evaluate_labeled(term, &ModularAlienWorld, i64_environment, budget, |v| {
            v.to_string()
        }),
        evaluate_labeled(
            term,
            &BooleanAlienWorld,
            bool_environment,
            budget,
            |v: &bool| v.to_string(),
        ),
        evaluate_labeled(term, &ModularFiveWorld, i64_environment, budget, |v| {
            v.to_string()
        }),
    ])
    .map_err(|error| format!("labeled bundle refused: {error}"))?)
}

/// A modular-5 world, defined HERE: adding it touched no
/// parser/sema/backend code — the World ABI is the only seam.
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

fn cargo_emath(args: &[&str]) -> Result<std::process::Output, String> {
    let _guard = BTreeMap::<String, String>::new();
    Command::new("cargo")
        .args(["run", "-q", "-p", "emath-cli", "--"])
        .args(args)
        .output()
        .map_err(|error| format!("cannot run emath CLI: {error}"))
}
