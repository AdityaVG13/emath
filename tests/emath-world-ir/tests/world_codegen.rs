//! Generated worlds evaluate distinctly and the codegen refuses silent divergence.

use emath_test_harness::Probe;

#[test]
fn world_codegen() {
    let mut p = Probe::new("swapped worlds differ, codegen refuses divergence, ABIs agree");
    p.case("swap", |p| {
        use semantic_genesis_worlds::{ModularWorld, SwappedModularWorld, Term, evaluate, fixture_modular};
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))").unwrap();
        let env = fixture_modular();
        let modular = evaluate(&term, &ModularWorld, &env).unwrap();
        let swapped = evaluate(&term, &SwappedModularWorld, &env).unwrap();
        p.eq("modular", modular, 6);
        p.eq("swapped", swapped, 5);
        p.ne("distinct", modular, swapped);
        let shape = Term::parse_canonical("apply(⋈,apply(⧖,var(a)),apply(⧖,var(b)))").unwrap();
        let (m2, s2) = (evaluate(&shape, &ModularWorld, &env).unwrap(), evaluate(&shape, &SwappedModularWorld, &env).unwrap());
        p.eq("modular2", m2, 14);
        p.eq("swapped2", s2, 2);
        p.ne("distinct2", m2, s2);
        p.eq("deterministic", evaluate(&term, &SwappedModularWorld, &env).unwrap(), swapped);
    });
    p.case("codegen", |p| {
        use emath_term::{Signature, SymbolId, Term};
        use emath_world_ir::world_codegen_rust::{WorldSpec, generate};
        let mut signature = Signature::default();
        for (symbol, arity) in [("ζ", 0usize), ("⋈", 2), ("⧖", 1), ("⊛", 2)] {
            signature.insert(SymbolId(symbol.to_string()), arity).unwrap();
        }
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))").unwrap();
        let spec = |ops: &[(&str, &str)]| WorldSpec { label: "modular_numeric".to_string(), operators: ops.iter().map(|&(s, m)| (s.to_string(), m.to_string())).collect() };
        let refusal = generate(&term, &signature, &[spec(&[("ζ", "3"), ("⋈", "(x-y) mod 17"), ("⧖", "(x*x) mod 17"), ("⊛", "(x*y) mod 17")])]).unwrap_err();
        p.eq("nondefault-code", refusal.code.to_string(), "E-GEN-094".to_string());
        p.contains("nondefault-op", &refusal.message, "⋈");
        p.contains("nondefault-label", &refusal.message, "modular_numeric");
        let extra = generate(&term, &signature, &[spec(&[("ζ", "3"), ("⋈", "(x+y) mod 17"), ("⧖", "(x*x) mod 17"), ("⊛", "(x*y) mod 17"), ("✳", "(x*y) mod 19")])]).unwrap_err();
        p.eq("extra-code", extra.code.to_string(), "E-GEN-094".to_string());
        p.contains("extra-op", &extra.message, "✳");
        let package = generate(&term, &signature, &[
            WorldSpec { label: "free_symbolic".to_string(), operators: vec![] },
            WorldSpec { label: "boolean_algebra".to_string(), operators: vec![("ζ".to_string(), "true".to_string()), ("⋈".to_string(), "xor".to_string()), ("⧖".to_string(), "not".to_string()), ("⊛".to_string(), "and".to_string())] },
            spec(&[("ζ", "3"), ("⋈", "(x+y) mod 17"), ("⧖", "(x*x) mod 17"), ("⊛", "(x*y) mod 17")]),
        ]).unwrap();
        p.contains("contract", &package.files["src/lib.rs"], "mod contract_tests");
        p.contains("abi", &package.files["src/lib.rs"], "mod specialized_abi_tests");
        p.contains("reference", &package.files["src/lib.rs"], "reference_term");
        p.contains("manifest", &package.files["Cargo.toml"], "semantic-genesis-worlds");
    });
    p.case("abi", |p| {
        use semantic_genesis_worlds::{EvalError, ModularWorld, Term, evaluate, evaluate_specialized, fixture_modular, reference_term};
        let (term, env) = (reference_term(), fixture_modular());
        p.eq("agree", evaluate(&term, &ModularWorld, &env).unwrap(), evaluate_specialized(&term, &ModularWorld, &env).unwrap());
        p.demand("unknown", matches!(evaluate_specialized(&Term::Apply { operator: "✳".into(), arguments: vec![] }, &ModularWorld, &env).unwrap_err(), EvalError::UnknownSymbol(_)), "unknown refuses");
        p.demand("arity", matches!(evaluate_specialized(&Term::Apply { operator: "⧖".into(), arguments: vec![Term::Variable("a".into()), Term::Variable("b".into())] }, &ModularWorld, &env).unwrap_err(), EvalError::Arity { .. }), "arity refuses");
    });
    p.finish();
}
