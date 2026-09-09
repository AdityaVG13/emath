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
use emath_test_harness::Probe;

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
fn intent() {
    let mut p = Probe::new(": Core-growth gate — CDLOC/SCBD/KGS measured;");
    p.case("real_nucleus_passes_the_gate", |p| {

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
    p.demand(format!("the real nucleus grew an operation-name branch: {:?}", report.violations), report.violations.is_empty(), format!("the real nucleus grew an operation-name branch: {:?}", report.violations));
    // Registry DATA zone: cell names appear exactly in term_compile.rs —
    // 8 entries + 2 init-failure diagnostics naming their cell
    // (sum/softmax) = 10 string-literal mentions; the dispatch files
    // carry none.
    p.eq("8 entries + 2 diagnostics", report.data_zone_mentions, 10);
    for (name, _) in [&sources[0], &sources[1], &sources[2]] {
        p.eq(format!("{name} must be branch-free on cohort identity"), report.mentions_per_file[*name], 0);
    }

    });
    p.case("seeded_operation_name_branch_fails", |p| {

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
    p.eq(format!("{:?}", report.violations), report.violations.len(), 1);
    let GateViolation { file, line, token } = &report.violations[0];
    p.demand("seeded_operation_name_branch_fails#2", file == "backend:codegen.rs", format!("expected {:?}, got {:?}", "backend:codegen.rs", file));
    p.eq("the match arm line", *line, 4);
    p.demand("seeded_operation_name_branch_fails#4", token == "std.tensor.softmax", format!("expected {:?}, got {:?}", "std.tensor.softmax", token));

    // A parser-side name branch fails too (the whole nucleus is gated).
    let seeded_parser = r#"fn kind_of(name: &str) -> Kind {
    if name == "std.math.add" { Kind::Special } else { Kind::Generic }
}
"#;
    let report = growth_gate(&[("parser:cells.rs", seeded_parser)], &["std.math.add"]);
    p.eq("seeded_operation_name_branch_fails#5", report.violations.len(), 1);

    // The same token in the DATA zone is NOT a violation (registry
    // entries are the admitted path): the name lives in a STRING here,
    // like every registry entry.
    let registry_entry = r#"map.insert("std.math.add".to_string(), compiled_cell);
"#;
    let report = growth_gate(
        &[("kernel:term_compile.rs", registry_entry)],
        &["std.math.add"],
    );
    p.demand("seeded_operation_name_branch_fails#6", report.violations.is_empty(), "seeded_operation_name_branch_fails#6: report.violations.is_empty()");
    p.eq("seeded_operation_name_branch_fails#7", report.data_zone_mentions, 1);

    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/core_growth_gate.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects the gate refusal, found: {expect_line}"), expect_line.contains("E-GROWTH-001"), format!("seed expects the gate refusal, found: {expect_line}"));

    });
    p.case("comments_and_unrelated_names_do_not_trip", |p| {

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
    p.demand(format!("{:?}", report.violations), report.violations.is_empty(), format!("{:?}", report.violations));
    p.eq("comments_and_unrelated_names_do_not_trip#2", report.mentions_per_file["backend:notes.rs"], 0);

    // A same-shortname DIFFERENT cell (a user pack's "exp") does not
    // trip the std cell gate: the gate matches the full path.
    let user_pack = r#"match op { "acme.exp" => Ok(Expr::Call("acme_exp", args)), _ => unreachable() }
"#;
    let report = growth_gate(&[("backend:acme.rs", user_pack)], &["std.math.exp"]);
    p.demand(format!("{:?}", report.violations), report.violations.is_empty(), format!("{:?}", report.violations));

    });
    p.case("metrics_reported_for_the_cohort", |p| {

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
    p.eq("clean nucleus: no core LOC names a cell", report.cdloc, 0);
    p.eq("clean nucleus: no identity branches", report.scbd, 0);
    let kgs = kernel_generic_surface(lib);
    // Exact ratchet, not an ever-widening range: a generic vocabulary
    // change must update this measurement deliberately, while adding a
    // capability cell as data leaves it unchanged.
    p.eq("kernel generic surface changed; justify the generic vocabulary delta", kgs, 121);

    // The numbers RESPOND: seeding a violation moves CDLOC/SCBD.
    let seeded = r#"match op { "std.math.add" => add_kernel(a, b), _ => unreachable() }
"#;
    let seeded_report = growth_gate(&[("backend:seed.rs", seeded)], &["std.math.add"]);
    p.eq("metrics_reported_for_the_cohort#4", seeded_report.cdloc, 1);
    p.eq("metrics_reported_for_the_cohort#5", seeded_report.scbd, 1);
    p.eq("metrics_reported_for_the_cohort#6", seeded_report.violations.len(), 1);
    let _ = &report; // clean-report metrics recorded for the pack

    });
    p.case("nucleus_classification_and_bundle_fixture", |p| {

    // File classes drive the gate: data zones vs gated nucleus files.
    p.eq("nucleus_classification_and_bundle_fixture#1", nucleus_class("kernel:term_compile.rs"), NucleusClass::DataZone);
    for name in [
        "kernel:interp.rs",
        "kernel:emitter.rs",
        "kernel:optimize.rs",
        "parser:anything.rs",
        "sema:anything.rs",
        "backend:anything.rs",
    ] {
        p.eq(format!("{name}"), nucleus_class(name), NucleusClass::Gated);
    }
    // Unknown prefixes classify GATED (fail closed — a new directory
    // does not silently escape the gate).
    p.eq("nucleus_classification_and_bundle_fixture#3", nucleus_class("weird:new.rs"), NucleusClass::Gated);

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
    p.demand("nucleus_classification_and_bundle_fixture#4", matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ), "nucleus_classification_and_bundle_fixture#4: matches!(\n        result.disposition,\n        emath_genesis::Disposition::Answer { .. }\n    )");
    p.demand("nucleus_classification_and_bundle_fixture#5", result.world == "core-growth-gate", format!("expected {:?}, got {:?}", "core-growth-gate", result.world));
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("nucleus_classification_and_bundle_fixture#6", bundle.bundle_id.starts_with("fnv1a64:"), "nucleus_classification_and_bundle_fixture#6: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    });
    p.finish();
}









