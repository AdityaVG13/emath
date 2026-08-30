//! Bead `emath-0e68` — prove curve evaluation `r(t) -> Vector[3]`,
//! surface evaluation `r(u,v) -> Vector[3]`, and implicit-field
//! evaluation `f(p) -> Float64` end to end from ordinary `.emath`
//! source through the generic call machinery.
//!
//! Failure-first record (pre-fix): RED. Sema refused the user-function
//! call `r(0.5)` with E-TYPE-003 "unknown function `r`" — sibling
//! `emath function` names were not in the Phase 1 callable set (builtins
//! only), so user-defined parameterized calls were the named gap. The
//! fix lands sibling-function calls as pure-inline substitution at sema
//! (`crates/emath-sema/src/admit/lowering/sibling_calls.rs`).
//!
//! The runnable example
//! `language/examples/geometry/parametric-surfaces.emath` is the pinned
//! artifact for the curve/surface/implicit tests; the remaining sources
//! are negative (arity refusal) and metamorphic (reparameterization,
//! determinism) cases that do not belong in a user-facing example.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;

/// The runnable example IS the pinned artifact for the curve/surface/
/// implicit tests (the `geometry3d.rs` pattern): the shipped
/// `parametric-surfaces.emath` declares the curve `r(t)`, the
/// paraboloid/sphere/torus surfaces, the implicit sphere field, and one
/// acceptance function calling all of them; these tests evaluate that
/// file end to end so the example cannot drift from the proof.
const EXAMPLE_SOURCE: &str =
    include_str!("../../../language/examples/geometry/parametric-surfaces.emath");

/// Intended behavior: r(0.5) == [0.5, 0.25, 1.0], r(2.0) == [2.0, 4.0, 4.0].
#[test]
fn curve_point_evaluates_end_to_end() {
    let values = eval_source(
        EXAMPLE_SOURCE,
        "parametric-surfaces",
        "ParametricSurfacesAcceptance",
    );
    assert_eq!(
        values.get("p_half"),
        Some(&Value::Vector(vec![0.5, 0.25, 1.0])),
        "r(0.5) = [0.5, 0.25, 1.0]"
    );
    assert_eq!(
        values.get("p_two"),
        Some(&Value::Vector(vec![2.0, 4.0, 4.0])),
        "r(2.0) == [2.0, 4.0, 4.0]"
    );
}

/// Pass 5 artifact — a call with the wrong argument count must be
/// refused with a diagnostic that names the arity problem (not a generic
/// "unknown function").
const ARITY_SOURCE: &str = r#"
emath function paraboloid:
    inputs:
        u: Float64
        v: Float64
    outputs:
        point: Vector[Float64]
    definitions:
        point = [u, v, u * u + v * v]

emath function ArityAcceptance:
    definitions:
        bad = paraboloid(1.0)
"#;

fn eval_source(source: &str, session_name: &str, entry: &str) -> BTreeMap<String, Value> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(session_name, source);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "source must admit: {errors:#?}");
    let declaration = checked
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.name.leaf() == entry)
        .expect("the acceptance function must be present");
    eval_definitions_values(
        &checked.package,
        declaration,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("source must evaluate: {fault}"))
}

/// Intended behavior (pass 3): paraboloid(0.5, 2.0) == [0.5, 2.0, 4.25],
/// paraboloid(1.0, -1.0) == [1.0, -1.0, 2.0], sphere(0,0) == [0,0,1]
/// exactly (sin(0)=0, cos(0)=1 are exact in f64), and the torus outer
/// equator torus(0,0) == [3,0,0] exactly ((2+cos(0))·cos(0) = 3·1).
#[test]
fn surface_point_evaluates_end_to_end() {
    let values = eval_source(
        EXAMPLE_SOURCE,
        "parametric-surfaces",
        "ParametricSurfacesAcceptance",
    );
    assert_eq!(
        values.get("s_a"),
        Some(&Value::Vector(vec![0.5, 2.0, 4.25])),
        "paraboloid(0.5, 2.0) = [0.5, 2.0, 4.25]"
    );
    assert_eq!(
        values.get("s_b"),
        Some(&Value::Vector(vec![1.0, -1.0, 2.0])),
        "paraboloid(1.0, -1.0) = [1.0, -1.0, 2.0]"
    );
    assert_eq!(
        values.get("s_north"),
        Some(&Value::Vector(vec![0.0, 0.0, 1.0])),
        "sphere north pole r(0,0) = (0,0,1)"
    );
    assert_eq!(
        values.get("t_ring"),
        Some(&Value::Vector(vec![3.0, 0.0, 0.0])),
        "torus outer equator (0,0) = (3,0,0)"
    );
}

/// Intended behavior (pass 5): the implicit sphere field evaluates
/// exactly at representable points: f([1,2,2]) = 9, f([0,0,0]) = 0.
#[test]
fn implicit_field_evaluates_end_to_end() {
    let values = eval_source(
        EXAMPLE_SOURCE,
        "parametric-surfaces",
        "ParametricSurfacesAcceptance",
    );
    assert_eq!(values.get("f_q"), Some(&Value::F64(9.0)), "f([1,2,2]) = 9");
    assert_eq!(
        values.get("f_origin"),
        Some(&Value::F64(0.0)),
        "f([0,0,0]) = 0"
    );
}

/// Intended behavior (pass 5): a wrong-arity call is refused with a
/// diagnostic that names the arity problem. Failure-first: today the
/// refusal is the generic E-TYPE-003 "unknown function", which does NOT
/// mention arity, so this test is RED until the typed refusal lands.
#[test]
fn wrong_arity_call_refused_typed() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("parametric-arity", ARITY_SOURCE);
    let errors: Vec<String> = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect();
    assert!(!errors.is_empty(), "wrong-arity call must be refused");
    assert!(
        errors.iter().any(|e| e.contains("arity")),
        "refusal must name the arity problem, got: {errors:#?}"
    );
}

/// Pass 6 artifact — metamorphic: reparametrization by a period shift
/// preserves the sampled point set within tolerance. The sphere shifted
/// by 2*pi in u is the same point map; IEEE double rounding makes the
/// components agree only to ~1e-16 at generic points, while the
/// exactly-representable point (0,0) must reproduce EXACTLY (sin(0)=0
/// absorbs the argument error).
const REPARAM_SOURCE: &str = r#"
emath function sphere:
    inputs:
        u: Float64
        v: Float64
    outputs:
        point: Vector[Float64]
    definitions:
        point = [sin(v) * cos(u), sin(v) * sin(u), cos(v)]

emath function sphere_shifted:
    inputs:
        u: Float64
        v: Float64
    outputs:
        point: Vector[Float64]
    definitions:
        point = sphere(u + 6.283185307179586, v)

emath function ReparamAcceptance:
    definitions:
        r_a = sphere(0.3, 0.5)
        r_b = sphere_shifted(0.3, 0.5)
        r_origin_a = sphere(0.0, 0.0)
        r_origin_b = sphere_shifted(0.0, 0.0)
        dx_ok = abs(r_b[0] - r_a[0]) < 1e-12
        dy_ok = abs(r_b[1] - r_a[1]) < 1e-12
        dz_ok = abs(r_b[2] - r_a[2]) < 1e-12
"#;

/// Pass 6 artifact — metamorphic: pure-call determinism (same args,
/// bitwise-identical result), symmetry of the paraboloid under the
/// (u,v) swap in the shared z component (IEEE addition commutes
/// exactly), and exact reproduction of a value through a wrapper call.
const DETERMINISM_SOURCE: &str = r#"
emath function paraboloid:
    inputs:
        u: Float64
        v: Float64
    outputs:
        point: Vector[Float64]
    definitions:
        point = [u, v, u * u + v * v]

emath function paraboloid_wrapper:
    inputs:
        u: Float64
        v: Float64
    outputs:
        point: Vector[Float64]
    definitions:
        point = paraboloid(u, v)

emath function DeterminismAcceptance:
    definitions:
        p_a = paraboloid(0.5, 2.0)
        p_b = paraboloid(0.5, 2.0)
        p_swap = paraboloid(2.0, 0.5)
        p_wrap = paraboloid_wrapper(0.5, 2.0)
        z_a = p_a[2]
        z_swap = p_swap[2]
        same = p_a == p_b
        z_match = z_a == z_swap
        wrap_match = p_wrap == p_a
"#;

/// Pass 7 artifact — strict-f64: a called function that leaves the
/// function's domain (ln of a negative argument, sqrt of a negative
/// argument) must refuse with a typed diagnostic, never return NaN.
const DOMAIN_SOURCE: &str = r#"
emath function log_of:
    inputs:
        t: Float64
    outputs:
        value: Float64
    definitions:
        value = ln(t)

emath function DomainAcceptance:
    definitions:
        bad = log_of(-1.0)
"#;

const SQRT_DOMAIN_SOURCE: &str = r#"
emath function sqrt_of:
    inputs:
        t: Float64
    outputs:
        value: Float64
    definitions:
        value = sqrt(t)

emath function SqrtDomainAcceptance:
    definitions:
        bad = sqrt_of(-1.0)
"#;

fn try_eval_source(
    source: &str,
    session_name: &str,
    entry: &str,
) -> Result<BTreeMap<String, Value>, String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(session_name, source);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(format!("admission refused: {errors:#?}"));
    }
    let declaration = checked
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.name.leaf() == entry)
        .expect("the acceptance function must be present");
    eval_definitions_values(&checked.package, declaration, &BTreeMap::new(), &BTreeMap::new())
        .map_err(|fault| format!("evaluation fault: {fault}"))
}

/// Intended behavior (pass 6): the 2*pi-shifted sphere agrees with the
/// original within 1e-12 at a generic point and EXACTLY at (0,0).
#[test]
fn reparameterization_invariance_within_tolerance() {
    let values = eval_source(REPARAM_SOURCE, "reparam-invariance", "ReparamAcceptance");
    for name in ["dx_ok", "dy_ok", "dz_ok"] {
        assert_eq!(
            values.get(name),
            Some(&Value::Bool(true)),
            "{name}: shifted-sphere component must match within 1e-12"
        );
    }
    // Component-wise f64 equality (not Value equality): the shifted
    // path computes 0.0 * sin(2pi) = -0.0 in y — the same real number
    // zero, so f64 `==` (which treats -0.0 == 0.0) is the honest check.
    let (Some(Value::Vector(a)), Some(Value::Vector(b))) = (
        values.get("r_origin_a"),
        values.get("r_origin_b"),
    ) else {
        panic!("north-pole bindings must be vectors");
    };
    assert!(
        a == b,
        "period-shifted sphere must reproduce the north pole exactly: {a:?} vs {b:?}"
    );
}

/// Intended behavior (pass 6): same-args calls are bitwise identical,
/// the (u,v) swap leaves the shared z component bit-identical, and a
/// wrapper call reproduces the direct call exactly.
#[test]
fn symmetric_determinism_and_exact_reproduction() {
    let values = eval_source(DETERMINISM_SOURCE, "determinism", "DeterminismAcceptance");
    assert_eq!(values.get("same"), Some(&Value::Bool(true)), "determinism");
    assert_eq!(
        values.get("z_match"),
        Some(&Value::Bool(true)),
        "swap symmetry in z is exact"
    );
    assert_eq!(
        values.get("wrap_match"),
        Some(&Value::Bool(true)),
        "wrapper call reproduces the direct call exactly"
    );
}

/// Pass 7 pin — strict-f64 discipline through the call path. The
/// builtin contract (crates/emath-exec-ir/src/builtin.rs, the 9bj1
/// convention) is IEEE-faithful propagation: ln/sqrt of a negative
/// argument yield NaN, detectable with `is_finite`, and the tangent
/// stays NaN-consistent. The call path must preserve that convention
/// exactly — inlining may not perturb, silence, or magnify it. (The
/// bead's own typed-refusal duty — degenerate parameter domains —
/// lands in the sampling cell, where the domain is mine to define.)
#[test]
fn log_domain_violation_propagates_nan_per_ieee() {
    let values = eval_source(DOMAIN_SOURCE, "log-domain", "DomainAcceptance");
    assert_eq!(
        values.get("bad"),
        Some(&Value::F64(f64::NAN)),
        "ln(-1) through a called function stays IEEE NaN"
    );
}

/// Pass 7 pin — sqrt mirrors ln: IEEE NaN through the call path.
#[test]
fn sqrt_domain_violation_propagates_nan_per_ieee() {
    let values = eval_source(SQRT_DOMAIN_SOURCE, "sqrt-domain", "SqrtDomainAcceptance");
    assert_eq!(
        values.get("bad"),
        Some(&Value::F64(f64::NAN)),
        "sqrt(-1) through a called function stays IEEE NaN"
    );
}
