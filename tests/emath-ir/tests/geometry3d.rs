//! IR-level acceptance for the 3D geometry pack.
//!
//! IR truth: geometry is DATA over the generic surface, so the IR
//! contract is (1) the runnable example admits and evaluates to the
//! documented values, (2) the reference chapter documents the surface
//! and the call-seam fence, and (3) canonical content identity is
//! deterministic across identical compilations and discriminates any
//! semantic perturbation — no geometry-named core variants exist (the
//! core-growth gate is exercised in `core_growth_gate.rs`).
//!
//! Failure-first: all three were RED before this pass (example was a
//! refusal probe, not a runnable program; the reference chapter did
//! not exist; no IR identity pin existed).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_ir::canonical::canonical_package;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;
use emath_test_harness::{Probe, boot};

/// The runnable 3D-primitives example: the language truth for this.
const GEOMETRY_EXAMPLE: &str =
    include_str!("../../../language/examples/geometry/3d-primitives.emath");

/// The human reference chapter (geometry/topology surface + seam fence).
const REFERENCE_CHAPTER: &str =
    include_str!("../../../language/reference/geometry-and-topology.md");

fn define_values(p: &mut Probe, source: &str) -> BTreeMap<String, Value> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-ir-geometry", source);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    p.demand("\"geometry source must admit: {errors:#?}\"", errors.is_empty(), format!("geometry source must admit: {errors:#?}"));
    // The example now declares its capability cells before the
    // acceptance function; select the function by name, not position.
    let declaration = checked
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.name.leaf() == "Cartesian3DAcceptance")
        .expect("the acceptance function must be present");
    eval_definitions_values(
        &checked.package,
        declaration,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("geometry source must evaluate: {fault}"))
}

fn vector_eq(p: &mut Probe, actual: &Value, want: &[f64]) {
    p.eq("actual", &(actual), &(&Value::Vector(want.to_vec())));
}

#[test]
fn intent() {
    boot();
    let mut p = Probe::new("IR-level acceptance for the 3D geometry pack.");
    p.case("geometry_example_is_runnable", |p| {
// The planted gap (was RED): the 3D-primitives example is runnable and
// computes the documented cross/measure/sphere/plane/box/mesh values on
// the inline formula surface.

    let values = define_values(p, GEOMETRY_EXAMPLE);

    // cross laws: i × j == k in component 2; cross(u, v) = (-3, 6, -3).
    let i_cross_j = values.get("i_cross_j").expect("i_cross_j");
    p.eq("right-hand basis cross", i_cross_j, &Value::Vector(vec![0.0, 0.0, 1.0]));
    vector_eq(p, 
        values.get("cross_uv").expect("cross_uv"),
        &[-3.0, 6.0, -3.0],
    );
    vector_eq(p, values.get("cross_vu").expect("cross_vu"), &[3.0, -6.0, 3.0]);

    vector_eq(p, values.get("u").expect("u"), &[1.0, 2.0, 3.0]);
    p.eq("norm((1,2,3))", values.get("length_u"), Some(&Value::F64(3.7416573867739413)));
    p.eq("4/3·pi·2^3", values.get("sphere_volume"), Some(&Value::F64(33.510321638291124)));
    p.eq("flat soup signed volume is exactly 0", values.get("mesh_volume"), Some(&Value::F64(0.0)));

    });
    p.case("reference_chapter_documents_surface", |p| {
// The planted gap (was RED): the reference chapter documents the
// surface vocabulary, the executed law set, and the call-seam fence.

    p.demand("reference must document the 3D vocabulary", REFERENCE_CHAPTER.contains("Vector[3]")
            && REFERENCE_CHAPTER.contains("cross")
            && REFERENCE_CHAPTER.contains("normalize"), "reference must document the 3D vocabulary");
    p.demand("reference must document the generic call-seam architecture", REFERENCE_CHAPTER.contains("ApplyCapability")
            && REFERENCE_CHAPTER.contains("declared-function/capability invocation"), "reference must document the generic call-seam architecture");

    });
    p.case("geometry_meaning_canonical_identity_deterministic", |p| {
// Canonical content identity: the same geometry source admits to the
// same ContentId every time; a semantic perturbation (any term edit)
// changes the id. This is the IR-level determinism + identity pin.

    install_source_parser();
    let compile = |source: &str| -> ((emath_core::ContentId, emath_core::ContentId), bool) {
        let mut a = CompilerSession::new(Limits::default());
        let mut b = CompilerSession::new(Limits::default());
        let ca = a.check_owned("talo-determinism-a", source);
        let cb = b.check_owned("talo-determinism-b", source);
        let ok = !ca.diagnostics.has_errors() && !cb.diagnostics.has_errors();
        (
            (
                canonical_package(&ca.package),
                canonical_package(&cb.package),
            ),
            ok,
        )
    };
    let ((ia1, ia2), ok_example) = compile(GEOMETRY_EXAMPLE);
    p.demand("determinism probe must admit (example)", ok_example, "determinism probe must admit (example)");
    p.eq("identical source must canonicalize identically", ia1.clone(), ia2);

    // Semantic perturbation: swap the cross argument order (an
    // anti-commutativity-relevant edit that changes meaning, not
    // presentation).
    let perturbed = GEOMETRY_EXAMPLE.replace("cross_uv = cross(u, v)", "cross_uv = cross(v, u)");
    p.ne("perturbation probe must differ from the example", perturbed.clone(), GEOMETRY_EXAMPLE.to_string());
    let ((ib, _), ok_perturbed) = compile(&perturbed);
    p.demand("determinism probe must admit (perturbed)", ok_perturbed, "determinism probe must admit (perturbed)");
    p.ne("semantic perturbation must change canonical identity", ia1, ib);

    });
    p.finish();
}





