//! World-IR synthesis: declared and builtin worlds.

use super::*;

/// Canonical declared-expression semantics for the built-in worlds.
pub(super) fn declared_world(
    label: &str,
    signature: &Signature,
    semantics: &[(&str, &str)],
) -> WorldIr {
    let symbols = signature
        .iter()
        .map(|(symbol, arity)| SymbolDef {
            id: symbol.clone(),
            display: symbol.0.clone(),
            fixity: if *arity == 0 {
                Fixity::Constant
            } else {
                Fixity::Function
            },
            precedence: None,
            type_scheme: format!("Term^{arity} -> Term"),
        })
        .collect::<Vec<_>>();
    let operators = semantics
        .iter()
        .map(|(symbol, meaning)| OperatorDef {
            symbol: emath_term::SymbolId((*symbol).into()),
            semantics: OperatorSemantics::DeclaredExpression((*meaning).into()),
            origin: MeaningOrigin::Declared,
        })
        .collect::<Vec<_>>();
    WorldIr {
        version: 1,
        name: label.into(),
        signature: signature.clone(),
        carriers: vec![emath_world_ir::CarrierDef {
            name: "Element".into(),
            type_expression: match label {
                "Boolean_algebra" => "Bool".into(),
                "modular_numeric" => "Z_17".into(),
                "one_point" => "One".into(),
                "csa_seeded" => "U64".into(),
                _ => "FreeTerm".into(),
            },
        }],
        symbols,
        operators,
        constructors: vec!["Element -> Constant/Apply".into()],
        laws: vec!["total".into(), "deterministic".into()],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["pure".into()],
    }
}

/// Admitted built-in world labels (G4 gate: at least five world classes
/// with deterministic identities in the portfolio).
pub(super) const ADMITTED_WORLDS: [&str; 5] = [
    "free_symbolic",
    "Boolean_algebra",
    "modular_numeric",
    "one_point",
    "csa_seeded",
];

/// Worlds with a Rust codegen lowering (`compile --parametric`). The
/// one-point and seeded-CSA totality witnesses are portfolio candidates
/// only: the generator has no lowering for them and must refuse rather
/// than emit an unhonored map.
pub(super) const COMPILED_WORLDS: [&str; 3] =
    ["free_symbolic", "Boolean_algebra", "modular_numeric"];

/// Builds the five admitted `WorldIr` candidates for `signature`.
pub fn builtin_worlds(signature: &Signature) -> Vec<WorldIr> {
    let mut worlds = vec![free_symbolic_world("free_symbolic", signature.clone())];
    worlds.push(declared_world(
        "Boolean_algebra",
        signature,
        &[("ζ", "true"), ("⋈", "xor"), ("⧖", "not"), ("⊛", "and")],
    ));
    worlds.push(declared_world(
        "modular_numeric",
        signature,
        &[
            ("ζ", "3"),
            ("⋈", "(x+y) mod 17"),
            ("⧖", "(x*x) mod 17"),
            ("⊛", "(x*y) mod 17"),
        ],
    ));
    // The degenerate one-point algebra: every symbol means the single
    // carrier point (ADR-003 totality witness, never intended meaning).
    worlds.push(declared_world(
        "one_point",
        signature,
        &[("ζ", "•"), ("⋈", "•"), ("⧖", "•"), ("⊛", "•")],
    ));
    // The canonical seeded algebra: total, deterministic, seed-keyed
    // FNV-1a mixing over u64 (emath.csa v1, baseline seed).
    worlds.push(declared_world(
        "csa_seeded",
        signature,
        &[
            ("ζ", "fnv1a(seed, const:ζ)"),
            ("⋈", "fnv1a(seed, apply:⋈, args)"),
            ("⧖", "fnv1a(seed, apply:⧖, args)"),
            ("⊛", "fnv1a(seed, apply:⊛, args)"),
        ],
    ));
    debug_assert!(
        worlds
            .iter()
            .map(|world| world.name.as_str())
            .eq(ADMITTED_WORLDS),
        "builtin worlds must match the admitted-world roster"
    );
    worlds
}
