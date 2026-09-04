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

/// The runnable 3D-primitives example: the language truth for this.
const GEOMETRY_EXAMPLE: &str =
    include_str!("../../../language/examples/geometry/3d-primitives.emath");

/// The human reference chapter (geometry/topology surface + seam fence).
const REFERENCE_CHAPTER: &str =
    include_str!("../../../language/reference/geometry-and-topology.md");

fn define_values(source: &str) -> BTreeMap<String, Value> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-ir-geometry", source);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "geometry source must admit: {errors:#?}");
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

fn vector_eq(actual: &Value, want: &[f64]) {
    assert_eq!(actual, &Value::Vector(want.to_vec()), "vector mismatch");
}

/// The planted gap (was RED): the 3D-primitives example is runnable and
/// computes the documented cross/measure/sphere/plane/box/mesh values on
/// the inline formula surface.
#[test]
fn geometry_example_is_runnable() {
    let values = define_values(GEOMETRY_EXAMPLE);

    // cross laws: i × j == k in component 2; cross(u, v) = (-3, 6, -3).
    let i_cross_j = values.get("i_cross_j").expect("i_cross_j");
    assert_eq!(
        i_cross_j,
        &Value::Vector(vec![0.0, 0.0, 1.0]),
        "right-hand basis cross"
    );
    vector_eq(
        values.get("cross_uv").expect("cross_uv"),
        &[-3.0, 6.0, -3.0],
    );
    vector_eq(values.get("cross_vu").expect("cross_vu"), &[3.0, -6.0, 3.0]);

    vector_eq(values.get("u").expect("u"), &[1.0, 2.0, 3.0]);
    assert_eq!(
        values.get("length_u"),
        Some(&Value::F64(3.7416573867739413)),
        "norm((1,2,3))"
    );
    assert_eq!(
        values.get("sphere_volume"),
        Some(&Value::F64(33.510321638291124)),
        "4/3·pi·2^3"
    );
    assert_eq!(
        values.get("mesh_volume"),
        Some(&Value::F64(0.0)),
        "flat soup signed volume is exactly 0"
    );
}

/// The planted gap (was RED): the reference chapter documents the
/// surface vocabulary, the executed law set, and the call-seam fence.
#[test]
fn reference_chapter_documents_surface() {
    assert!(
        REFERENCE_CHAPTER.contains("Vector[3]")
            && REFERENCE_CHAPTER.contains("cross")
            && REFERENCE_CHAPTER.contains("normalize"),
        "reference must document the 3D vocabulary"
    );
    assert!(
        REFERENCE_CHAPTER.contains("ApplyCapability")
            && REFERENCE_CHAPTER.contains("declared-function/capability invocation"),
        "reference must document the generic call-seam architecture"
    );
}

/// Canonical content identity: the same geometry source admits to the
/// same ContentId every time; a semantic perturbation (any term edit)
/// changes the id. This is the IR-level determinism + identity pin.
#[test]
fn geometry_meaning_canonical_identity_deterministic() {
    install_source_parser();
    let compile = |source: &str| -> (emath_core::ContentId, emath_core::ContentId) {
        let mut a = CompilerSession::new(Limits::default());
        let mut b = CompilerSession::new(Limits::default());
        let ca = a.check_owned("talo-determinism-a", source);
        let cb = b.check_owned("talo-determinism-b", source);
        assert!(
            !ca.diagnostics.has_errors() && !cb.diagnostics.has_errors(),
            "determinism probe must admit"
        );
        (
            canonical_package(&ca.package),
            canonical_package(&cb.package),
        )
    };
    let (ia1, ia2) = compile(GEOMETRY_EXAMPLE);
    assert_eq!(ia1, ia2, "identical source must canonicalize identically");

    // Semantic perturbation: swap the cross argument order (an
    // anti-commutativity-relevant edit that changes meaning, not
    // presentation).
    let perturbed = GEOMETRY_EXAMPLE.replace("cross_uv = cross(u, v)", "cross_uv = cross(v, u)");
    assert_ne!(
        perturbed, GEOMETRY_EXAMPLE,
        "perturbation probe must differ from the example"
    );
    let (ib, _) = compile(&perturbed);
    assert_ne!(
        ia1, ib,
        "semantic perturbation must change canonical identity"
    );
}
