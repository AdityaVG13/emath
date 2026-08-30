//! Jacobian evaluation (bead emath-9bj1, Track A3, pass 3). The
//! jacobian surface is parse-time sugar for a matrix literal of
//! existing dual-number forward-mode `derivative` cells, so its
//! evaluation must equal hand-derived partials through the SAME
//! interpreter path as any user-written matrix of derivatives —
//! no new engine. Hand-computed exact values at chosen points (where
//! sin/cos/exp are exact) keep the assertions exact.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_sema::admit::CheckResult;
use emath_syntax::install_source_parser;

fn check_source(name: &str, source: &str) -> CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
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

fn matrix_value(value: &Value, rows: usize, cols: usize) -> &[f64] {
    match value {
        Value::Matrix {
            rows: r,
            cols: c,
            data,
        } => {
            assert_eq!(*r, rows, "matrix rows");
            assert_eq!(*c, cols, "matrix cols");
            data
        }
        other => panic!("expected Matrix[{rows}, {cols}], got {other:?}"),
    }
}

#[test]
fn jacobian_two_var_evaluates_to_hand_derived_partials() {
    let result = check_source("jac-two-var", JACOBIAN_TWO_VAR);
    assert!(
        !result.diagnostics.has_errors(),
        "jacobian fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    // J = [[df1/dx, df1/dy], [df2/dx, df2/dy]] = [[y, x], [1, 1]]
    // at (x, y) = (3, 2): [[2, 3], [1, 1]] in row-major order.
    let data = matrix_value(
        test.outputs.get("J").expect("J must be evaluated"),
        2,
        2,
    );
    assert_eq!(data, &[2.0, 3.0, 1.0, 1.0], "J must equal the hand-derived partials");
    // JVP check through the mat-vec path: J * [1, 2] = [2*1+3*2, 1*1+1*2].
    assert_eq!(
        test.outputs.get("jv"),
        Some(&Value::Vector(vec![8.0, 3.0])),
        "the Jacobian-vector product must equal the hand-computed JVP"
    );
    assert!(
        test.verdict.expect_passed(),
        "the fixture's in-language expect must pass: {}",
        test.verdict
    );
}

#[test]
fn jacobian_scalar_body_is_a_one_row_matrix() {
    let result = check_source("jac-scalar-row", JACOBIAN_SCALAR_ROW);
    assert!(
        !result.diagnostics.has_errors(),
        "scalar-body jacobian must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    // f = x^2 + y: [df/dx, df/dy] = [2x, 1] at (3, 2) = [6, 1].
    let data = matrix_value(
        test.outputs.get("J").expect("J must be evaluated"),
        1,
        2,
    );
    assert_eq!(data, &[6.0, 1.0], "row jacobian must equal [2x, 1]");
    assert!(test.verdict.expect_passed(), "in-language expect must pass");
}

#[test]
fn jacobian_cells_use_dual_rules_beyond_polynomials() {
    let result = check_source("jac-dual-rules", JACOBIAN_DUAL_RULES);
    assert!(
        !result.diagnostics.has_errors(),
        "dual-rule jacobian must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    // f1 = exp(x)*y: df1/dx = exp(x)*y = 2, df1/dy = exp(x) = 1 at (0, 2).
    // f2 = x*y:      df2/dx = y = 2,        df2/dy = x = 0.
    let data = matrix_value(
        test.outputs.get("J").expect("J must be evaluated"),
        2,
        2,
    );
    assert_eq!(data, &[2.0, 1.0, 2.0, 0.0], "dual rules must hold for exp");
    assert!(test.verdict.expect_passed(), "in-language expect must pass");
}

// --- Metamorphic laws (pass 8): Jacobian linearity, scaling, and
// composition consistency. The oracle problem (no closed-form general
// Jacobian) is bypassed by relating Jacobians of transformed programs
// to Jacobians of the originals through in-language matrix operators
// (MatrixAdd / MatrixScale) and through per-cell derivative
// recomposition (the chain/product-rule consistency law).

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

#[test]
fn mr_jacobian_of_a_sum_is_the_sum_of_jacobians() {
    // J(f + g) == J(f) + J(g): the Jacobian is a linear map. The
    // in-language matrix equality is exact at the given point.
    let result = check_source("jac-additivity", JACOBIAN_ADDITIVITY);
    assert!(
        !result.diagnostics.has_errors(),
        "additivity fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "J(f+g) must equal J(f)+J(g): {}",
        test.verdict
    );
}

#[test]
fn mr_jacobian_of_a_scaled_body_is_the_scaled_jacobian() {
    // J(c*f) == c*J(f): homogeneous linearity.
    let result = check_source("jac-scaling", JACOBIAN_SCALING);
    assert!(
        !result.diagnostics.has_errors(),
        "scaling fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "J(c*f) must equal c*J(f): {}",
        test.verdict
    );
}

#[test]
fn mr_jacobian_matches_per_cell_recomposition_through_intermediate_definitions() {
    // Composition consistency: the Jacobian of an expression built from
    // intermediate definitions equals the matrix of independent
    // derivative cells of the same sub-expressions — the chain/product
    // rules must compose identically in both paths.
    let result = check_source("jac-composition", JACOBIAN_COMPOSITION_CONSISTENCY);
    assert!(
        !result.diagnostics.has_errors(),
        "composition fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "the Jacobian must equal the per-cell derivative recomposition: {}",
        test.verdict
    );
}

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

#[test]
fn jacobian_cells_match_hand_derived_exact_rules() {
    // At x = 1: q' = 3/(1+3)^2 = 3/16 = 0.1875 (quotient rule),
    // s' = (sqrt(x) * x)' = 1/(2*sqrt(1)) + sqrt(1) = 1.5 (product rule),
    // l' = d/dx ln(sqrt(x)) = 1/(2*x) = 0.5 (chain rule). Every expected
    // cell is an exactly representable dyadic rational, so equality is exact.
    let result = check_source("jac-exact-rules", JACOBIAN_EXACT_RULES);
    assert!(
        !result.diagnostics.has_errors(),
        "fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert!(
        test.verdict.expect_passed(),
        "jacobian cells must equal the hand-derived [3/16, 3/2, 1/2] row: {}",
        test.verdict
    );
}
