//!: Core-growth gate — CDLOC/SCBD/KGS measured;
//! operation-name branches blocked.
//!
//! The law: a rising handwritten-core-per-capability slope is a
//! regression. The gate MEASURES the nucleus and BLOCKES the mutation:
//! a stable pure cell must enter as DATA (cell schema + registry entry);
//! any parser/sema/backend/kernel-dispatch branch that names a cohort
//! operation FAILS the gate typed (`E-GROWTH-001`). Metrics (hypotheses
//! until calibrated): CDLOC = core lines naming a capability outside its
//! data zone; SCBD = shared-core branch deltas on capability identity;
//! KGS = the kernel's generic op surface (must not grow per capability).
//! The REAL nucleus is scanned via include_str — the gate is a live
//! tripwire, not a fixture-only check.

use emath_exec_ir::growth::{
    GateViolation, NucleusClass, growth_gate, kernel_generic_surface, nucleus_class,
};

const COHORT: [&str; 8] = [
    "std.math.add",
    "std.math.mul",
    "std.math.sin",
    "std.math.exp",
    "std.math.sqrt",
    "std.math.lt",
    "std.tensor.sum",
    "std.tensor.softmax",
];

fn short(token: &str) -> &str {
    token.rsplit('.').next().unwrap_or(token)
}

#[test]
fn real_nucleus_passes_the_gate() {
    // The LIVE tripwire: the actual exec-ir nucleus sources, scanned as
    // the gate will see them. The registry file names cells (DATA zone);
    // the kernel dispatch files (interp/emitter/optimize) must be
    // branch-free on cohort identity.
    let interp = include_str!("../../../crates/emath-exec-ir/src/interp.rs");
    let emitter = include_str!("../../../crates/emath-exec-ir/src/emitter.rs");
    let optimize = include_str!("../../../crates/emath-exec-ir/src/optimize.rs");
    let term_compile = include_str!("../../../crates/emath-exec-ir/src/term_compile.rs");
    let sources = [
        ("kernel:interp.rs", interp),
        ("kernel:emitter.rs", emitter),
        ("kernel:optimize.rs", optimize),
        ("kernel:term_compile.rs", term_compile),
    ];
    let report = growth_gate(&sources, &COHORT);
    assert!(
        report.violations.is_empty(),
        "the real nucleus grew an operation-name branch: {:?}",
        report.violations
    );
    // Registry DATA zone: cell names appear exactly in term_compile.rs —
    // 8 entries + 2 init-failure diagnostics naming their cell
    // (sum/softmax) = 10 string-literal mentions; the dispatch files
    // carry none.
    assert_eq!(report.data_zone_mentions, 10, "8 entries + 2 diagnostics");
    for (name, _) in [&sources[0], &sources[1], &sources[2]] {
        assert_eq!(
            report.mentions_per_file[*name], 0,
            "{name} must be branch-free on cohort identity"
        );
    }
}

#[test]
fn seeded_operation_name_branch_fails() {
    // Seeded PR-style fixture: a backend file grows a per-cell dispatch
    // arm. The gate FAILS it typed, naming file, line, and token —
    // the negative seed's silent-success scenario.
    let seeded_backend = r#"
fn lower_apply(op: &str, args: &[Value]) -> Result<Expr, Error> {
    match op {
        "std.tensor.softmax" => Ok(Expr::Call("softmax_kernel", args)),
        _ => Err(Error::Unsupported),
    }
}
"#;
    let sources = [("backend:codegen.rs", seeded_backend)];
    let report = growth_gate(&sources, &["std.tensor.softmax"]);
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    let GateViolation { file, line, token } = &report.violations[0];
    assert_eq!(file, "backend:codegen.rs");
    assert_eq!(*line, 4, "the match arm line");
    assert_eq!(token, "std.tensor.softmax");

    // A parser-side name branch fails too (the whole nucleus is gated).
    let seeded_parser = r#"fn kind_of(name: &str) -> Kind {
    if name == "std.math.add" { Kind::Special } else { Kind::Generic }
}
"#;
    let report = growth_gate(&[("parser:cells.rs", seeded_parser)], &["std.math.add"]);
    assert_eq!(report.violations.len(), 1);

    // The same token in the DATA zone is NOT a violation (registry
    // entries are the admitted path): the name lives in a STRING here,
    // like every registry entry.
    let registry_entry = r#"map.insert("std.math.add".to_string(), compiled_cell);
"#;
    let report = growth_gate(
        &[("kernel:term_compile.rs", registry_entry)],
        &["std.math.add"],
    );
    assert!(report.violations.is_empty());
    assert_eq!(report.data_zone_mentions, 1);

    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/core_growth_gate.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-GROWTH-001"),
        "seed expects the gate refusal, found: {expect_line}"
    );
}

#[test]
fn comments_and_unrelated_names_do_not_trip() {
    // The gate measures BRANCHES, not prose: comments mentioning a cell
    // (design notes) are stripped before scanning; a name that only
    // shares a short prefix does not trip (whole-token match on the
    // cell path, not substring noise).
    let notes = r#"
// TODO: consider whether std.math.exp needs an axis policy (design note).
/// The softmax cell ships as registry data (see term_compile).
fn unrelated() -> u32 { 0 }
"#;
    let report = growth_gate(
        &[("backend:notes.rs", notes)],
        &["std.math.exp", "std.tensor.softmax"],
    );
    assert!(report.violations.is_empty(), "{:?}", report.violations);
    assert_eq!(report.mentions_per_file["backend:notes.rs"], 0);

    // A same-shortname DIFFERENT cell (a user pack's "exp") does not
    // trip the std cell gate: the gate matches the full path.
    let user_pack = r#"match op { "acme.exp" => Ok(Expr::Call("acme_exp", args)), _ => unreachable() }
"#;
    let report = growth_gate(&[("backend:acme.rs", user_pack)], &["std.math.exp"]);
    assert!(report.violations.is_empty(), "{:?}", report.violations);
}

#[test]
fn metrics_reported_for_the_cohort() {
    // CDLOC/SCBD/KGS (hypotheses until calibrated — the asks for
    // NUMBERS, and the numbers must respond to the inputs):
    // CDLOC = core lines naming a capability outside the data zone
    // (0 on a clean nucleus); SCBD = branch deltas on capability
    // identity (0 clean); KGS = the kernel's generic op surface
    // (variant count — grows only with NEW GENERIC vocabulary, never
    // per cell).
    let interp = include_str!("../../../crates/emath-exec-ir/src/interp.rs");
    let emitter = include_str!("../../../crates/emath-exec-ir/src/emitter.rs");
    let optimize = include_str!("../../../crates/emath-exec-ir/src/optimize.rs");
    let term_compile = include_str!("../../../crates/emath-exec-ir/src/term_compile.rs");
    let lib = include_str!("../../../crates/emath-exec-ir/src/lib.rs");
    let sources = [
        ("kernel:interp.rs", interp),
        ("kernel:emitter.rs", emitter),
        ("kernel:optimize.rs", optimize),
        ("kernel:term_compile.rs", term_compile),
    ];
    let report = growth_gate(&sources, &COHORT);
    assert_eq!(report.cdloc, 0, "clean nucleus: no core LOC names a cell");
    assert_eq!(report.scbd, 0, "clean nucleus: no identity branches");
    let kgs = kernel_generic_surface(lib);
    // Exact ratchet, not an ever-widening range: a generic vocabulary
    // change must update this measurement deliberately, while adding a
    // capability cell as data leaves it unchanged.
    assert_eq!(
        kgs, 121,
        "kernel generic surface changed; justify the generic vocabulary delta"
    );

    // The numbers RESPOND: seeding a violation moves CDLOC/SCBD.
    let seeded = r#"match op { "std.math.add" => add_kernel(a, b), _ => unreachable() }
"#;
    let seeded_report = growth_gate(&[("backend:seed.rs", seeded)], &["std.math.add"]);
    assert_eq!(seeded_report.cdloc, 1);
    assert_eq!(seeded_report.scbd, 1);
    assert_eq!(seeded_report.violations.len(), 1);
    let _ = &report; // clean-report metrics recorded for the pack
}

#[test]
fn nucleus_classification_and_bundle_fixture() {
    // File classes drive the gate: data zones vs gated nucleus files.
    assert_eq!(
        nucleus_class("kernel:term_compile.rs"),
        NucleusClass::DataZone
    );
    for name in [
        "kernel:interp.rs",
        "kernel:emitter.rs",
        "kernel:optimize.rs",
        "parser:anything.rs",
        "sema:anything.rs",
        "backend:anything.rs",
    ] {
        assert_eq!(nucleus_class(name), NucleusClass::Gated, "{name}");
    }
    // Unknown prefixes classify GATED (fail closed — a new directory
    // does not silently escape the gate).
    assert_eq!(nucleus_class("weird:new.rs"), NucleusClass::Gated);

    // Labeled portfolio: the healthy gate verdict lands in the
    // envelope (gate-as-world: evidence carries the metric laws).
    struct GateWorld;
    impl emath_genesis::FirstOrderWorld for GateWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let interp = include_str!("../../../crates/emath-exec-ir/src/interp.rs");
            let report = growth_gate(&[("kernel:interp.rs", interp)], &COHORT);
            if report.violations.is_empty() && report.cdloc == 0 && report.scbd == 0 {
                Ok("gate-green".to_string())
            } else {
                Ok("gate-red".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "core-growth-gate",
                &["no-operation-name-branches", "metrics-respond-to-inputs"],
            )
        }
    }

    use emath_term::SymbolId;
    let term = emath_term::Term::Constant(SymbolId("gate[cohort]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &GateWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "core-growth-gate");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
