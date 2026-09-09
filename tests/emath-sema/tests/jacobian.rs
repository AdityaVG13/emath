//! Jacobian evaluation (Track A3). The
//! jacobian surface is parse-time sugar for a matrix literal of
//! existing dual-number forward-mode `derivative` cells, so its
//! evaluation must equal hand-derived partials through the SAME
//! interpreter path as any user-written matrix of derivatives —
//! no new engine. Hand-computed exact values at chosen points (where
//! sin/cos/exp are exact) keep the assertions exact.

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_test_harness::{boot, Probe, Source};

fn cells<'a>(p: &mut Probe, name: &str, value: Option<&'a Value>, rows: usize, cols: usize) -> &'a [f64] {
    match value {
        Some(Value::Matrix { rows: r, cols: c, data }) if *r == rows && *c == cols => data,
        other => {
            p.fail(name, format!("expected Matrix[{rows}, {cols}], got {other:?}"));
            &[]
        }
    }
}

fn verdict(p: &mut Probe, name: &str, passed: bool, detail: &impl std::fmt::Display) {
    p.demand(name, passed, format!("in-language expect must pass: {detail}"));
}

const JACOBIAN_TWO_VAR: &str = "\
emath function JacobianTwoVar:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]
        jv: Vector[2]

    definitions:
        f1 = x * y
        f2 = x + y
        J = jacobian([f1, f2]) wrt x, y
        jv = J * [1.0, 2.0]

    tests:
        example <eval>:
            given x = 3
            given y = 2
            expect jv == [8.0, 3.0]
";

const JACOBIAN_SCALAR_ROW: &str = "\
emath function JacobianScalarRow:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[1, 2]

    definitions:
        f = x * x + y
        J = jacobian(f) wrt x, y

    tests:
        example <eval>:
            given x = 3
            given y = 2
            expect J == [[6.0, 1.0]]
";

const JACOBIAN_DUAL_RULES: &str = "\
emath function JacobianDualRules:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f1 = exp(x) * y
        f2 = x * y
        J = jacobian([f1, f2]) wrt x, y

    tests:
        example <eval>:
            given x = 0
            given y = 2
            expect J == [[2.0, 1.0], [2.0, 0.0]]
";

const JACOBIAN_ADDITIVITY: &str = "\
emath function JacobianAdditivity:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J_sum: Matrix[2, 2]
        J1: Matrix[2, 2]
        J2: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y
        u = x * x
        v = x - y
        J_sum = jacobian([f1 + u, f2 + v]) wrt x, y
        J1 = jacobian([f1, f2]) wrt x, y
        J2 = jacobian([u, v]) wrt x, y

    tests:
        example <linearity>:
            given x = 3
            given y = 2
            expect J_sum == J1 + J2
";

const JACOBIAN_SCALING: &str = "\
emath function JacobianScaling:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J_scaled: Matrix[2, 2]
        J_plain: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y
        J_scaled = jacobian([3.0 * f1, 3.0 * f2]) wrt x, y
        J_plain = jacobian([f1, f2]) wrt x, y

    tests:
        example <scaling>:
            given x = 3
            given y = 2
            expect J_scaled == 3.0 * J_plain
";

const JACOBIAN_COMPOSITION_CONSISTENCY: &str = "\
emath function JacobianComposition:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]
        J_cells: Matrix[2, 2]

    definitions:
        u = x * y
        v = x + y
        p = u * v
        J = jacobian([p, u]) wrt x, y
        J_cells = [[(derivative(p) wrt x), (derivative(p) wrt y)],
                   [(derivative(u) wrt x), (derivative(u) wrt y)]]

    tests:
        example <consistency>:
            given x = 3
            given y = 2
            expect J == J_cells
";

const JACOBIAN_EXACT_RULES: &str = "\
emath function JacobianExactRules:
    inputs:
        x: Float64

    outputs:
        J: Matrix[3, 1]

    definitions:
        q = x / (x + 3.0)
        s = sqrt(x) * x
        l = ln(sqrt(x))
        J = jacobian([q, s, l]) wrt x

    tests:
        example <eval>:
            given x = 1
            expect J == [[0.1875], [1.5], [0.5]]
";

const JACOBIAN_WRT_ORDER_SWAPPED: &str = "\
emath function JacobianWrtOrderSwapped:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y
        J = jacobian([f1, f2]) wrt y, x

    tests:
        example <eval>:
            given x = 3
            given y = 2
            expect J == [[3.0, 2.0], [1.0, 1.0]]
";

const JACOBIAN_ROW_ORDER_SWAPPED: &str = "\
emath function JacobianRowOrderSwapped:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y
        J = jacobian([f2, f1]) wrt x, y

    tests:
        example <eval>:
            given x = 3
            given y = 2
            expect J == [[1.0, 1.0], [2.0, 3.0]]
";

const JACOBIAN_VECTOR_COMPONENT: &str = "\
emath function JacobianVectorComponent:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[1, 2]

    definitions:
        v = [x, y]
        J = jacobian([v]) wrt x, y
";

const JACOBIAN_MATRIX_COMPONENT: &str = "\
emath function JacobianMatrixComponent:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[1, 2]

    definitions:
        M = [[x, y], [y, x]]
        J = jacobian([M]) wrt x, y
";

const JACOBIAN_NESTED: &str = "\
emath function JacobianNested:
    inputs:
        x: Float64
        y: Float64

    outputs:
        J: Matrix[2, 2]

    definitions:
        f1 = x * y
        f2 = x + y
        J = jacobian(jacobian([f1, f2]) wrt x, y) wrt x, y
";

const JACOBIAN_SINGULAR_LN: &str = "\
emath function JacobianSingularLn:
    inputs:
        x: Float64

    outputs:
        J: Matrix[1, 1]
        chk: Float64

    definitions:
        J = jacobian(ln(x)) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = -1
            expect chk == 1.0
";

const JACOBIAN_SINGULAR_SQRT: &str = "\
emath function JacobianSingularSqrt:
    inputs:
        x: Float64

    outputs:
        Js: Matrix[1, 1]
        chk: Float64

    definitions:
        Js = jacobian(sqrt(x)) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = -1
            expect chk == 1.0
";

const JACOBIAN_SINGULAR_DIV: &str = "\
emath function JacobianSingularDiv:
    inputs:
        x: Float64

    outputs:
        Jd: Matrix[1, 1]
        chk: Float64

    definitions:
        Jd = jacobian(1 / x) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = 0
            expect chk == 1.0
";

const JACOBIAN_NONDIFFERENTIABLE_POINTS: &str = "\
emath function JacobianNondifferentiablePoints:
    inputs:
        x: Float64

    outputs:
        Ja: Matrix[1, 1]
        Jf: Matrix[1, 1]
        Jc: Matrix[1, 1]
        chk: Float64

    definitions:
        Ja = jacobian(abs(x)) wrt x
        Jf = jacobian(floor(x)) wrt x
        Jc = jacobian(ceil(x)) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = 0
            expect chk == 1.0
";

const JACOBIAN_UNIT_CONSTANT: &str = "\
emath function JacobianUnitConstant:
    inputs:
        x: Float64

    outputs:
        J: Matrix[1, 1]
        chk: Float64

    definitions:
        J = jacobian(3 m) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = 2
            expect chk == 1.0
";

const JACOBIAN_UNIT_SCALED: &str = "\
emath function JacobianUnitScaled:
    inputs:
        x: Float64

    outputs:
        J: Matrix[1, 1]
        chk: Float64

    definitions:
        q = x * 1 km
        J = jacobian(q) wrt x
        chk = derivative(x) wrt x

    tests:
        example <eval>:
            given x = 2
            expect chk == 1.0
";

const JACOBIAN_STRING_BODY: &str = "\
emath function JacobianStringBody:
    inputs:
        x: Float64

    outputs:
        J: Matrix[1, 1]

    definitions:
        J = jacobian(\"not a number\") wrt x

    tests:
        example <eval>:
            given x = 2
            expect J == [[0.0]]
";

const JACOBIAN_EMPTY_BODY: &str = "\
emath function JacobianEmptyBody:
    inputs:
        x: Float64

    outputs:
        J: Matrix[0, 1]

    definitions:
        J = jacobian([]) wrt x

    tests:
        example <eval>:
            given x = 2
            expect chk == 1.0
";

const JACOBIAN_MATCHES_PLAIN_DERIVATIVE_SINGULAR: &str = "\
emath function JacobianMatchesPlainDerivative:
    inputs:
        x: Float64

    outputs:
        J: Matrix[1, 1]
        d: Matrix[1, 1]

    definitions:
        J = jacobian(ln(x)) wrt x
        d = [[derivative(ln(x)) wrt x]]

    tests:
        example <eval>:
            given x = -1
            expect d == d
";

const GRAD_SINGULAR_LN: &str = "\
emath function GradSingularLn:
    inputs:
        x: Float64

    outputs:
        g: Vector[1]
        chk: Float64

    definitions:
        g = grad(ln(x))
        chk = x

    tests:
        example <eval>:
            given x = -1
            expect chk == -1.0
";

#[test]
fn jacobian() {
    boot();
    let mut p = Probe::new("jacobian sugar evaluates to hand-derived partials and refuses non-scalar bodies");
    p.case("two-var", |p| {
        let result = Source::from_str("jac-two-var", JACOBIAN_TWO_VAR).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        // J = [[df1/dx, df1/dy], [df2/dx, df2/dy]] = [[y, x], [1, 1]] at (3, 2).
        let data = cells(&mut *p, "J:shape", test.outputs.get("J"), 2, 2);
        p.eq("J", data, [2.0, 3.0, 1.0, 1.0].as_slice());
        // JVP through the mat-vec path: J * [1, 2] = [2 + 6, 1 + 2].
        p.eq(
            "jv",
            test.outputs.get("jv"),
            Some(&Value::Vector(vec![8.0, 3.0])),
        );
        verdict(&mut *p, "verdict", test.verdict.expect_passed(), &test.verdict);
    });
    p.case("scalar-row", |p| {
        let result = Source::from_str("jac-scalar-row", JACOBIAN_SCALAR_ROW).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        // f = x^2 + y: [df/dx, df/dy] = [2x, 1] = [6, 1] at (3, 2).
        let data = cells(&mut *p, "J:shape", test.outputs.get("J"), 1, 2);
        p.eq("J", data, [6.0, 1.0].as_slice());
        verdict(&mut *p, "verdict", test.verdict.expect_passed(), &test.verdict);
    });
    p.case("dual-rules", |p| {
        let result = Source::from_str("jac-dual-rules", JACOBIAN_DUAL_RULES).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        // f1 = exp(x)*y: [exp(0)*2, exp(0)] = [2, 1]; f2 = x*y: [y, x] = [2, 0] at (0, 2).
        let data = cells(&mut *p, "J:shape", test.outputs.get("J"), 2, 2);
        p.eq("J", data, [2.0, 1.0, 2.0, 0.0].as_slice());
        verdict(&mut *p, "verdict", test.verdict.expect_passed(), &test.verdict);
    });
    // Metamorphic laws: J(f+g) == J(f)+J(g), J(c*f) == c*J(f), and the
    // Jacobian equals the per-cell derivative recomposition. Each
    // in-language matrix equality is exact at the given point.
    p.case("additivity", |p| {
        Source::from_str("jac-additivity", JACOBIAN_ADDITIVITY).eval_tests(&mut *p);
    });
    p.case("scaling", |p| {
        Source::from_str("jac-scaling", JACOBIAN_SCALING).eval_tests(&mut *p);
    });
    p.case("composition", |p| {
        Source::from_str("jac-composition", JACOBIAN_COMPOSITION_CONSISTENCY).eval_tests(&mut *p);
    });
    // At x = 1: q' = 3/(1+3)^2 = 3/16 (quotient), s' = 1/2 + 1 = 3/2
    // (product), l' = 1/2 (chain). Every cell is an exactly
    // representable dyadic rational, so the in-language equality is exact.
    p.case("exact-rules", |p| {
        Source::from_str("jac-exact-rules", JACOBIAN_EXACT_RULES).eval_tests(&mut *p);
    });
    p.case("wrt-order", |p| {
        let result = Source::from_str("jac-wrt-order", JACOBIAN_WRT_ORDER_SWAPPED).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        // wrt y, x puts df/dy in column 1: f1 = xy -> (3, 2); f2 = x+y -> (1, 1).
        let data = cells(&mut *p, "J:shape", test.outputs.get("J"), 2, 2);
        p.eq("J", data, [3.0, 2.0, 1.0, 1.0].as_slice());
        verdict(&mut *p, "verdict", test.verdict.expect_passed(), &test.verdict);
    });
    p.case("row-order", |p| {
        let result = Source::from_str("jac-row-order", JACOBIAN_ROW_ORDER_SWAPPED).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        // jacobian([f2, f1]) stacks f2's derivatives as row 1.
        let data = cells(&mut *p, "J:shape", test.outputs.get("J"), 2, 2);
        p.eq("J", data, [1.0, 1.0, 2.0, 3.0].as_slice());
        verdict(&mut *p, "verdict", test.verdict.expect_passed(), &test.verdict);
    });
    p.case("vector-component", |p| {
        Source::from_str("jac-vector-component", JACOBIAN_VECTOR_COMPONENT)
            .must_refuse(&mut *p, &["E-TYPE-012"]);
    });
    p.case("matrix-component", |p| {
        Source::from_str("jac-matrix-component", JACOBIAN_MATRIX_COMPONENT)
            .must_refuse(&mut *p, &["E-TYPE-012"]);
    });
    p.case("nested", |p| {
        // A jacobian of a jacobian is a matrix of matrices; second-order
        // derivatives are unshipped, so this must be a typed refusal.
        let result = Source::from_str("jac-nested", JACOBIAN_NESTED).check();
        let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
        p.demand(
            "refused",
            !errors.is_empty(),
            "nested jacobian must refuse, not admit".to_string(),
        );
        p.demand(
            "typed",
            errors.iter().any(|e| e.starts_with("E-TYPE-")),
            format!("nested refusal must be E-TYPE-*, got {errors:#?}"),
        );
    });
    p.case("deterministic", |p| {
        // Two independent compile+run passes must agree exactly: no
        // HashMap iteration or parallel float reduction in the path.
        let first = Source::from_str("jac-det-1", JACOBIAN_TWO_VAR).must_admit(&mut *p);
        let second = Source::from_str("jac-det-2", JACOBIAN_TWO_VAR).must_admit(&mut *p);
        if first.diagnostics.has_errors() || second.diagnostics.has_errors() {
            return;
        }
        p.eq(
            "rerun",
            run_package(&first.package).declarations[0].tests[0].outputs.get("J"),
            run_package(&second.package).declarations[0].tests[0].outputs.get("J"),
        );
    });
    p.case("singular-ln", |p| {
        // House NaN policy: ln(x) at x = -1 has NO derivative; the cell
        // must be NaN, never a finite stand-in.
        let result = Source::from_str("jac-singular-ln", JACOBIAN_SINGULAR_LN).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let data = cells(
            &mut *p,
            "J:shape",
            report.declarations[0].tests[0].outputs.get("J"),
            1,
            1,
        );
        p.demand(
            "nan",
            data.first().map(|v| v.is_nan()).unwrap_or(false),
            format!("jacobian(ln(x)) at x=-1 must be NaN, got {data:?}"),
        );
    });
    p.case("singular-sqrt-div", |p| {
        let sqrt = Source::from_str("jac-singular-sqrt", JACOBIAN_SINGULAR_SQRT).must_admit(&mut *p);
        if !sqrt.diagnostics.has_errors() {
            let report = run_package(&sqrt.package);
            let data = cells(
                &mut *p,
                "Js:shape",
                report.declarations[0].tests[0].outputs.get("Js"),
                1,
                1,
            );
            p.demand(
                "sqrt-nan",
                data.first().map(|v| v.is_nan()).unwrap_or(false),
                format!("jacobian(sqrt(x)) at x=-1 must be NaN, got {data:?}"),
            );
        }
        let div = Source::from_str("jac-singular-div", JACOBIAN_SINGULAR_DIV).must_admit(&mut *p);
        if div.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&div.package);
        let data = cells(
            &mut *p,
            "Jd:shape",
            report.declarations[0].tests[0].outputs.get("Jd"),
            1,
            1,
        );
        p.demand(
            "div-neg-inf",
            data.first().map(|v| v.is_infinite() && *v < 0.0).unwrap_or(false),
            format!("jacobian(1/x) at x=0 must be -Inf, got {data:?}"),
        );
    });
    p.case("nondiff-points", |p| {
        // House subgradients: abs'(0) = 0, floor/ceil tangent 0 everywhere.
        let result =
            Source::from_str("jac-nondiff-points", JACOBIAN_NONDIFFERENTIABLE_POINTS).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        for (name, key) in [("abs", "Ja"), ("floor", "Jf"), ("ceil", "Jc")] {
            let data = cells(&mut *p, key, test.outputs.get(key), 1, 1);
            p.eq(name, data.first().copied().unwrap_or(f64::NAN), 0.0);
        }
    });
    p.case("unit-constant", |p| {
        // A unit-bearing constant admits; d(3 meters)/dx is 0 in every unit.
        let result = Source::from_str("jac-unit-const", JACOBIAN_UNIT_CONSTANT).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let data = cells(
            &mut *p,
            "J:shape",
            report.declarations[0].tests[0].outputs.get("J"),
            1,
            1,
        );
        p.eq("zero", data.first().copied().unwrap_or(f64::NAN), 0.0);
    });
    p.case("unit-scaled", |p| {
        // q = x * 1 km lowers to the SI-scaled product; the runtime is
        // unit-less f64, so the cell is the bare scale factor 1000.0.
        let result = Source::from_str("jac-unit-scaled", JACOBIAN_UNIT_SCALED).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let data = cells(
            &mut *p,
            "J:shape",
            report.declarations[0].tests[0].outputs.get("J"),
            1,
            1,
        );
        p.eq("si-scale", data.first().copied().unwrap_or(f64::NAN), 1000.0);
    });
    p.case("string-body", |p| {
        Source::from_str("jac-string-body", JACOBIAN_STRING_BODY).must_refuse(&mut *p, &["E-TYPE-012"]);
    });
    p.case("empty-body", |p| {
        // Zero components: whatever the house outcome (typed refusal or
        // typed admit), it must be defined — never a panic.
        match std::panic::catch_unwind(|| Source::from_str("jac-empty-body", JACOBIAN_EMPTY_BODY).check()) {
            Ok(result) => {
                let errors: Vec<String> =
                    result.diagnostics.errors().map(|d| d.to_string()).collect();
                p.demand(
                    "typed",
                    errors.iter().any(|e| e.starts_with("E-")),
                    format!("empty-body jacobian must resolve to a typed outcome, got {errors:#?}"),
                );
            }
            Err(_) => {
                p.fail("no-panic", "jacobian([]) wrt x must not panic during admission");
            }
        }
    });
    p.case("matches-plain-derivative", |p| {
        // At a singular point the jacobian cell and the hand-written
        // derivative cell share one dual evaluation: bit-identical.
        let result = Source::from_str(
            "jac-matches-plain",
            JACOBIAN_MATCHES_PLAIN_DERIVATIVE_SINGULAR,
        )
        .must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        let test = &report.declarations[0].tests[0];
        let j = cells(&mut *p, "J:shape", test.outputs.get("J"), 1, 1);
        let d = cells(&mut *p, "d:shape", test.outputs.get("d"), 1, 1);
        p.eq(
            "bit-identical",
            j.first().map(|v| v.to_bits()),
            d.first().map(|v| v.to_bits()),
        );
    });
    p.case("grad-singular-ln", |p| {
        // Reverse mode obeys the same NaN policy: grad(ln(x)) at x = -1
        // is NaN, never the finite value of a naive backward pass.
        let result = Source::from_str("grad-singular-ln", GRAD_SINGULAR_LN).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let report = run_package(&result.package);
        match report.declarations[0].tests[0].outputs.get("g") {
            Some(Value::Vector(values)) => {
                p.demand(
                    "nan",
                    values.first().map(|v| v.is_nan()).unwrap_or(false),
                    format!("grad(ln(x)) at x=-1 must be NaN, got {values:?}"),
                );
            }
            other => {
                p.fail("shape", format!("expected Vector[1], got {other:?}"));
            }
        }
    });
    p.finish();
}
