//! Tests for Vector and Matrix types, literals, indexing, and arithmetic in semantic analysis.

use std::collections::BTreeMap;

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_test_harness::{boot, Probe, Source};

fn f64_of(p: &mut Probe, test: &emath_exec_ir::runner::TestRun, name: &str) -> f64 {
    match test.outputs.get(name) {
        Some(Value::F64(v)) => *v,
        other => {
            p.fail(name, format!("{name} must be F64, got {other:?}"));
            f64::NAN
        }
    }
}

fn admit_eval(p: &mut Probe, name: &str, source: &str) -> emath_exec_ir::runner::TestRun {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        p.fail(name, "cannot evaluate a source that did not admit".to_string());
    }
    let report = run_package(&result.package);
    if report.declarations.is_empty() || report.declarations[0].tests.is_empty() {
        p.fail(name, "declaration must run once".to_string());
    }
    report
        .declarations
        .into_iter()
        .next()
        .and_then(|d| d.tests.into_iter().next())
        .unwrap_or_else(|| {
            // Unreachable after the fail above; empty run keeps downstream
            // f64_of checks honest instead of aborting the probe.
            emath_exec_ir::runner::TestRun {
                name: name.to_string(),
                given: BTreeMap::new(),
                state: BTreeMap::new(),
                definitions: BTreeMap::new(),
                outputs: BTreeMap::new(),
                verdict: emath_exec_ir::runner::TestVerdict::Computed,
            }
        })
}

#[test]
fn vector_matrix_contract() {
    boot();
    let mut p = Probe::new("vector/matrix/tensor literals, shape gates, and numeric kernels");
    p.case("literals-admit", |p| {
        Source::from_str(
            "vec",
            "\
emath function VectorOps:
    inputs:
        x: Float64
    outputs:
        v: Vector[3]
        first: Float64
    definitions:
        v = [x, 2.0 * x, 3.0]
        first = v[0]
",
        )
        .must_admit(p);
        Source::from_str(
            "mat",
            "\
emath function MatrixOps:
    inputs:
        a: Float64
        b: Float64
    outputs:
        m: Matrix[2, 2]
        elem: Float64
    definitions:
        m = [[a, b], [0.0, 1.0]]
        elem = m[0, 1]
",
        )
        .must_admit(p);
        Source::from_str(
            "linalg",
            "\
emath function LinearAlgebra:
    inputs:
        v1: Vector[3]
        v2: Vector[3]
        m1: Matrix[3, 3]
        s: Float64
    outputs:
        v_sum: Vector[3]
        v_diff: Vector[3]
        v_scaled: Vector[3]
        d: Float64
        n: Float64
        mv: Vector[3]
        m_trans: Matrix[3, 3]
        m_sq: Matrix[3, 3]
    definitions:
        v_sum = v1 + v2
        v_diff = v1 - v2
        v_scaled = s * v1
        d = dot(v1, v2)
        n = norm(v1)
        mv = m1 * v1
        m_trans = transpose(m1)
        m_sq = m1 * m1
",
        )
        .must_admit(p);
        Source::from_str(
            "tensor-slice",
            "\
emath function TensorSlice:
    inputs:
        n: Float64
    outputs:
        t: Tensor[2, 2, 2]
        face: Matrix[2, 2]
    definitions:
        t = [[[1.0, 2.0], [3.0, 4.0]], [[5.0, 6.0], [7.0, 8.0]]]
        face = t[0, :, :]
",
        )
        .must_admit(p);
        Source::from_str(
            "nat-index",
            "\
emath function NatIndex:
    inputs:
        v: Vector[3]
        i: Nat
    outputs:
        x: Float64
    definitions:
        x = v[i]
",
        )
        .must_admit(p);
        Source::from_workspace("tests/fixtures/language/numerical/spatial-3d.emath").must_admit(p);
    });
    p.case("shape-refusals", |p| {
        Source::from_str(
            "ragged",
            "\
emath function BadMatrix:
    inputs:
        x: Float64
    outputs:
        m: Matrix[2, 2]
    definitions:
        m = [[1.0, 2.0], [3.0]]
",
        )
        .must_refuse(p, &["E-SHAPE-005"]);
        Source::from_str(
            "dim-mismatch",
            "\
emath function DimMismatch:
    inputs:
        v1: Vector[2]
        v2: Vector[3]
    outputs:
        v3: Vector[2]
    definitions:
        v3 = v1 + v2
",
        )
        .must_refuse(p, &["E-SHAPE-005"]);
        Source::from_str(
            "matvec",
            "\
emath function MatDimMismatch:
    inputs:
        m1: Matrix[2, 3]
        v: Vector[2]
    outputs:
        res: Vector[2]
    definitions:
        res = m1 * v
",
        )
        .must_refuse(p, &["E-SHAPE-002"]);
        Source::from_str(
            "rank",
            "\
emath function BadIndex:
    inputs:
        v: Vector[3]
    outputs:
        x: Float64
    definitions:
        x = v[0, 1]
",
        )
        .must_refuse(p, &["E-SHAPE-006"]);
        Source::from_str(
            "neg-index",
            "\
emath function NegIndex:
    inputs:
        v: Vector[3]
    outputs:
        x: Float64
    definitions:
        x = v[-1]
",
        )
        .must_refuse(p, &["E-SHAPE-006"]);
        Source::from_str(
            "broadcast",
            "\
emath function Broadcast:
    inputs:
        v3: Vector[3]
        v1: Vector[1]
    outputs:
        out: Vector[3]
    definitions:
        out = v3 + v1
",
        )
        .must_refuse(p, &["E-SHAPE-005"]);
        for (name, body) in [
            ("empty-lit", "emath function EmptyLit:\n    outputs:\n        v: Vector[1]\n    definitions:\n        v = []\n"),
            ("mean-empty", "emath function MeanEmpty:\n    outputs:\n        m: Float64\n    definitions:\n        m = mean([])\n"),
            ("norm-empty", "emath function NormEmpty:\n    outputs:\n        n: Float64\n    definitions:\n        n = norm([])\n"),
        ] {
            Source::from_str(name, body).must_refuse(p, &["E-SHAPE-004"]);
        }
        Source::from_str(
            "len-gone",
            "\
emath function LenGone:
    inputs:
        v: Vector[3]
    outputs:
        n: Float64
    definitions:
        n = len(v)
",
        )
        .must_refuse(p, &["E-TYPE-003"]);
        Source::from_str(
            "binder-leak",
            "\
emath function Leak:
    outputs:
        s: Float64
        leaked: Float64
    definitions:
        s = sum k in 1..4: k
        leaked = k
",
        )
        .must_refuse(p, &["E-TYPE-002"]);
        let bad = Source::from_str(
            "bad-spatial",
            "\
emath function BadSpatial3d:
    inputs:
        u: Matrix[3, 3]
    outputs:
        lap: Matrix[3, 3]
    definitions:
        lap = laplacian_3d(u, 1.0)
",
        )
        .check();
        let items: Vec<(String, String)> = bad
            .diagnostics
            .errors()
            .map(|d| (d.code.to_string(), d.to_string()))
            .collect();
        p.demand(
            "spatial-rank-gate",
            items.iter().any(|(c, m)| m.contains("Tensor") && (c == "E-LANG-FEATURE" || c == "E-TYPE-012")),
            format!("laplacian_3d on a matrix must name the Tensor gate, got {items:?}"),
        );
        let bare = Source::from_str(
            "partial-bare",
            "\
emath function PartialBare:
    inputs:
        x: Float64
        y: Float64
    outputs:
        d: Float64
    definitions:
        d = partial(x * y) wrt x
",
        )
        .check();
        let messages: Vec<String> = bare.diagnostics.errors().map(|d| d.to_string()).collect();
        p.demand(
            "partial-needs-holding",
            bare.diagnostics.has_errors() && messages.iter().any(|m| m.contains("holding")),
            format!("partial without holding must refuse naming holding, got {messages:?}"),
        );
    });
    p.case("matmul-einsum", |p| {
        let face = admit_eval(p, "tensor-face", &Source::from_workspace("tests/fixtures/language/intro/tensor-face.emath").text().to_string());
        p.eq(
            "first-face",
            face.outputs.get("face"),
            Some(&Value::Matrix { rows: 2, cols: 2, data: vec![1.0, 2.0, 3.0, 4.0] }),
        );
        p.demand("face-passed", face.verdict.expect_passed(), format!("got {}", face.verdict));
        let ein = admit_eval(p, "einsum", &Source::from_workspace("tests/fixtures/language/intro/einsum.emath").text().to_string());
        // [[1,2],[3,4]] x [[5,6],[7,8]] = [[19,22],[43,50]] by hand.
        let product = Some(&Value::Matrix { rows: 2, cols: 2, data: vec![19.0, 22.0, 43.0, 50.0] });
        p.eq("matmul", ein.outputs.get("ab"), product);
        p.eq("einsum-matches", ein.outputs.get("ein"), ein.outputs.get("ab"));
        p.eq("implicit-matches", ein.outputs.get("implicit"), ein.outputs.get("ab"));
        p.eq("dot-matches", ein.outputs.get("ddot"), ein.outputs.get("ein_dot"));
        p.demand("einsum-passed", ein.verdict.expect_passed(), format!("got {}", ein.verdict));
        Source::from_str(
            "einsum-ids",
            "\
emath function EinsumIds:
    inputs:
        n: Float64
    outputs:
        ab: Matrix[2, 2]
        ein: Matrix[2, 2]
        implicit: Matrix[2, 2]
        ddot: Float64
        ein_dot: Float64
        tt: Matrix[2, 3]
        m: Matrix[2, 3]
    definitions:
        a = [[1.0, 2.0], [3.0, 4.0]]
        b = [[5.0, 6.0], [7.0, 8.0]]
        ab = a * b
        ein = einsum(\"ik,kj->ij\", a, b)
        implicit = einsum(\"ik,kj\", a, b)
        u = [n, 2.0, 3.0]
        v = [4.0, 5.0, 6.0]
        ddot = dot(u, v)
        ein_dot = einsum(\"i,i->\", u, v)
        m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
        tt = transpose(transpose(m))
    tests:
        example <ids>:
            given n = 1.0
            expect ab == ein and ein == implicit and ddot == ein_dot and tt == m
",
        )
        .eval_tests(p);
    });
    p.case("folds-sums", |p| {
        Source::from_workspace("tests/fixtures/language/intro/sum-one-to-five.emath").eval_tests(p);
        Source::from_str(
            "fold",
            "\
emath function Fold:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        face: Matrix[2, 2]
    definitions:
        s = sum([1, 2, 3, 4, 5])
        p = product([[1, 2], [3, 4]]) * n
        face = [[1.0, 2.0], [3.0, 4.0]]
    tests:
        example <known>:
            given n = 1.0
            expect s == 15 and p == 24 and face == [[1.0, 2.0], [3.0, 4.0]]
",
        )
        .eval_tests(p);
        // 1+2+3+4+5 = 15 and 1*2*3*4 = 24 by hand.
        let stats = admit_eval(
            p,
            "vec-stats",
            "\
emath function VecStats:
    inputs:
        v: Vector[3]
    outputs:
        avg: Estimate
        a: Vector[3]
    definitions:
        avg = mean(v)
        a = abs(v)
    tests:
        example <stats>:
            given v = [1.0, -2.0, 4.0]
            expect a == [1.0, 2.0, 4.0]
",
        );
        p.eq(
            "mean-estimate",
            stats.outputs.get("avg"),
            Some(&Value::Record {
                type_name: "Estimate".into(),
                fields: BTreeMap::from([
                    ("value".into(), Value::F64(1.0)),
                    ("method".into(), Value::Text("mean".into())),
                    ("n".into(), Value::I64(3)),
                ]),
            }),
        );
        p.eq("abs-vector", stats.outputs.get("a"), Some(&Value::Vector(vec![1.0, 2.0, 4.0])));
        p.demand("stats-passed", stats.verdict.expect_passed(), format!("got {}", stats.verdict));
        for (name, body) in [
            (
                "triangular",
                "\
emath function TriangularSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n: i
    tests:
        example <triangular>:
            given n = 5
            expect total == 10
",
            ),
            (
                "range-sum",
                "\
emath function VectorRangeSum:
    inputs:
        v: Vector[3]
    outputs:
        s: Float64
    definitions:
        n = length(v)
        s = sum i in 0..n: v[i]
    tests:
        example <range>:
            given v = [1.0, 2.0, 3.0]
            expect s == 6
",
            ),
            (
                "filtered-sum",
                "\
emath function FilteredSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n if i > 2: i
    tests:
        example <filtered>:
            given n = 5
            expect total == 7
",
            ),
            (
                "empty-filter",
                "\
emath function EmptyFilteredSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n if i < 0: i
    tests:
        example <empty>:
            given n = 5
            expect total == 0
",
            ),
            (
                "filtered-forall",
                "\
emath function FilteredForAll:
    inputs:
        n: Float64
    outputs:
        ok: Bool
    definitions:
        ok = forall i in 0..n if i < n: i >= 0
    tests:
        example <filteredforall>:
            given n = 5
            expect ok == true
",
            ),
            (
                "forall-pos",
                "\
emath function AllPositive:
    inputs:
        v: Vector[3]
    outputs:
        all_pos: Bool
    definitions:
        n = length(v)
        all_pos = forall i in 0..n: v[i] > 0
    tests:
        example <positive>:
            given v = [1.0, 2.0, 3.0]
            expect all_pos == true
",
            ),
            (
                "forall-neg",
                "\
emath function AllPositiveCheck:
    inputs:
        v: Vector[3]
    outputs:
        all_pos: Bool
    definitions:
        n = length(v)
        all_pos = forall i in 0..n: v[i] > 0
    tests:
        example <mixed>:
            given v = [1.0, -2.0, 3.0]
            expect all_pos == false
",
            ),
            (
                "exists-zero",
                "\
emath function HasZero:
    inputs:
        v: Vector[3]
    outputs:
        has_zero: Bool
    definitions:
        n = length(v)
        has_zero = exists i in 0..n: v[i] == 0
    tests:
        example <zero>:
            given v = [1.0, 0.0, 3.0]
            expect has_zero == true
",
            ),
            (
                "factorial-20",
                "\
emath function ExactFactorial:
    inputs:
        n: Int
    outputs:
        fac: Int
    definitions:
        fac = product i in 1..=n: i
    tests:
        example <twenty>:
            given n = 20
            expect fac == 2432902008176640000
",
            ),
            (
                "factorial-fns",
                "\
emath function Fac:
    inputs:
        n: Int
    outputs:
        z: Int
        f5: Int
        f20: Int
    definitions:
        z = factorial(0)
        f5 = factorial(n)
        f20 = factorial(20)
    tests:
        example <ok>:
            given n = 5
            expect z == 1
            expect f5 == 120
            expect f20 == 2432902008176640000
",
            ),
        ] {
            Source::from_str(name, body).eval_tests(p);
        }
        // 20! = 2432902008176640000 needs the exact i64 path (not f64-rounded).
        let fac21 = admit_eval(
            p,
            "fac-21",
            "\
emath function Fac21:
    inputs:
        n: Int
    outputs:
        f: Int
    definitions:
        f = factorial(n)
    tests:
        example <overflow>:
            given n = 21
            expect f == 0
",
        );
        p.demand("fac21-refused", fac21.verdict.is_refused(), format!("21! must named-refuse, got {}", fac21.verdict));
        let fac_nan = admit_eval(
            p,
            "fac-nan",
            "\
emath function FacNan:
    inputs:
        n: Int
    outputs:
        f: Int
    definitions:
        f = factorial(n / 0)
    tests:
        example <nan>:
            given n = 0
            expect f == 1
",
        );
        p.demand("fac-nan-refused", fac_nan.verdict.is_refused(), format!("0/0 must not silently return 1, got {}", fac_nan.verdict));
    });
    p.case("calculus-opt", |p| {
        let ix = admit_eval(
            p,
            "integral-x",
            "\
emath function IntegrateX:
    inputs:
        a: Float64
        b: Float64
    outputs:
        area: Float64
    definitions:
        area = integral x in a..b: x
    tests:
        example <linear>:
            given a = 0
            given b = 2
",
        );
        let int_x = f64_of(p, &ix, "area");
        p.close("int-x-0-2", int_x, 2.0, 1e-10);
        let ix2 = admit_eval(
            p,
            "integral-x2",
            "\
emath function IntegrateXSquared:
    inputs:
        n: Float64
    outputs:
        area: Float64
    definitions:
        area = integral x in 0..3: x * x * n
    tests:
        example <quadratic>:
            given n = 1.0
",
        );
        let int_x2 = f64_of(p, &ix2, "area");
        p.close("int-x2-0-3", int_x2, 9.0, 1e-10);
        Source::from_str(
            "autodiff",
            "\
emath function AutoDiffSquare:
    inputs:
        x: Float64
    outputs:
        y: Float64
        dy: Float64
    definitions:
        y = x * x
        dy = derivative(y) wrt x
    tests:
        example <parabola>:
            given x = 3
            expect dy == 6
",
        )
        .eval_tests(p);
        Source::from_str(
            "autodiff-sin",
            "\
emath function AutoDiffSin:
    inputs:
        x: Float64
    outputs:
        dy: Float64
    definitions:
        dy = derivative(sin(x)) wrt x
    tests:
        example <sin>:
            given x = 0
            expect dy == 1
",
        )
        .eval_tests(p);
        Source::from_str(
            "power",
            "\
emath function Power:
    inputs:
        x: Float64
    outputs:
        y: Float64
        dy: Float64
    definitions:
        y = x^2
        dy = derivative(x^2) wrt x
    tests:
        example <nine>:
            given x = 3
            expect y == 9
            expect dy == 6
",
        )
        .eval_tests(p);
        Source::from_str(
            "partial-held",
            "\
emath function PartialHeld:
    inputs:
        x: Float64
        y: Float64
    outputs:
        d: Float64
    definitions:
        d = partial(x * y) wrt x holding y
    tests:
        example <held>:
            given x = 3
            given y = 5
            expect d == 5
",
        )
        .eval_tests(p);
        Source::from_str("complex", "\
emath function CplxElem:
    inputs:
        n: Float64
    outputs:
        s: Complex
        l: Complex
        mag: Float64
    definitions:
        s = sqrt(-1 + 0i)
        l = ln(-1 + 0i)
        mag = abs(i) * n
    tests:
        example <principal>:
            given n = 1.0
            expect abs(s - i) < 1e-12
            expect abs(l - 3.141592653589793i) < 1e-12
            expect mag == 1
",
        )
        .eval_tests(p);
        Source::from_str("dot-deriv", "\
emath function DotDeriv:
    inputs:
        x: Float64
    outputs:
        d: Float64
    definitions:
        d = derivative(dot([x, 1.0], [1.0, x])) wrt x
    tests:
        example <two>:
            given x = 3.0
            expect d == 2.0
",
        )
        .eval_tests(p);
        let root = admit_eval(
            p,
            "solve",
            "\
emath function SolveRoot:
    inputs:
        x: Float64
    outputs:
        root: Float64
    definitions:
        residual = x * x - 4
        root = solve(residual) wrt x
    tests:
        example <from_one>:
            given x = 1
            expect abs(root - 2) < 0.001
",
        );
        let root_val = f64_of(p, &root, "root");
        p.close("root-is-2", root_val, 2.0, 1e-9);
        p.close("root-residual", root_val * root_val - 4.0, 0.0, 1e-12);
        let min = admit_eval(
            p,
            "minimize",
            "\
emath function MinimizeSquare:
    inputs:
        x: Float64
    outputs:
        optimum: Float64
    definitions:
        loss = (x - 3) * (x - 3)
        optimum = minimize(loss) wrt x
    tests:
        example <from_zero>:
            given x = 0
            expect abs(optimum - 3) < 0.1
",
        );
        let opt = f64_of(p, &min, "optimum");
        p.close("min-at-3", opt, 3.0, 1e-6);
        p.close("min-stationary", 2.0 * (opt - 3.0), 0.0, 1e-6);
        let max = admit_eval(
            p,
            "maximize",
            "\
emath function MaximizeNegSquare:
    inputs:
        x: Float64
    outputs:
        optimum: Float64
    definitions:
        score = -(x - 2) * (x - 2)
        optimum = maximize(score) wrt x
    tests:
        example <from_zero>:
            given x = 0
            expect abs(optimum - 2) < 0.1
",
        );
        let peak = f64_of(p, &max, "optimum");
        p.close("max-at-2", peak, 2.0, 1e-6);
        p.close("max-stationary", -2.0 * (peak - 2.0), 0.0, 1e-6);
        let multi = admit_eval(
            p,
            "multivar",
            "\
emath function MultiVarOpt:
    inputs:
        x: Float64
        y: Float64
    outputs:
        opt_x: Float64
        opt_y: Float64
    definitions:
        loss = (x - 1) * (x - 1) + (y - 2) * (y - 2)
        opt_x = minimize(loss) wrt x, y
        opt_y = minimize(loss) wrt y, x
    tests:
        example <bowl>:
            given x = 0
            given y = 0
            expect abs(opt_x - 1) < 0.1
            expect abs(opt_y - 2) < 0.1
",
        );
        let bowl_x = f64_of(p, &multi, "opt_x");
        let bowl_y = f64_of(p, &multi, "opt_y");
        p.close("bowl-x", bowl_x, 1.0, 1e-6);
        p.close("bowl-y", bowl_y, 2.0, 1e-6);
        let constrained = admit_eval(
            p,
            "constrained",
            "\
emath function ConstrainedMin:
    inputs:
        x: Float64
        y: Float64
    outputs:
        opt_x: Float64
        opt_y: Float64
    constraints:
        x + y >= 1
    definitions:
        objective = x * x + y * y
        opt_x = minimize(objective) wrt x, y
        opt_y = minimize(objective) wrt y, x
    tests:
        example <demo>:
            given x = 0
            given y = 0
            expect abs(opt_x - 0.5) < 0.01
            expect abs(opt_y - 0.5) < 0.01
            expect opt_x + opt_y >= 0.999
",
        );
        let (cx, cy) = (f64_of(p, &constrained, "opt_x"), f64_of(p, &constrained, "opt_y"));
        p.close("constrained-x", cx, 0.5, 0.01);
        p.close("constrained-y", cy, 0.5, 0.01);
        p.demand("penalty-enforced", cx + cy >= 0.999, format!("x+y>=1, got {}", cx + cy));
        p.demand("constrained-passed", constrained.verdict.expect_passed(), format!("got {}", constrained.verdict));
        let solve_ex = Source::from_workspace("tests/fixtures/language/intro/solve.emath").must_admit(p);
        let solve_report = run_package(&solve_ex.package);
        p.eq("two-basins", solve_report.declarations[0].tests.len(), 2);
        let pos = f64_of(p, &solve_report.declarations[0].tests[0], "root");
        let neg = f64_of(p, &solve_report.declarations[0].tests[1], "root");
        p.close("basin-pos", pos, 2.0, 1e-9);
        p.close("basin-pos-residual", pos * pos - 4.0, 0.0, 1e-12);
        p.close("basin-neg", neg, -2.0, 1e-9);
        p.close("basin-neg-residual", neg * neg - 4.0, 0.0, 1e-12);
        let opt_ex = Source::from_workspace("language/examples/intro/optimize.emath").must_admit(p);
        let opt_report = run_package(&opt_ex.package);
        let opt_test = &opt_report.declarations[0].tests[0];
        let ex_min = f64_of(p, opt_test, "min_x");
        let ex_max = f64_of(p, opt_test, "max_x");
        p.close("min-stationary-ex", 2.0 * (ex_min - 3.0), 0.0, 1e-6);
        p.close("max-stationary-ex", -2.0 * (ex_max - 2.0), 0.0, 1e-6);
        p.demand("optimize-passed", opt_test.verdict.expect_passed(), format!("got {}", opt_test.verdict));
        let con_ex = Source::from_workspace("tests/fixtures/language/intro/constrained-optimization.emath").must_admit(p);
        let con_test = &run_package(&con_ex.package).declarations[0].tests[0];
        let con_x = f64_of(p, con_test, "opt_x");
        let con_y = f64_of(p, con_test, "opt_y");
        p.demand(
            "fixture-penalty",
            con_x + con_y >= 0.999,
            "example must nearly satisfy x+y>=1".to_string(),
        );
        p.demand("fixture-passed", con_test.verdict.expect_passed(), format!("got {}", con_test.verdict));
    });
    p.case("fields-builtins", |p| {
        Source::from_str(
            "heat-rod",
            r#"
emath function HeatStep:
    inputs:
        u: Vector[5]
        alpha: Float64
        dt: Float64

    outputs:
        next: Vector[5]
        next_dirichlet: Vector[5]
        next_neumann: Vector[5]

    definitions:
        next = u + dt * alpha * laplacian(u, 1.0)
        next_dirichlet = u + dt * alpha * laplacian_dirichlet(u, 1.0, 0.0, 0.0)
        next_neumann = u + dt * alpha * laplacian_neumann(u, 1.0)

    tests:
        example <constant_holds>:
            given u = [5.0, 5.0, 5.0, 5.0, 5.0]
            given alpha = 1.0
            given dt = 1.0
            expect next == [5.0, 5.0, 5.0, 5.0, 5.0]

        example <zero_dt_identity>:
            given u = [0.0, 1.0, 4.0, 9.0, 16.0]
            given alpha = 1.0
            given dt = 0.0
            expect next == [0.0, 1.0, 4.0, 9.0, 16.0]

        example <dirichlet_cools_boundary>:
            given u = [5.0, 5.0, 5.0, 5.0, 5.0]
            given alpha = 1.0
            given dt = 1.0
            expect next_dirichlet == [0.0, 5.0, 5.0, 5.0, 0.0]

        example <neumann_insulated_linear>:
            given u = [0.0, 1.0, 2.0, 3.0, 4.0]
            given alpha = 1.0
            given dt = 1.0
            expect next_neumann == [2.0, 1.0, 2.0, 3.0, 2.0]
"#,
        )
        .eval_tests(p);
        Source::from_str(
            "heat-plate",
            r#"
emath function HeatPlate:
    inputs:
        u: Matrix[3, 3]
        alpha: Float64
        dt: Float64

    outputs:
        next: Matrix[3, 3]

    definitions:
        next = u + dt * alpha * laplacian_2d(u, 1.0)

    tests:
        example <constant_holds>:
            given u = [[5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0]]
            given alpha = 1.0
            given dt = 1.0
            expect next == [[5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0]]

        example <hot_spot_diffuses>:
            given u = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]
            given alpha = 1.0
            given dt = 1.0
            expect next == [[0.0, 1.0, 0.0], [1.0, -3.0, 1.0], [0.0, 1.0, 0.0]]
"#,
        )
        .eval_tests(p);
        Source::from_workspace("tests/fixtures/language/numerical/gradient-field.emath").eval_tests(p);
        let grad = Source::from_workspace("tests/fixtures/language/numerical/gradient-field.emath").must_admit(p);
        let grad_report = run_package(&grad.package);
        p.eq(
            "ramp-gradient",
            grad_report.declarations[0].tests[0].outputs.get("du"),
            Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0, 1.0])),
        );
        Source::from_str(
            "scalars",
            "\
emath function Scalars:
    inputs:
        n: Float64
    outputs:
        h: Float64
        l: Float64
        c: Float64
        r: Float64
        cb: Float64
        s0: Float64
        sneg: Float64
        spos: Float64
    definitions:
        h = hypot(n, 4)
        l = lerp(0, 10, 0.5)
        c = clamp(12, 0, 10)
        r = recip(4)
        cb = cbrt(8)
        s0 = sign(0)
        sneg = sign(-2)
        spos = sign(3)
    tests:
        example <closed>:
            given n = 3.0
            expect h == 5
            expect l == 5
            expect c == 10
            expect r == 0.25
            expect cb == 2
            expect s0 == 0
            expect sneg == -1
            expect spos == 1
",
        )
        .eval_tests(p);
        let modular = Source::from_workspace("tests/fixtures/language/intro/modular-arithmetic.emath").must_admit(p);
        let modular_report = run_package(&modular.package);
        p.eq("decls", modular_report.declarations.len(), 4);
        let basics = &modular_report.declarations[0].tests[0];
        p.eq("inv3", basics.outputs.get("inv3"), Some(&Value::I64(5)));
        p.eq("mod-check", basics.outputs.get("check"), Some(&Value::Bool(true)));
        p.eq("fac6", basics.outputs.get("fac6"), Some(&Value::I64(720)));
        p.eq("wilson", basics.outputs.get("wilson_ok"), Some(&Value::Bool(true)));
        p.demand("basics-passed", basics.verdict.expect_passed(), format!("got {}", basics.verdict));
        p.eq(
            "rs-distance",
            modular_report.declarations[1].tests[0].outputs.get("distance"),
            Some(&Value::I64(5)),
        );
        Source::from_str(
            "duals",
            "\
emath function Duals:
    inputs:
        base: Float64
    outputs:
        a: Float64
        s: Float64
        m: Float64
        d: Float64
        n: Float64
        via_op: Float64
        via_caret: Float64
        via_pow: Float64
    definitions:
        a = core::math::add(base, 3)
        s = sub(10, 4)
        m = math::mul(3, 5)
        d = div(9, 3)
        n = neg(6)
        via_op = 2 + 3
        via_caret = 2 ^ 3
        via_pow = core::math::pow(2, 3)
    tests:
        example <duals>:
            given base = 2.0
            expect a == 5
            expect s == 6
            expect m == 15
            expect d == 3
            expect n == -6
            expect a == via_op
            expect via_caret == 8
            expect via_pow == via_caret
",
        )
        .eval_tests(p);
        Source::from_str(
            "noted",
            "\
emath function Noted:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
        s: Float64
    definitions:
        r = x ⊕ y
        s = x + y
    tests:
        example <plus_glyph>:
            given x = 2
            given y = 3
            expect r == 5
            expect r == s
notation infixl 40 \"⊕\" => core::math::add
",
        )
        .eval_tests(p);
        Source::from_workspace("tests/fixtures/language/intro/notation-ops.emath").eval_tests(p);
        let noted = Source::from_workspace("tests/fixtures/language/intro/notation-ops.emath").must_admit(p);
        let noted_report = run_package(&noted.package);
        p.eq("pow-sqrt-recip", noted_report.declarations[0].tests[0].outputs.get("t"), Some(&Value::F64(0.125)));
        Source::from_str(
            "involutions",
            "\
emath function InvRound:
    inputs:
        k: Int
    outputs:
        n: Int
        r: Float64
        tt: Matrix[2, 3]
        m: Matrix[2, 3]
        a: Int
    definitions:
        n = -(-k)
        r = recip(recip(8))
        m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
        tt = transpose(transpose(m))
        a = mod_inv(mod_inv(3, 7), 7)
    tests:
        example <round>:
            given k = 42
            expect n == 42
            expect r == 8
            expect tt == m
            expect a == 3
",
        )
        .eval_tests(p);
        let imin = admit_eval(
            p,
            "neg-imin",
            "\
emath function NegI64Min:
    inputs:
        k: Int
    outputs:
        y: Int
    definitions:
        x = -k - 1
        y = -x
    tests:
        example <overflow>:
            given k = 9223372036854775807
            expect y == 0
",
        );
        p.demand("imin-refused", imin.verdict.is_refused(), format!("-I64::MIN must named-fault, got {}", imin.verdict));
    });
    p.case("closed-forms", |p| {
        let happy = admit_eval(
            p,
            "cf-happy",
            "\
emath function Closed:
    inputs:
        n: Float64
    outputs:
        s0: Float64
        e0: Float64
        c0: Float64
        sq4: Float64
        p00: Float64
        caret00: Float64
        a00: Float64
        l2: Float64
        l10: Float64
        ln1: Float64
        log1: Float64
        log10e: Float64
        a10: Float64
        cbneg: Float64
    definitions:
        s0 = sin(n)
        e0 = exp(0)
        c0 = cos(0)
        sq4 = sqrt(4)
        p00 = pow(0, 0)
        caret00 = 0 ^ 0
        a00 = atan2(0, 0)
        l2 = log2(8)
        l10 = log10(1000)
        ln1 = ln(1)
        log1 = log(1)
        log10e = log(10)
        a10 = atan2(1, 0)
        cbneg = cbrt(-8)
    tests:
        example <closed>:
            given n = 0.0
            expect s0 == 0
            expect e0 == 1
            expect c0 == 1
            expect sq4 == 2
            expect p00 == 1
            expect caret00 == 1
            expect a00 == 0
            expect l2 == 3
            expect l10 == 3
            expect ln1 == 0
            expect log1 == 0
            expect cbneg == -2
",
        );
        p.eq("sin0", happy.outputs.get("s0"), Some(&Value::F64(0.0)));
        p.eq("pow00", happy.outputs.get("p00"), Some(&Value::F64(1.0)));
        p.eq("atan2-00", happy.outputs.get("a00"), Some(&Value::F64(0.0)));
        p.eq("cbrt-neg", happy.outputs.get("cbneg"), Some(&Value::F64(-2.0)));
        match happy.outputs.get("log10e") {
            Some(Value::F64(v)) => {
                p.close("log-is-ln", *v, 10.0_f64.ln(), 1e-12);
                p.demand("log-not-log10", (*v - 1.0).abs() > 0.5, format!("log(10) must not be 1, got {v}"));
            }
            other => { p.fail("log10e", format!("log(10) must be Float64, got {other:?}")); },
        }
        match happy.outputs.get("a10") {
            Some(Value::F64(v)) => { p.close("atan2-pi2", *v, std::f64::consts::FRAC_PI_2, 1e-12); },
            other => { p.fail("a10", format!("atan2(1,0) must be Float64, got {other:?}")); },
        }
        p.demand("closed-passed", happy.verdict.expect_passed(), format!("got {}", happy.verdict));
        let sqrt_neg = admit_eval(
            p,
            "cf-sqrt-neg",
            "\
emath function SqrtNeg:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = sqrt(-1 * n)
        finite = is_finite(y)
    tests:
        example <nan>:
            given n = 1.0
            expect finite == false
",
        );
        p.demand(
            "sqrt-nan",
            matches!(sqrt_neg.outputs.get("y"), Some(Value::F64(v)) if v.is_nan()),
            format!("sqrt(-1) must be IEEE NaN, got {:?}", sqrt_neg.outputs.get("y")),
        );
        p.eq("sqrt-not-finite", sqrt_neg.outputs.get("finite"), Some(&Value::Bool(false)));
        let ln_neg = admit_eval(
            p,
            "cf-ln-neg",
            "\
emath function LnNeg:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = ln(-1 * n)
        finite = is_finite(y)
    tests:
        example <nan>:
            given n = 1.0
            expect finite == false
",
        );
        p.demand(
            "ln-nan",
            matches!(ln_neg.outputs.get("y"), Some(Value::F64(v)) if v.is_nan()),
            format!("ln(-1) must be IEEE NaN, got {:?}", ln_neg.outputs.get("y")),
        );
        let log0 = admit_eval(
            p,
            "cf-log0",
            "\
emath function Log0:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = log(n)
        finite = is_finite(y)
    tests:
        example <ninf>:
            given n = 0.0
            expect finite == false
",
        );
        p.demand(
            "log0-ninf",
            matches!(log0.outputs.get("y"), Some(Value::F64(v)) if *v == f64::NEG_INFINITY),
            format!("log(0) must be IEEE -Inf, got {:?}", log0.outputs.get("y")),
        );
        let inv0 = admit_eval(
            p,
            "cf-modinv0",
            "\
emath function Inv0:
    inputs:
        n: Int
    outputs:
        y: Int
    definitions:
        y = mod_inv(n, 7)
    tests:
        example <noinv>:
            given n = 0
            expect y == 0
",
        );
        p.demand("modinv0-refused", inv0.verdict.is_refused(), format!("mod_inv(0,7) must named-refuse, got {}", inv0.verdict));
        let inv_m0 = admit_eval(
            p,
            "cf-modinv-m0",
            "\
emath function InvM0:
    inputs:
        m: Int
    outputs:
        y: Int
    definitions:
        y = mod_inv(3, m)
    tests:
        example <badm>:
            given m = 0
            expect y == 0
",
        );
        p.demand("modinv-m0-refused", inv_m0.verdict.is_refused(), format!("mod_inv(3,0) must named-refuse, got {}", inv_m0.verdict));
        let mod0 = admit_eval(
            p,
            "cf-mod0",
            "\
emath function Mod0:
    inputs:
        d: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = mod(1, d)
        finite = is_finite(y)
    tests:
        example <nan>:
            given d = 0.0
            expect finite == false
",
        );
        p.demand(
            "mod0-nan",
            matches!(mod0.outputs.get("y"), Some(Value::F64(v)) if v.is_nan()),
            format!("mod(1,0) must be IEEE NaN, got {:?}", mod0.outputs.get("y")),
        );
        let tan = admit_eval(
            p,
            "cf-tan",
            "\
emath function TanHalfPi:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = tan(n)
        finite = is_finite(y)
    tests:
        example <ieee>:
            given n = 1.5707963267948966
            expect finite == true
",
        );
        p.demand(
            "tan-huge",
            matches!(tan.outputs.get("y"), Some(Value::F64(v)) if v.is_finite() && v.abs() > 1e15),
            format!("tan(π/2) must be IEEE huge-finite, got {:?}", tan.outputs.get("y")),
        );
        let gf0 = admit_eval(
            p,
            "cf-gf0",
            "\
emath function Gf0:
    inputs:
        p: Int
    outputs:
        y: Int
    definitions:
        y = poly_eval_mod([1, 2], 3, p)
    tests:
        example <badp>:
            given p = 0
            expect y == 0
",
        );
        p.demand("gf0-refused", gf0.verdict.is_refused(), format!("poly_eval_mod p=0 must named-refuse, got {}", gf0.verdict));
        let gf1 = admit_eval(
            p,
            "cf-gf1",
            "\
emath function Gf1:
    inputs:
        p: Int
    outputs:
        y: Int
        inv: Int
    definitions:
        y = poly_eval_mod([5], 3, p)
        inv = mod_inv(1, p)
    tests:
        example <zeroring>:
            given p = 1
            expect y == 0
            expect inv == 0
",
        );
        p.eq("gf1-zero", gf1.outputs.get("y"), Some(&Value::I64(0)));
        p.eq("gf1-inv", gf1.outputs.get("inv"), Some(&Value::I64(0)));
        p.demand("gf1-passed", gf1.verdict.expect_passed(), format!("got {}", gf1.verdict));
    });
    p.case("binder-scope", |p| {
        for (name, body) in [
            (
                "empty-const",
                "\
emath function EmptyConst:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        a: Bool
        e: Bool
        vacuous: Bool
    definitions:
        s = sum i in 0..0: i * n
        p = product i in 0..0: i
        a = forall i in 0..0: false
        e = exists i in 0..0: true
        vacuous = forall i in 0..0: 1 / i == 0
    tests:
        example <empty>:
            given n = 1.0
            expect s == 0
            expect p == 1
            expect a == true
            expect e == false
            expect vacuous == true
",
            ),
            (
                "empty-n",
                "\
emath function EmptyN:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        a: Bool
        e: Bool
    definitions:
        s = sum i in 0..n: i
        p = product i in 0..n: i
        a = forall i in 0..n: false
        e = exists i in 0..n: true
    tests:
        example <n0>:
            given n = 0
            expect s == 0
            expect p == 1
            expect a == true
            expect e == false
",
            ),
            (
                "empty-guard",
                "\
emath function EmptyProductGuard:
    inputs:
        n: Float64
    outputs:
        p: Float64
        s: Float64
    definitions:
        p = product i in 1..5 if i > 10: i
        s = sum i in 1..5 if i > 10: i * n
    tests:
        example <id>:
            given n = 1.0
            expect p == 1
            expect s == 0
",
            ),
            (
                "capture",
                "\
emath function Capture:
    inputs:
        n: Float64
        m: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 0..n: sum j in 0..m: i
    tests:
        example <cap>:
            given n = 3
            given m = 2
            expect t == 6
",
            ),
            (
                "const-shadow",
                "\
emath function ConstShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 1..4: sum i in 10..12: i * n
    tests:
        example <cs>:
            given n = 1.0
            expect t == 63
",
            ),
            (
                "def-shadow",
                "\
emath function DefShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
        k: Float64
    definitions:
        k = 7
        t = sum k in 0..n: k
    tests:
        example <ds>:
            given n = 4
            expect t == 6
            expect k == 7
",
            ),
            (
                "input-shadow",
                "\
emath function InputShadow:
    inputs:
        i: Float64
        n: Float64
    outputs:
        t: Float64
        g: Float64
        after: Float64
    definitions:
        t = sum i in 0..n: i
        g = sum i in 0..n if i > 2: i
        after = (sum i in 1..4: i) + i
    tests:
        example <is>:
            given i = 99
            given n = 5
            expect t == 10
            expect g == 7
            expect after == 105
",
            ),
            (
                "nest-shadow",
                "\
emath function NestShadow:
    inputs:
        n: Float64
        m: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 0..n: sum i in 0..m: i
    tests:
        example <ns>:
            given n = 3
            given m = 2
            expect t == 3
",
            ),
            (
                "mixed-shadow",
                "\
emath function MixedShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 1..4: sum i in 0..n: i
    tests:
        example <ms>:
            given n = 2
            expect t == 3
",
            ),
        ] {
            Source::from_str(name, body).eval_tests(p);
        }
        // Spot-pin the shadowing arithmetic directly: input i=99 must not
        // leak into the binder, and the outer scope must restore it.
        let shadow = admit_eval(
            p,
            "shadow-pin",
            "\
emath function InputShadow:
    inputs:
        i: Float64
        n: Float64
    outputs:
        t: Float64
        g: Float64
        after: Float64
    definitions:
        t = sum i in 0..n: i
        g = sum i in 0..n if i > 2: i
        after = (sum i in 1..4: i) + i
    tests:
        example <is>:
            given i = 99
            given n = 5
            expect t == 10
            expect g == 7
            expect after == 105
",
        );
        p.eq("shadow-sum", shadow.outputs.get("t"), Some(&Value::F64(10.0)));
        p.eq("shadow-guard", shadow.outputs.get("g"), Some(&Value::F64(7.0)));
        p.eq("shadow-restore", shadow.outputs.get("after"), Some(&Value::F64(105.0)));
    });
    p.finish();
}
