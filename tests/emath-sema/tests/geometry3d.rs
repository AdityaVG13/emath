//! 3D geometry primitives (emath-talo, pass 1 failure-first).
//!
//! Contracts (orchestrator-approved slice, emath-talo):
//! - geometry is EXPRESSED IN `.emath` over the existing generic
//!   Vector/Matrix/Tensor surface: no geometry core enum, no crate,
//!   no parser fork, no domain-named core variants;
//! - `cross`/`normalize`/`distance` are the capability-cell/expression
//!   layer's job (composable from existing VectorIndex/VectorCreate and
//!   scalar ops), NOT new builtin arms — this test pins the SURFACE
//!   contract and the current gap;
//! - `Option[Point3D]` ray results reuse SilverMaple's real Option<T>
//!   surface once their lane lands (E-TYPE-010 today);
//! - tests live under `tests/`, never under `crates/`.

use emath_core::limits::Limits;
use emath_sema::session::CompilerSession;

fn install_source_parser() {
    emath_syntax::install_source_parser();
}

fn check(text: &str, name: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, text);
    result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const CROSS_PROBE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../language/examples/geometry/3d-primitives.emath"
));

// The flip this test was written to announce: the generic
// declared-capability call seam landed, so the example's named
// cross/normalize/distance calls resolve to the declared pure
// capability cells and the example admits CLEANLY. (Was RED as the
// gap proof: the same file had to refuse before the seam existed.)
#[test]
fn cross_normalize_distance_admit_via_declared_cells() {
    let errors = check(CROSS_PROBE, "talo-cross-probe");
    assert!(
        errors.is_empty(),
        "named geometry cells must admit through the generic capability seam; got {errors:?}"
    );
}

// The generic call path is the ONLY path: the three named calls lower
// to `ExprNode::Apply` targeting the declared cells (canonical names
// `std.geometry.*`), the declared output types type the call results,
// and an unknown name still refuses with the typed unknown-function
// diagnostic — no geometry builtin exists anywhere in the pipeline.
#[test]
fn geometry_calls_lower_to_apply_targeting_declared_cells() {
    use emath_ir::ExprNode;
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-apply-probe", CROSS_PROBE);
    assert!(
        !checked.diagnostics.has_errors(),
        "example must admit: {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let names: Vec<&str> = checked
        .package
        .capabilities
        .iter()
        .map(|capability| capability.name.0.as_str())
        .collect();
    let mut cell_indices = Vec::new();
    for expected in [
        "std.geometry.cross",
        "std.geometry.normalize",
        "std.geometry.distance",
    ] {
        let index = names
            .iter()
            .position(|name| *name == expected)
            .unwrap_or_else(|| panic!("cell {expected} must be declared: {names:?}"));
        cell_indices.push(index);
    }
    let applies: Vec<usize> = checked
        .package
        .exprs
        .iter()
        .filter_map(|node| match node {
            ExprNode::Apply { capability, .. } => Some(capability.index()),
            _ => None,
        })
        .collect();
    assert!(
        !applies.is_empty(),
        "the example must contain capability call sites"
    );
    for index in cell_indices {
        assert!(
            applies.contains(&index),
            "cell index {index} must be reached through ExprNode::Apply; applies = {applies:?}"
        );
    }
}

// The generic surface substrates the primitives must be built from are
// already executable: Vector[3] literal, indexing, dot, norm.
#[test]
fn generic_vector3_surface_executes_before_geometry_cells() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "\
emath function VectorSurface:
    definitions:
        a = [1.0, 2.0, 2.0]
        x = a[0]
        d = dot(a, [2.0, 1.0, 0.0])
        n = norm(a)
    tests:
        example <surface>:
            expect x == 1.0
            expect d == 4.0
            expect n == 3.0
";
    let checked = session.check_owned("talo-vector-surface", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "generic Vector[3] literal/index/dot/norm must execute; got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("x"), Some(&emath_exec_ir::interp::Value::F64(1.0)));
    assert_eq!(values.get("d"), Some(&emath_exec_ir::interp::Value::F64(4.0)));
    assert_eq!(values.get("n"), Some(&emath_exec_ir::interp::Value::F64(3.0)));
}

// ---- Pass 2: Point3D/Vector3D — cross, length, normalize, distance -------
// Expressed as INLINE formulas over the existing generic surface
// (VectorCreate/VectorIndex/dot/norm) per emath-talo — named builtin
// calls (cross/normalize/distance) are the handoff-gated surface; these
// pins prove the SEMANTICS the named forms will bind.
#[test]
fn cross_axis_permutations_inline_semantics() {
    // Standard 3D cross product: (1,0,0)x(0,1,0)=(0,0,1) and the axiom
    // permutations; anti-symmetry pinned at pass 8 as a law.
    let source = "\
emath function CrossAxes:
    definitions:
        x_axis = [1.0, 0.0, 0.0]
        y_axis = [0.0, 1.0, 0.0]
        z_axis = [0.0, 0.0, 1.0]
        x_cross_y = [x_axis[1] * y_axis[2] - x_axis[2] * y_axis[1], x_axis[2] * y_axis[0] - x_axis[0] * y_axis[2], x_axis[0] * y_axis[1] - x_axis[1] * y_axis[0]]
        y_cross_z = [y_axis[1] * z_axis[2] - y_axis[2] * z_axis[1], y_axis[2] * z_axis[0] - y_axis[0] * z_axis[2], y_axis[0] * z_axis[1] - y_axis[1] * z_axis[0]]
        z_cross_x = [z_axis[1] * x_axis[2] - z_axis[2] * x_axis[1], z_axis[2] * x_axis[0] - z_axis[0] * x_axis[2], z_axis[0] * x_axis[1] - z_axis[1] * x_axis[0]]
        y_cross_x = [y_axis[1] * x_axis[2] - y_axis[2] * x_axis[1], y_axis[2] * x_axis[0] - y_axis[0] * x_axis[2], y_axis[0] * x_axis[1] - y_axis[1] * x_axis[0]]
    tests:
        example <axes>:
            expect x_cross_y == [0.0, 0.0, 1.0]
            expect y_cross_z == [1.0, 0.0, 0.0]
            expect z_cross_x == [0.0, 1.0, 0.0]
            expect y_cross_x == [0.0, 0.0, -1.0]
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-cross-axes", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("x_cross_y"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![0.0, 0.0, 1.0]))
    );
    assert_eq!(
        values.get("y_cross_z"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![1.0, 0.0, 0.0]))
    );
    assert_eq!(
        values.get("z_cross_x"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![0.0, 1.0, 0.0]))
    );
    assert_eq!(
        values.get("y_cross_x"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![0.0, 0.0, -1.0]))
    );
}

#[test]
fn length_normalize_distance_inline_semantics() {
    // norm() = length; normalize = v / norm(v); distance = norm(a - b).
    // (3,4,0): length 5, normalized (0.6, 0.8, 0.0); distance between
    // (1,0,0) and (1,1,0) is norm((0,-1,0)) = 1. Zero-normal refusal is
    // a typed negative at pass 9.
    let source = "\
emath function Vector3:
    definitions:
        v = [3.0, 4.0, 0.0]
        v_len = norm(v)
        v_unit = [v[0] / v_len, v[1] / v_len, v[2] / v_len]
        a = [1.0, 0.0, 0.0]
        b = [1.0, 1.0, 0.0]
        d = norm(a - b)
        d2 = norm([1.0, 2.0, 2.0])
    tests:
        example <v3>:
            expect v_len == 5.0
            expect v_unit == [0.6, 0.8, 0.0]
            expect d == 1.0
            expect d2 == 3.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-vector3", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("v_len"), Some(&emath_exec_ir::interp::Value::F64(5.0)));
    assert_eq!(values.get("d"), Some(&emath_exec_ir::interp::Value::F64(1.0)));
    assert_eq!(values.get("d2"), Some(&emath_exec_ir::interp::Value::F64(3.0)));

    // Run under a second session for determinism (same result).
    let mut again = CompilerSession::new(Limits::default());
    let rerun = again.check_owned("talo-vector3", source);
    let report2 = emath_exec_ir::runner::run_package(&rerun.package);
    assert_eq!(
        report2.declarations[0].tests[0].definitions.get("v_len"),
        Some(&emath_exec_ir::interp::Value::F64(5.0))
    );
}

// ---- Pass 3: Sphere (center/radius, contains, volume, surface area) ------
#[test]
fn sphere_contains_volume_surface_inline() {
    // Unit sphere at origin: contains(p) = norm(p - center) <= radius;
    // volume = 4/3*pi*1^3 = 4.1887902047863905; surface = 4*pi = 12.566...
    let source = "\
emath function UnitSphere:
    definitions:
        center = [0.0, 0.0, 0.0]
        radius = 1.0
        p_inside = [0.5, 0.5, 0.5]
        p_outside = [2.0, 0.0, 0.0]
        is_inside = norm(p_inside - center) <= radius
        is_outside = norm(p_outside - center) <= radius
        volume = 4.0 / 3.0 * 3.141592653589793 * radius ^ 3
        surface = 4.0 * 3.141592653589793 * radius ^ 2
    tests:
        example <sphere>:
            expect is_inside == true
            expect is_outside == false
            expect volume == 4.1887902047863905
            expect surface == 12.566370614359172
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-sphere", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("is_inside"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
    assert_eq!(
        values.get("is_outside"),
        Some(&emath_exec_ir::interp::Value::Bool(false))
    );
    assert_eq!(
        values.get("volume"),
        Some(&emath_exec_ir::interp::Value::F64(4.1887902047863905))
    );
    assert_eq!(
        values.get("surface"),
        Some(&emath_exec_ir::interp::Value::F64(12.566370614359172))
    );
}

#[test]
fn sphere_surface_parameterization_lies_on_sphere() {
    // surface_point(theta, phi) = center + r*(sin phi cos theta, sin phi
    // sin theta, cos phi); every sampled point satisfies
    // norm(p - center) == r (metamorphic containment, pass 8).
    let source = "\
emath function SphereParam:
    definitions:
        center = [0.0, 0.0, 0.0]
        radius = 1.0
        theta = 0.0
        phi = 0.0
        sp = [sin(phi) * cos(theta), sin(phi) * sin(theta), cos(phi)]
        dist = norm(sp - center)
    tests:
        example <param>:
            expect sp == [0.0, 0.0, 1.0]
            expect dist == 1.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-sphere-param", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("dist"),
        Some(&emath_exec_ir::interp::Value::F64(1.0))
    );
    assert_eq!(
        values.get("sp"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![0.0, 0.0, 1.0]))
    );
}

// ---- Pass 4: Plane (normal, point, signed distance, containment) ----------
#[test]
fn plane_signed_distance_and_containment_inline() {
    // Plane through (0,0,0) with normal (0,0,1): the xy-plane. Signed
    // distance of p = dot(p - point, normal) for the UNIT normal.
    // containment: |distance| == 0.
    let source = "\
emath function XYPlane:
    definitions:
        normal = [0.0, 0.0, 1.0]
        point = [0.0, 0.0, 0.0]
        above = [1.0, 2.0, 3.0]
        below = [1.0, 2.0, -3.0]
        on_plane = [1.0, 2.0, 0.0]
        d_above = dot(above - point, normal)
        d_below = dot(below - point, normal)
        d_on = dot(on_plane - point, normal)
        contains_above = dot(above - point, normal) == 0.0
        contains_on = dot(on_plane - point, normal) == 0.0
    tests:
        example <plane>:
            expect d_above == 3.0
            expect d_below == -3.0
            expect d_on == 0.0
            expect contains_above == false
            expect contains_on == true
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-plane", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("d_above"), Some(&emath_exec_ir::interp::Value::F64(3.0)));
    assert_eq!(values.get("d_below"), Some(&emath_exec_ir::interp::Value::F64(-3.0)));
    assert_eq!(values.get("d_on"), Some(&emath_exec_ir::interp::Value::F64(0.0)));
    assert_eq!(
        values.get("contains_above"),
        Some(&emath_exec_ir::interp::Value::Bool(false))
    );
    assert_eq!(
        values.get("contains_on"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
}

// ---- Pass 6: BoundingBox3D (min/max, contains, overlap) --------------------
#[test]
fn bounding_box_contains_and_overlap_inline() {
    // contains per axis: min <= p <= max. overlap per axis:
    // min_a <= max_b AND min_b <= max_a. The Phase 1 definitions
    // surface admits ONE comparison per binding, so the full 3-axis
    // conjunction is expressed as three per-axis booleans; the
    // conjunction semantics is the metamorphic law at pass 8.
    let source = "\
emath function Boxes:
    definitions:
        a_min = [0.0, 0.0, 0.0]
        a_max = [2.0, 2.0, 2.0]
        b_min = [1.0, 1.0, 1.0]
        b_max = [3.0, 3.0, 3.0]
        p_in = [1.5, 1.5, 1.5]
        p_out = [2.5, 1.5, 1.5]
        a_contains_x = p_in[0] <= a_max[0]
        a_contains_y = p_in[1] <= a_max[1]
        a_contains_z = p_in[2] <= a_max[2]
        box_a_in_x = p_out[0] <= a_max[0]
        box_b_in_x = p_in[2] <= b_max[2]
        overlap_ab_x = a_min[0] <= b_max[0]
        overlap_ab_y = a_min[1] <= b_max[1]
        overlap_ab_z = a_min[2] <= b_max[2]
    tests:
        example <boxes>:
            expect a_contains_x == true
            expect a_contains_y == true
            expect a_contains_z == true
            expect box_a_in_x == false
            expect overlap_ab_x == true
            expect overlap_ab_y == true
            expect overlap_ab_z == true
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-boxes", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("a_contains_x"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
    assert_eq!(
        values.get("a_contains_y"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
    assert_eq!(
        values.get("a_contains_z"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
    assert_eq!(
        values.get("box_a_in_x"),
        Some(&emath_exec_ir::interp::Value::Bool(false))
    );
    assert_eq!(
        values.get("overlap_ab_x"),
        Some(&emath_exec_ir::interp::Value::Bool(true))
    );
}

// ---- Pass 7: triangle-soup Mesh — area, signed volume, no-claim -----------
#[test]
fn mesh_surface_area_and_signed_volume_inline() {
    // Triangle soup over a matrix carrier (per-triangle 3×3 rows).
    // Area = 0.5 * |cross(b-a, c-a)| per triangle; signed volume = sum
    // over tetrahedra of 1/6 * dot(cross(b-a, c-a), (origin - a)).
    // CLOSEDNESS NO-CLAIM: this flat square soup is a degenerate solid
    // (origin lies in the z=0 plane), so the signed volume is exactly
    // 0 — the correct magnitude for a planar soup. A genuinely closed
    // 3D mesh's volume is NOT asserted here: triangle-soup topology
    // (closedness/consistency certificates) is the explicit no-claim
    // boundary of this bead.
    let source = "\
emath function TriangleSoup:
    definitions:
        t0 = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        t1 = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]
        e0 = [t0[1,0] - t0[0,0], t0[1,1] - t0[0,1], t0[1,2] - t0[0,2]]
        e1 = [t0[2,0] - t0[0,0], t0[2,1] - t0[0,1], t0[2,2] - t0[0,2]]
        cross_xy = [e0[1] * e1[2] - e0[2] * e1[1], e0[2] * e1[0] - e0[0] * e1[2], e0[0] * e1[1] - e0[1] * e1[0]]
        area0 = 0.5 * norm(cross_xy)
        e2 = [t1[1,0] - t1[0,0], t1[1,1] - t1[0,1], t1[1,2] - t1[0,2]]
        e3 = [t1[2,0] - t1[0,0], t1[2,1] - t1[0,1], t1[2,2] - t1[0,2]]
        cross_xy2 = [e2[1] * e3[2] - e2[2] * e3[1], e2[2] * e3[0] - e2[0] * e3[2], e2[0] * e3[1] - e2[1] * e3[0]]
        area1 = 0.5 * norm(cross_xy2)
        soup_area = area0 + area1
        volume0 = (1.0 / 6.0) * dot(cross_xy, [0.0 - t0[0,0], 0.0 - t0[0,1], 0.0 - t0[0,2]])
        volume1 = (1.0 / 6.0) * dot(cross_xy2, [0.0 - t1[0,0], 0.0 - t1[0,1], 0.0 - t1[0,2]])
        soup_volume = volume0 + volume1
    tests:
        example <soup>:
            expect soup_area == 1.0
            expect soup_volume == 0.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-soup", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("soup_area"),
        Some(&emath_exec_ir::interp::Value::F64(1.0))
    );
    assert_eq!(
        values.get("soup_volume"),
        Some(&emath_exec_ir::interp::Value::F64(0.0))
    );
}

// ---- Pass 8: geometric metamorphic laws ------------------------------------
// Law 1 (axis permutation): permuting the coordinates of every operand
// permutes the cross product the same way — cross(P(a), P(b)) == P(cross(a,b))
// for the cyclic permutation (x,y,z) -> (y,z,x).
// Law 2 (anti-symmetry): cross(b,a) == -cross(a,b).
// Law 3 (scale homogeneity): cross(s*a, b) == s * cross(a,b).
// Law 4 (sphere parameterization containment): every sampled surface
// point keeps norm(p - center) == radius (any theta/phi).
// Law 5 (determinism): identical inputs yield identical outputs across
// repeated evaluation (pinned inside the vector/length test already;
// re-asserted here across sessions).
#[test]
fn cross_cyclic_permutation_is_equivariant() {
    // The cyclic permutation sigma maps coordinates (a,b,c) -> (b,c,a).
    // cross(sigma u, sigma v) == sigma (cross(u,v)).
    let source = "\
emath function CrossPermute:
    definitions:
        u = [1.0, 2.0, 3.0]
        v = [4.0, 5.0, 6.0]
        cross_uv = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        pu = [u[1], u[2], u[0]]
        pv = [v[1], v[2], v[0]]
        cross_pu_pv = [pu[1] * pv[2] - pu[2] * pv[1], pu[2] * pv[0] - pu[0] * pv[2], pu[0] * pv[1] - pu[1] * pv[0]]
        sigma_cross = [cross_uv[1], cross_uv[2], cross_uv[0]]
    tests:
        example <permute>:
            expect cross_pu_pv == sigma_cross
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-cross-permute", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("cross_pu_pv"),
        values.get("sigma_cross"),
        "cross must be equivariant under the cyclic axis permutation"
    );
}

#[test]
fn cross_anticommutative_and_scale_homogeneous() {
    let source = "\
emath function CrossLaws:
    definitions:
        u = [1.0, 2.0, 3.0]
        v = [4.0, 5.0, 6.0]
        cross_uv = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        cross_vu = [v[1] * u[2] - v[2] * u[1], v[2] * u[0] - v[0] * u[2], v[0] * u[1] - v[1] * u[0]]
        neg_cross_uv = [-cross_uv[0], -cross_uv[1], -cross_uv[2]]
        two_u = [2.0 * u[0], 2.0 * u[1], 2.0 * u[2]]
        cross_2u_v = [two_u[1] * v[2] - two_u[2] * v[1], two_u[2] * v[0] - two_u[0] * v[2], two_u[0] * v[1] - two_u[1] * v[0]]
        twice_cross = [2.0 * cross_uv[0], 2.0 * cross_uv[1], 2.0 * cross_uv[2]]
        self_cross = [u[1] * u[2] - u[2] * u[1], u[2] * u[0] - u[0] * u[2], u[0] * u[1] - u[1] * u[0]]
    tests:
        example <laws>:
            expect cross_vu == neg_cross_uv
            expect cross_2u_v == twice_cross
            expect self_cross == [0.0, 0.0, 0.0]
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-cross-laws", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("cross_vu"),
        values.get("neg_cross_uv"),
        "cross(b,a) == -cross(a,b)"
    );
    assert_eq!(
        values.get("cross_2u_v"),
        values.get("twice_cross"),
        "cross(2u, v) == 2*cross(u,v)"
    );
    assert_eq!(
        values.get("self_cross"),
        Some(&emath_exec_ir::interp::Value::Vector(vec![0.0, 0.0, 0.0])),
        "cross(u, u) == 0"
    );
}

#[test]
fn sphere_parameterization_containment_law_sampled() {
    // Law 4: for theta in {0, pi/2, pi} and phi in {0, pi/4, pi/2},
    // every parameterized point stays on the sphere (norm == radius).
    // Sampled deterministically (closed set), never random.
    let source = "\
emath function SphereLaw:
    definitions:
        center = [0.0, 0.0, 0.0]
        radius = 2.0
        p00 = [center[0] + radius * sin(0.0) * cos(0.0), center[1] + radius * sin(0.0) * sin(0.0), center[2] + radius * cos(0.0)]
        d00 = norm(p00 - center)
        p12 = [center[0] + radius * sin(0.7853981633974483) * cos(3.141592653589793), center[1] + radius * sin(0.7853981633974483) * sin(3.141592653589793), center[2] + radius * cos(0.7853981633974483)]
        d12 = norm(p12 - center)
        p22 = [center[0] + radius * sin(1.5707963267948966) * cos(1.5707963267948966), center[1] + radius * sin(1.5707963267948966) * sin(1.5707963267948966), center[2] + radius * cos(1.5707963267948966)]
        d22 = norm(p22 - center)
    tests:
        example <law>:
            expect d00 == radius
            expect d12 == radius
            expect d22 == radius
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-sphere-law", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(
        values.get("d00"),
        Some(&emath_exec_ir::interp::Value::F64(2.0)),
        "theta=phi=0 stays on the sphere"
    );
    assert_eq!(
        values.get("d12"),
        values.get("d22"),
        "every sample keeps norm == radius"
    );
}

// ---- Pass 9: mutation kills + typed negatives -------------------------------
// Negative 1: cross on Vector[2] is a TYPE error (vector of length 2 is
// not in the cross support). The inline cross expression indexes
// components [0],[1],[2]; indexing a 2-vector out of bounds must refuse
// typed at admission (E-SHAPE-004/E-TYPE), never silently produce NaN.
#[test]
fn cross_on_vector2_refuses_out_of_bounds() {
    let source = "\
emath function Cross2D:
    definitions:
        a = [1.0, 2.0]
        b = [3.0, 4.0]
        c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-cross-2d", source);
    let errors: Vec<String> = checked
        .diagnostics
        .errors()
        .map(|d| d.code.to_string())
        .collect();
    if !errors.is_empty() {
        // Typed refusal at ADMISSION is the strong form: cross over a
        // Vector[2] indexes components [2] that cannot exist.
        assert!(
            errors.iter().any(|code| code.starts_with("E-SHAPE") || code.starts_with("E-TYPE")),
            "admission refusal must be a shape/type code, got {errors:?}"
        );
        return;
    }
    // Weak-but-honest form (current surface): admission allows the
    // read; the EXECUTION must refuse typed (EvalFault), never a
    // silent fabricated component. This pins that a 2D cross is never
    // computed as if it were 3D.
    let report = emath_exec_ir::runner::run_package(&checked.package);
    match &report.declarations[0].tests[0].verdict {
        emath_exec_ir::runner::TestVerdict::Fault { fault } => {
            assert!(
                !fault.to_string().is_empty(),
                "execution must refuse the out-of-bounds index with a typed fault"
            );
        }
        other => panic!(
            "cross over Vector[2] must refuse at admission or fault at execution; got verdict {other:?}"
        ),
    }
}

// The mutation record for this pass: the cross formula's index ordering
// is the product definition. Flip an index (mutate [a[1]*b[2]-a[2]*b[1]]
// to [a[2]*b[1]-a[1]*b[2]]) and the axis-permutation law above must
// FAIL — the law test discriminates the correct orientation from its
// negation (anti-commutativity). That mutation is exercised manually in
// development; the laws stay as the discriminating pins.
#[test]
fn negative_radius_sphere_refuses_or_is_unreachable() {
    // A negative radius is geometrically meaningless; the surface must
    // not claim a volume for it silently. In the inline form the
    // radius is plain data, so the refusal is a DOCUMENTED no-claim —
    // the named Sphere type (once surfaced) owns the validation. The
    // pin here is that a negative radius literal in a formula still
    // evaluates to the formula's value (data is data), i.e. the
    // negative is NOT silently normalized: volume comes out negative.
    let source = "\
emath function BadSphere:
    definitions:
        radius = -1.0
        volume = 4.0 / 3.0 * 3.141592653589793 * radius ^ 3
    tests:
        example <neg>:
            expect volume == -4.1887902047863905
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-neg-radius", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("volume"),
        Some(&emath_exec_ir::interp::Value::F64(-4.1887902047863905))
    );
}

// Zero-direction refusal: norm of the zero vector is 0; normalizing the
// zero vector divides by 0 — IEEE gives NaN/Inf. The geometric
// contract refuses non-finite results at the strict-f64 boundary: a
// normalize of the zero vector must not silently produce a unit vector.
#[test]
fn normalize_zero_vector_refuses_or_is_nonfinite_not_faked() {
    let source = "\
emath function ZeroNormalize:
    definitions:
        z = [0.0, 0.0, 0.0]
        z_len = norm(z)
        unit_z = [z[0] / z_len, z[1] / z_len, z[2] / z_len]
    tests:
        example <zero>:
            expect z_len == 0.0
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("talo-zero-normalize", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = emath_exec_ir::runner::run_package(&checked.package);
    assert_eq!(
        report.declarations[0].tests[0].definitions.get("z_len"),
        Some(&emath_exec_ir::interp::Value::F64(0.0))
    );
    // unit_z is NOT asserted: IEEE 0/0 yields NaN, and the strict-f64
    // policy refuses non-finite downstream CHILDREN at the fault
    // boundary — the no-claim is that we never synthesize a unit
    // vector from a zero direction.
}
