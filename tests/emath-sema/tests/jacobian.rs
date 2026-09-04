//! Jacobian evaluation (Track A3). The
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
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
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
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 2, 2);
    assert_eq!(
        data,
        &[2.0, 3.0, 1.0, 1.0],
        "J must equal the hand-derived partials"
    );
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
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 1, 2);
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
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 2, 2);
    assert_eq!(data, &[2.0, 1.0, 2.0, 0.0], "dual rules must hold for exp");
    assert!(test.verdict.expect_passed(), "in-language expect must pass");
}

// --- Metamorphic laws: Jacobian linearity, scaling, and
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

// --- shape determinism. Orientation, row/column ordering,
// typed refusals for non-scalar components, and cross-run stability.

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

#[test]
fn jacobian_wrt_order_swaps_columns_not_rows() {
    // Column j = derivative wrt the j-th wrt variable in SOURCE order:
    // wrt y, x must put df/dy in column 1 — no canonical reordering.
    let result = check_source("jac-wrt-order", JACOBIAN_WRT_ORDER_SWAPPED);
    assert!(
        !result.diagnostics.has_errors(),
        "swapped-wrt fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    // f1 = xy: (df1/dy, df1/dx) = (x, y) = (3, 2); f2 = x+y: (1, 1).
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 2, 2);
    assert_eq!(
        data,
        &[3.0, 2.0, 1.0, 1.0],
        "wrt y, x must place df/dy before df/dx in each row"
    );
    assert!(test.verdict.expect_passed(), "in-language expect must pass");
}

#[test]
fn jacobian_row_order_follows_list_source_order() {
    // Row i = component i in LIST order: swapping the list elements
    // must swap rows, never sort or deduplicate them.
    let result = check_source("jac-row-order", JACOBIAN_ROW_ORDER_SWAPPED);
    assert!(
        !result.diagnostics.has_errors(),
        "swapped-list fixture must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 2, 2);
    assert_eq!(
        data,
        &[1.0, 1.0, 2.0, 3.0],
        "jacobian([f2, f1]) must stack f2's derivatives as row 1"
    );
    assert!(test.verdict.expect_passed(), "in-language expect must pass");
}

#[test]
fn jacobian_of_a_vector_valued_component_refuses_with_typed_error() {
    // A component that is itself a vector has no scalar partial
    // derivative; the matrix cell would be a vector. Must refuse with
    // the typed numeric-body code, never silently flatten or emit a
    // wrong-shaped matrix.
    let result = check_source("jac-vector-component", JACOBIAN_VECTOR_COMPONENT);
    let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        errors.iter().any(|e| e.starts_with("E-TYPE-012")),
        "jacobian of a vector-valued component must refuse with E-TYPE-012; got: {errors:#?}"
    );
}

#[test]
fn jacobian_of_a_matrix_valued_component_refuses_with_typed_error() {
    // A matrix-valued component is even further from a scalar partial;
    // same typed refusal, never a tensor surprise.
    let result = check_source("jac-matrix-component", JACOBIAN_MATRIX_COMPONENT);
    let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        errors.iter().any(|e| e.starts_with("E-TYPE-012")),
        "jacobian of a matrix-valued component must refuse with E-TYPE-012; got: {errors:#?}"
    );
}

#[test]
fn nested_jacobian_refuses_with_typed_error() {
    // A jacobian whose body is another jacobian is a matrix of
    // matrices; second-order derivatives are a different (unshipped)
    // capability. Must refuse with a typed error, never silently
    // reinterpret the inner rows as components.
    let result = check_source("jac-nested", JACOBIAN_NESTED);
    let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        !errors.is_empty(),
        "nested jacobian must refuse with a typed error, not admit"
    );
    assert!(
        errors.iter().any(|e| e.starts_with("E-TYPE-")),
        "nested jacobian refusal must be a typed E-TYPE-* diagnostic; got: {errors:#?}"
    );
}

#[test]
fn jacobian_evaluation_is_deterministic_across_runs() {
    // Same source text, two independent compile+run passes, must
    // produce byte-identical matrix cells: no HashMap iteration, no
    // parallel float reduction anywhere in the parse->eval path.
    let first = check_source("jac-det-1", JACOBIAN_TWO_VAR);
    let second = check_source("jac-det-2", JACOBIAN_TWO_VAR);
    let report = run_package(&first.package);
    let report2 = run_package(&second.package);
    let a = report.declarations[0].tests[0].outputs.get("J");
    let b = report2.declarations[0].tests[0].outputs.get("J");
    assert_eq!(a, b, "two runs of the same source must agree exactly");
}

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

// ── (emath-9bj1 cross-review): nondifferentiable refusals ─────

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

#[test]
fn jacobian_ln_at_negative_input_is_a_nan_cell_never_a_finite_wrong_derivative() {
    // House NaN policy (unguarded-scalar, term_compile.rs): a numeric
    // domain error propagates IEEE NaN — it must never be silently
    // replaced by a finite value. ln(x) at x = -1 has NO derivative;
    // the cell must be NaN.
    let result = check_source("jac-singular-ln", JACOBIAN_SINGULAR_LN);
    assert!(
        !result.diagnostics.has_errors(),
        "fixture must admit (domain errors are runtime, not static): {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let data = matrix_value(test.outputs.get("J").expect("J must be evaluated"), 1, 1);
    assert!(
        data[0].is_nan(),
        "jacobian(ln(x)) at x=-1 must be a NaN cell, got {} (a finite value here is a silently WRONG derivative)",
        data[0]
    );
}

#[test]
fn jacobian_sqrt_and_division_singularities_propagate_ieee_nan_inf() {
    // sqrt(x) at x<0 and 1/x at x=0 follow the house unguarded-scalar
    // policy: IEEE NaN/Inf in the cell, matching plain derivative,
    // never a panic and never a silent finite stand-in.
    let result = check_source("jac-singular-sqrt", JACOBIAN_SINGULAR_SQRT);
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
    let s = matrix_value(test.outputs.get("Js").expect("Js"), 1, 1)[0];
    assert!(s.is_nan(), "sqrt(x) at x=-1 must propagate NaN, got {s}");
    let result = check_source("jac-singular-div", JACOBIAN_SINGULAR_DIV);
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
    let d = matrix_value(test.outputs.get("Jd").expect("Jd"), 1, 1)[0];
    assert!(
        d.is_infinite() && d < 0.0,
        "1/x at x=0 must propagate -Inf, got {d}"
    );
}

#[test]
fn jacobian_nondifferentiable_but_defined_points_use_the_house_subgradient() {
    // House convention (builtin.rs eval_dual_unary): abs'(0) = sgn(0) = 0,
    // floor/ceil have tangent 0 everywhere. The jacobian cells are the
    // same dual nodes, so they must match — 0.0, never a panic.
    let result = check_source("jac-nondiff-points", JACOBIAN_NONDIFFERENTIABLE_POINTS);
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
    for (name, key) in [("abs", "Ja"), ("floor", "Jf"), ("ceil", "Jc")] {
        let data = matrix_value(
            test.outputs.get(key).unwrap_or_else(|| panic!("{key}")),
            1,
            1,
        );
        assert_eq!(
            data[0], 0.0,
            "{name} at the kink must give the house subgradient 0.0"
        );
    }
}

#[test]
fn jacobian_of_a_unit_constant_admits_and_is_zero() {
    // A quantity literal lowers to its SI-scaled scalar with unit dims
    // carried in the type, and is_numeric_element accepts Unit — so a
    // unit-bearing CONSTANT component admits and its derivative is 0
    // in every unit (0 m/s == 0). Pin the house behavior: admitted,
    // [[0.0]], never a refusal and never a panic.
    let result = check_source("jac-unit-const", JACOBIAN_UNIT_CONSTANT);
    assert!(
        !result.diagnostics.has_errors(),
        "unit-constant jacobian must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let data = matrix_value(test.outputs.get("J").expect("J"), 1, 1);
    assert_eq!(data[0], 0.0, "d(3 meters)/dx must be 0");
}

#[test]
fn jacobian_of_a_unit_scaled_variable_matches_plain_derivative_scaling() {
    // q = x * 1 meter lowers to the SI-scaled scalar product; the
    // runtime is unit-less f64 (house convention), so the cell is the
    // bare SI scale factor. The jacobian must equal the plain
    // derivative of the same expression — same gate, same cells.
    let result = check_source("jac-unit-scaled", JACOBIAN_UNIT_SCALED);
    assert!(
        !result.diagnostics.has_errors(),
        "unit-scaled jacobian must admit: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let data = matrix_value(test.outputs.get("J").expect("J"), 1, 1);
    assert_eq!(
        data[0], 1000.0,
        "d(x * 1 km)/dx must equal the plain derivative (SI-scaled 1000.0, unit-less at runtime)"
    );
}

#[test]
fn jacobian_of_a_string_valued_component_refuses_with_typed_error() {
    // A string-valued component has no scalar partial; must refuse
    // with the typed numeric-body code, never a silent zero cell.
    let result = check_source("jac-string-body", JACOBIAN_STRING_BODY);
    let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        errors.iter().any(|e| e.starts_with("E-TYPE-012")),
        "jacobian of a string-valued component must refuse with E-TYPE-012; got: {errors:#?}"
    );
}

#[test]
fn jacobian_of_an_empty_body_refuses_or_admits_typed_never_panics() {
    // `jacobian([]) wrt x` has zero components: whatever the house
    // outcome (typed refusal or an empty Matrix[0, 1]), it must be a
    // defined outcome — never a panic and never a silent wrong shape.
    let result = std::panic::catch_unwind(|| check_source("jac-empty-body", JACOBIAN_EMPTY_BODY));
    let result = match result {
        Ok(result) => result,
        Err(_) => panic!("jacobian([]) wrt x must not panic during admission"),
    };
    let errors: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        errors.iter().any(|e| e.starts_with("E-")),
        "empty-body jacobian must resolve to a typed outcome (refusal or typed admit); got: {errors:#?}"
    );
}

#[test]
fn jacobian_cells_equal_plain_derivative_cells_at_a_singular_point() {
    // House-consistency: at a singular point the jacobian cell and the
    // hand-written derivative cell go through the SAME dual evaluation,
    // so they must be bit-identical — whatever the NaN policy produces.
    let result = check_source(
        "jac-matches-plain",
        JACOBIAN_MATCHES_PLAIN_DERIVATIVE_SINGULAR,
    );
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
    let j = matrix_value(test.outputs.get("J").expect("J"), 1, 1)[0];
    let d = matrix_value(test.outputs.get("d").expect("d"), 1, 1)[0];
    assert_eq!(
        j.to_bits(),
        d.to_bits(),
        "jacobian cell must be bit-identical to the plain derivative cell at x=-1"
    );
}

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
fn grad_ln_at_negative_input_is_a_nan_gradient_never_a_finite_wrong_derivative() {
    // Reverse mode must obey the same house NaN policy as the dual path:
    // ln(x) at x = -1 has NO derivative, so the grad entry must be NaN —
    // never the finite `adj / primal_in` value a naive backward pass
    // produces when the forward primal was NaN but the division is not.
    let result = check_source("grad-singular-ln", GRAD_SINGULAR_LN);
    assert!(
        !result.diagnostics.has_errors(),
        "fixture must admit (domain errors are runtime, not static): {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    match test.outputs.get("g").expect("g must be evaluated") {
        Value::Vector(v) => assert!(
            v[0].is_nan(),
            "grad(ln(x)) at x=-1 must be NaN, got {} (a finite value here is a silently WRONG derivative)",
            v[0]
        ),
        other => panic!("expected Vector[1], got {other:?}"),
    }
}
