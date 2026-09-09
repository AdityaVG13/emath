//! 3D geometry primitives (failure-first).
//!
//! Contracts:
//! - geometry is EXPRESSED IN `.emath` over the existing generic
//!   Vector/Matrix/Tensor surface: no geometry core enum, no crate,
//!   no parser fork, no domain-named core variants;
//! - `cross`/`normalize`/`distance` are the capability-cell/expression
//!   layer's job (composable from existing VectorIndex/VectorCreate and
//!   scalar ops), NOT new builtin arms — this test pins the SURFACE
//!   contract and the current gap;
//! - `Option[Point3D]` ray results use the real Option<T>
//!   surface once it lands (E-TYPE-010 today);
//! - tests live under `tests/`, never under `crates/`.

use emath_test_harness::{Probe, Source, boot};

const PRIMITIVES: &str = "language/examples/geometry/3d-primitives.emath";

#[test]
fn geometry3d() {
    boot();
    let mut p = Probe::new("3d geometry over generic Vector surface with declared cells and numeric laws");
    p.case("declared-cells-admit", |p| {
        Source::from_workspace(PRIMITIVES).must_admit(p);
    });
    p.case("calls-lower-to-apply", |p| {
        let checked = Source::from_workspace(PRIMITIVES).must_admit(p);
        let names: Vec<&str> = checked
            .package
            .capabilities
            .iter()
            .map(|capability| capability.name.0.as_str())
            .collect();
        let mut cell_indices = Vec::new();
        for expected in ["std.geometry.cross", "std.geometry.normalize", "std.geometry.distance"] {
            match names.iter().position(|name| *name == expected) {
                Some(index) => cell_indices.push(index),
                None => {
                    p.fail("cell-declared", format!("cell {expected} must be declared: {names:?}"));
                }
            }
        }
        let applies: Vec<usize> = checked
            .package
            .exprs
            .iter()
            .filter_map(|node| match node {
                emath_ir::ExprNode::Apply { capability, .. } => Some(capability.index()),
                _ => None,
            })
            .collect();
        p.demand("has-apply", !applies.is_empty(), "example must contain capability call sites");
        for index in cell_indices {
            p.demand(format!("apply/{index}"), applies.contains(&index), format!("cell index {index} must be reached through ExprNode::Apply; applies = {applies:?}"));
        }
    });
    p.case("vector-surface", |p| {
        Source::from_str(
            "talo-vector-surface",
            "emath function VectorSurface:\n    definitions:\n        a = [1.0, 2.0, 2.0]\n        x = a[0]\n        d = dot(a, [2.0, 1.0, 0.0])\n        n = norm(a)\n    tests:\n        example <surface>:\n            expect x == 1.0\n            expect d == 4.0\n            expect n == 3.0\n",
        )
        .eval_tests(p);
    });
    p.case("cross-axes", |p| {
        Source::from_str(
            "talo-cross-axes",
            "emath function CrossAxes:\n    definitions:\n        x_axis = [1.0, 0.0, 0.0]\n        y_axis = [0.0, 1.0, 0.0]\n        z_axis = [0.0, 0.0, 1.0]\n        x_cross_y = [x_axis[1] * y_axis[2] - x_axis[2] * y_axis[1], x_axis[2] * y_axis[0] - x_axis[0] * y_axis[2], x_axis[0] * y_axis[1] - x_axis[1] * y_axis[0]]\n        y_cross_z = [y_axis[1] * z_axis[2] - y_axis[2] * z_axis[1], y_axis[2] * z_axis[0] - y_axis[0] * z_axis[2], y_axis[0] * z_axis[1] - y_axis[1] * z_axis[0]]\n        z_cross_x = [z_axis[1] * x_axis[2] - z_axis[2] * x_axis[1], z_axis[2] * x_axis[0] - z_axis[0] * x_axis[2], z_axis[0] * x_axis[1] - z_axis[1] * x_axis[0]]\n        y_cross_x = [y_axis[1] * x_axis[2] - y_axis[2] * x_axis[1], y_axis[2] * x_axis[0] - y_axis[0] * x_axis[2], y_axis[0] * x_axis[1] - y_axis[1] * x_axis[0]]\n    tests:\n        example <axes>:\n            expect x_cross_y == [0.0, 0.0, 1.0]\n            expect y_cross_z == [1.0, 0.0, 0.0]\n            expect z_cross_x == [0.0, 1.0, 0.0]\n            expect y_cross_x == [0.0, 0.0, -1.0]\n",
        )
        .eval_tests(p);
    });
    p.case("length-normalize-distance", |p| {
        Source::from_str(
            "talo-vector3",
            "emath function Vector3:\n    definitions:\n        v = [3.0, 4.0, 0.0]\n        v_len = norm(v)\n        v_unit = [v[0] / v_len, v[1] / v_len, v[2] / v_len]\n        a = [1.0, 0.0, 0.0]\n        b = [1.0, 1.0, 0.0]\n        d = norm(a - b)\n        d2 = norm([1.0, 2.0, 2.0])\n    tests:\n        example <v3>:\n            expect v_len == 5.0\n            expect v_unit == [0.6, 0.8, 0.0]\n            expect d == 1.0\n            expect d2 == 3.0\n",
        )
        .eval_tests(p);
    });
    p.case("sphere", |p| {
        Source::from_str(
            "talo-sphere",
            "emath function UnitSphere:\n    definitions:\n        center = [0.0, 0.0, 0.0]\n        radius = 1.0\n        p_inside = [0.5, 0.5, 0.5]\n        p_outside = [2.0, 0.0, 0.0]\n        is_inside = norm(p_inside - center) <= radius\n        is_outside = norm(p_outside - center) <= radius\n        volume = 4.0 / 3.0 * 3.141592653589793 * radius ^ 3\n        surface = 4.0 * 3.141592653589793 * radius ^ 2\n    tests:\n        example <sphere>:\n            expect is_inside == true\n            expect is_outside == false\n            expect volume == 4.1887902047863905\n            expect surface == 12.566370614359172\n",
        )
        .eval_tests(p);
    });
    p.case("sphere-param", |p| {
        Source::from_str(
            "talo-sphere-param",
            "emath function SphereParam:\n    definitions:\n        center = [0.0, 0.0, 0.0]\n        radius = 1.0\n        theta = 0.0\n        phi = 0.0\n        sp = [sin(phi) * cos(theta), sin(phi) * sin(theta), cos(phi)]\n        dist = norm(sp - center)\n    tests:\n        example <param>:\n            expect sp == [0.0, 0.0, 1.0]\n            expect dist == 1.0\n",
        )
        .eval_tests(p);
    });
    p.case("plane", |p| {
        Source::from_str(
            "talo-plane",
            "emath function XYPlane:\n    definitions:\n        normal = [0.0, 0.0, 1.0]\n        point = [0.0, 0.0, 0.0]\n        above = [1.0, 2.0, 3.0]\n        below = [1.0, 2.0, -3.0]\n        on_plane = [1.0, 2.0, 0.0]\n        d_above = dot(above - point, normal)\n        d_below = dot(below - point, normal)\n        d_on = dot(on_plane - point, normal)\n        contains_above = dot(above - point, normal) == 0.0\n        contains_on = dot(on_plane - point, normal) == 0.0\n    tests:\n        example <plane>:\n            expect d_above == 3.0\n            expect d_below == -3.0\n            expect d_on == 0.0\n            expect contains_above == false\n            expect contains_on == true\n",
        )
        .eval_tests(p);
    });
    p.case("boxes", |p| {
        Source::from_str(
            "talo-boxes",
            "emath function Boxes:\n    definitions:\n        a_min = [0.0, 0.0, 0.0]\n        a_max = [2.0, 2.0, 2.0]\n        b_min = [1.0, 1.0, 1.0]\n        b_max = [3.0, 3.0, 3.0]\n        p_in = [1.5, 1.5, 1.5]\n        p_out = [2.5, 1.5, 1.5]\n        a_contains_x = p_in[0] <= a_max[0]\n        a_contains_y = p_in[1] <= a_max[1]\n        a_contains_z = p_in[2] <= a_max[2]\n        box_a_in_x = p_out[0] <= a_max[0]\n        box_b_in_x = p_in[2] <= b_max[2]\n        overlap_ab_x = a_min[0] <= b_max[0]\n        overlap_ab_y = a_min[1] <= b_max[1]\n        overlap_ab_z = a_min[2] <= b_max[2]\n    tests:\n        example <boxes>:\n            expect a_contains_x == true\n            expect a_contains_y == true\n            expect a_contains_z == true\n            expect box_a_in_x == false\n            expect overlap_ab_x == true\n            expect overlap_ab_y == true\n            expect overlap_ab_z == true\n",
        )
        .eval_tests(p);
    });
    p.case("soup", |p| {
        Source::from_str(
            "talo-soup",
            "emath function TriangleSoup:\n    definitions:\n        t0 = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]\n        t1 = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]\n        e0 = [t0[1,0] - t0[0,0], t0[1,1] - t0[0,1], t0[1,2] - t0[0,2]]\n        e1 = [t0[2,0] - t0[0,0], t0[2,1] - t0[0,1], t0[2,2] - t0[0,2]]\n        cross_xy = [e0[1] * e1[2] - e0[2] * e1[1], e0[2] * e1[0] - e0[0] * e1[2], e0[0] * e1[1] - e0[1] * e1[0]]\n        area0 = 0.5 * norm(cross_xy)\n        e2 = [t1[1,0] - t1[0,0], t1[1,1] - t1[0,1], t1[1,2] - t1[0,2]]\n        e3 = [t1[2,0] - t1[0,0], t1[2,1] - t1[0,1], t1[2,2] - t1[0,2]]\n        cross_xy2 = [e2[1] * e3[2] - e2[2] * e3[1], e2[2] * e3[0] - e2[0] * e3[2], e2[0] * e3[1] - e2[1] * e3[0]]\n        area1 = 0.5 * norm(cross_xy2)\n        soup_area = area0 + area1\n        volume0 = (1.0 / 6.0) * dot(cross_xy, [0.0 - t0[0,0], 0.0 - t0[0,1], 0.0 - t0[0,2]])\n        volume1 = (1.0 / 6.0) * dot(cross_xy2, [0.0 - t1[0,0], 0.0 - t1[0,1], 0.0 - t1[0,2]])\n        soup_volume = volume0 + volume1\n    tests:\n        example <soup>:\n            expect soup_area == 1.0\n            expect soup_volume == 0.0\n",
        )
        .eval_tests(p);
    });
    p.case("permute-law", |p| {
        Source::from_str(
            "talo-cross-permute",
            "emath function CrossPermute:\n    definitions:\n        u = [1.0, 2.0, 3.0]\n        v = [4.0, 5.0, 6.0]\n        cross_uv = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]\n        pu = [u[1], u[2], u[0]]\n        pv = [v[1], v[2], v[0]]\n        cross_pu_pv = [pu[1] * pv[2] - pu[2] * pv[1], pu[2] * pv[0] - pu[0] * pv[2], pu[0] * pv[1] - pu[1] * pv[0]]\n        sigma_cross = [cross_uv[1], cross_uv[2], cross_uv[0]]\n    tests:\n        example <permute>:\n            expect cross_pu_pv == sigma_cross\n",
        )
        .eval_tests(p);
    });
    p.case("anticommute-scale", |p| {
        Source::from_str(
            "talo-cross-laws",
            "emath function CrossLaws:\n    definitions:\n        u = [1.0, 2.0, 3.0]\n        v = [4.0, 5.0, 6.0]\n        cross_uv = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]\n        cross_vu = [v[1] * u[2] - v[2] * u[1], v[2] * u[0] - v[0] * u[2], v[0] * u[1] - v[1] * u[0]]\n        neg_cross_uv = [-cross_uv[0], -cross_uv[1], -cross_uv[2]]\n        two_u = [2.0 * u[0], 2.0 * u[1], 2.0 * u[2]]\n        cross_2u_v = [two_u[1] * v[2] - two_u[2] * v[1], two_u[2] * v[0] - two_u[0] * v[2], two_u[0] * v[1] - two_u[1] * v[0]]\n        twice_cross = [2.0 * cross_uv[0], 2.0 * cross_uv[1], 2.0 * cross_uv[2]]\n        self_cross = [u[1] * u[2] - u[2] * u[1], u[2] * u[0] - u[0] * u[2], u[0] * u[1] - u[1] * u[0]]\n    tests:\n        example <laws>:\n            expect cross_vu == neg_cross_uv\n            expect cross_2u_v == twice_cross\n            expect self_cross == [0.0, 0.0, 0.0]\n",
        )
        .eval_tests(p);
    });
    p.case("sphere-law", |p| {
        Source::from_str(
            "talo-sphere-law",
            "emath function SphereLaw:\n    definitions:\n        center = [0.0, 0.0, 0.0]\n        radius = 2.0\n        p00 = [center[0] + radius * sin(0.0) * cos(0.0), center[1] + radius * sin(0.0) * sin(0.0), center[2] + radius * cos(0.0)]\n        d00 = norm(p00 - center)\n        p12 = [center[0] + radius * sin(0.7853981633974483) * cos(3.141592653589793), center[1] + radius * sin(0.7853981633974483) * sin(3.141592653589793), center[2] + radius * cos(0.7853981633974483)]\n        d12 = norm(p12 - center)\n        p22 = [center[0] + radius * sin(1.5707963267948966) * cos(1.5707963267948966), center[1] + radius * sin(1.5707963267948966) * sin(1.5707963267948966), center[2] + radius * cos(1.5707963267948966)]\n        d22 = norm(p22 - center)\n    tests:\n        example <law>:\n            expect d00 == radius\n            expect d12 == radius\n            expect d22 == radius\n",
        )
        .eval_tests(p);
    });
    p.case("cross2d-refuses", |p| {
        let checked = Source::from_str(
            "talo-cross-2d",
            "emath function Cross2D:\n    definitions:\n        a = [1.0, 2.0]\n        b = [3.0, 4.0]\n        c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]\n",
        )
        .check();
        let codes = emath_test_harness::error_codes(&checked.diagnostics);
        if !codes.is_empty() {
            p.demand("cross2d/admission-is-shape-or-type", codes.iter().any(|code| code.starts_with("E-SHAPE") || code.starts_with("E-TYPE")), format!("admission refusal must be shape/type, got {codes:?}"));
        } else {
            let report = emath_exec_ir::runner::run_package(&checked.package);
            match report.declarations.first().and_then(|declaration| declaration.tests.first()) {
                Some(test) => match &test.verdict {
                    emath_exec_ir::runner::TestVerdict::Fault { fault } => {
                        p.demand("cross2d/fault-typed", !fault.to_string().is_empty(), "execution must refuse out-of-bounds with a typed fault");
                    }
                    other => {
                        p.fail("cross2d/must-refuse", format!("cross over Vector[2] must refuse at admission or fault at execution, got {other:?}"));
                    }
                },
                None => {
                    p.fail("cross2d/must-refuse", "cross over Vector[2] produced no test verdict to refuse");
                }
            }
        }
    });
    p.case("negative-radius", |p| {
        Source::from_str(
            "talo-neg-radius",
            "emath function BadSphere:\n    definitions:\n        radius = -1.0\n        volume = 4.0 / 3.0 * 3.141592653589793 * radius ^ 3\n    tests:\n        example <neg>:\n            expect volume == -4.1887902047863905\n",
        )
        .eval_tests(p);
    });
    p.case("zero-normalize", |p| {
        Source::from_str(
            "talo-zero-normalize",
            "emath function ZeroNormalize:\n    definitions:\n        z = [0.0, 0.0, 0.0]\n        z_len = norm(z)\n        unit_z = [z[0] / z_len, z[1] / z_len, z[2] / z_len]\n    tests:\n        example <zero>:\n            expect z_len == 0.0\n",
        )
        .eval_tests(p);
    });
    p.finish();
}
