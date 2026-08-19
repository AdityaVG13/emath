mod contract_tests {
    use semantic_genesis_worlds::{
        Term, evaluate, fixture_modular, ModularWorld, SwappedModularWorld,
    };

    /// The swap transform is not a no-op mutation. The demo term
    /// `⊛(⧖(⋈(a, b)), ζ)` evaluates to 6 under the modular world and to
    /// 5 under the swapped world (⋈ becomes `*`, ⊛ becomes `+`, ζ = 3,
    /// a = 4, b = 7). A mutant that delegates the swapped world to the
    /// modular world returns 6 here and is killed.
    #[test]
    fn swapped_world_is_not_a_noop_mutation() {
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses");
        let env = fixture_modular();
        let modular = evaluate(&term, &ModularWorld, &env).expect("modular evaluates");
        let swapped = evaluate(&term, &SwappedModularWorld, &env).expect("swapped evaluates");
        assert_eq!(modular, 6, "⋈ adds, ⧖ squares, ⊛ multiplies (mod 17)");
        assert_eq!(
            swapped, 5,
            "⋈ multiplies, ⊛ adds after the swap — no-op mutants return 6"
        );
        assert_ne!(modular, swapped);
    }

    /// Nested-shape kill: the swap must hold on every operator path, not
    /// just the demo shape. `⋈(⧖(a), ⧖(b))` is 14 modular (16 + 15) vs 2
    /// swapped (16 * 15 mod 17).
    #[test]
    fn swap_mutation_is_killed_on_other_operator_paths() {
        let term = Term::parse_canonical("apply(⋈,apply(⧖,var(a)),apply(⧖,var(b)))")
            .expect("canonical parses");
        let env = fixture_modular();
        let modular = evaluate(&term, &ModularWorld, &env).expect("modular evaluates");
        let swapped = evaluate(&term, &SwappedModularWorld, &env).expect("swapped evaluates");
        assert_eq!(modular, 14);
        assert_eq!(swapped, 2);
        assert_ne!(modular, swapped);
    }

    /// Metamorphic determinism: the dual-run comparison is seed-free and
    /// deterministic (the seed contract records `consumes_rng: false`),
    /// so repeated evaluation must agree exactly.
    #[test]
    fn dual_run_is_deterministic() {
        let term = Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses");
        let env = fixture_modular();
        let first = evaluate(&term, &SwappedModularWorld, &env).expect("evaluates");
        let second = evaluate(&term, &SwappedModularWorld, &env).expect("evaluates");
        assert_eq!(first, second, "dual-run evaluation must be deterministic");
    }
}

mod unused_worldir_tests {
    use emath_term::{Signature, SymbolId, Term};
    use emath_world_codegen_rust::{WorldSpec, generate};

    fn reference_signature() -> Signature {
        let mut signature = Signature::default();
        for (symbol, arity) in [("ζ", 0usize), ("⋈", 2), ("⧖", 1), ("⊛", 2)] {
            signature
                .insert(SymbolId(symbol.to_string()), arity)
                .expect("fresh symbol inserts");
        }
        signature
    }

    fn reference_term() -> Term {
        Term::parse_canonical("apply(⊛,apply(⧖,apply(⋈,var(a),var(b))),const(ζ))")
            .expect("canonical parses")
    }

    fn modular_spec(operators: &[(&str, &str)]) -> WorldSpec {
        WorldSpec {
            label: "modular_numeric".to_string(),
            operators: operators
                .iter()
                .map(|&(symbol, meaning)| (symbol.to_string(), meaning.to_string()))
                .collect(),
        }
    }

    /// A non-default operator map (SURF-0008: analyzed `WorldIr` codegen
    /// does not consult) must be refused: emitting the crate anyway
    /// would silently disagree with the genesis analysis.
    #[test]
    fn unused_worldir_with_a_non_default_operator_map_is_refused() {
        let refusal = generate(
            &reference_term(),
            &reference_signature(),
            &[modular_spec(&[
                ("ζ", "3"),
                ("⋈", "(x-y) mod 17"),
                ("⧖", "(x*x) mod 17"),
                ("⊛", "(x*y) mod 17"),
            ])],
        )
        .expect_err("non-default ⋈ semantics must be refused");
        assert_eq!(refusal.code, "E-GEN-094");
        assert!(refusal.message.contains("⋈"), "{}", refusal.message);
        assert!(refusal.message.contains("modular_numeric"));
    }

    /// An operator outside the hardcoded per-label set is a silent-drop
    /// candidate and must be refused as well.
    #[test]
    fn unused_worldir_with_an_extra_operator_is_refused() {
        let refusal = generate(
            &reference_term(),
            &reference_signature(),
            &[modular_spec(&[
                ("ζ", "3"),
                ("⋈", "(x+y) mod 17"),
                ("⧖", "(x*x) mod 17"),
                ("⊛", "(x*y) mod 17"),
                ("✳", "(x*y) mod 19"),
            ])],
        )
        .expect_err("extra undeclared operator must be refused");
        assert_eq!(refusal.code, "E-GEN-094");
        assert!(refusal.message.contains("✳"), "{}", refusal.message);
    }

    /// The empty/default operator map keeps today's label-only success
    /// path byte-identical: the refusal only fires on divergence.
    #[test]
    fn unused_worldir_default_operator_maps_still_generate() {
        let package = generate(
            &reference_term(),
            &reference_signature(),
            &[
                WorldSpec {
                    label: "free_symbolic".to_string(),
                    operators: vec![],
                },
                WorldSpec {
                    label: "boolean_algebra".to_string(),
                    operators: vec![
                        ("ζ".to_string(), "true".to_string()),
                        ("⋈".to_string(), "xor".to_string()),
                        ("⧖".to_string(), "not".to_string()),
                        ("⊛".to_string(), "and".to_string()),
                    ],
                },
                modular_spec(&[
                    ("ζ", "3"),
                    ("⋈", "(x+y) mod 17"),
                    ("⧖", "(x*x) mod 17"),
                    ("⊛", "(x*y) mod 17"),
                ]),
            ],
        )
        .expect("default operator maps must keep generating");
        assert!(package.files["src/lib.rs"].contains("reference_term"));
        assert!(package.files["Cargo.toml"].contains("semantic-genesis-worlds"));
    }
}

mod specialized_abi_tests {
    use semantic_genesis_worlds::{
        evaluate, evaluate_specialized, fixture_modular, reference_term, EvalError, ModularWorld,
        Term,
    };

    /// Differential pin: the declaration-specific ABI must agree with the
    /// generic ABI on the reference term (both dispatch into the same
    /// world semantics; a divergence would mean the generated dispatcher
    /// mis-mapped a symbol or an arity).
    #[test]
    fn specialized_abi_agrees_with_generic_evaluation() {
        let term = reference_term();
        let env = fixture_modular();
        let generic = evaluate(&term, &ModularWorld, &env).expect("generic evaluates");
        let specialized =
            evaluate_specialized(&term, &ModularWorld, &env).expect("specialized evaluates");
        assert_eq!(generic, specialized);
    }

    /// The specialized dispatcher refuses symbols outside the declared
    /// signature instead of guessing.
    #[test]
    fn specialized_dispatch_refuses_unknown_operators() {
        let term = Term::Apply {
            operator: "✳".into(),
            arguments: vec![],
        };
        let env = fixture_modular();
        let error = evaluate_specialized(&term, &ModularWorld, &env).expect_err("unknown refused");
        assert!(matches!(error, EvalError::UnknownSymbol(_)));
    }

    /// A wrong runtime arity through the generic term shape is a typed
    /// refusal in the specialized dispatcher (compile-time safety only
    /// covers direct method calls).
    #[test]
    fn specialized_dispatch_refuses_wrong_arity() {
        let term = Term::Apply {
            operator: "⧖".into(),
            arguments: vec![Term::Variable("a".into()), Term::Variable("b".into())],
        };
        let env = fixture_modular();
        let error = evaluate_specialized(&term, &ModularWorld, &env).expect_err("arity refused");
        assert!(matches!(error, EvalError::Arity { .. }));
    }
}
