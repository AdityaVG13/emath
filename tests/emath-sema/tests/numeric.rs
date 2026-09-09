//! Numeric-model admission, unit/shape/domain refusals, and e2e corpus.

use emath_ir::NumericProfile;
use emath_test_harness::{boot, Probe, Source, error_codes};

fn function_with_compile(compile: &str, extra_inputs: &str, definitions: &str) -> String {
    format!(
        "emath function Timed:\n    inputs:\n        t: Duration\n        {extra_inputs}\n    outputs:\n        y: Float64\n    definitions:\n        {definitions}\n    compile:\n        target rust\n        profile library\n        {compile}\n"
    )
}

const CACHE_POLICY: &str = r#"use core::math::{Real, Probability, NonNegative, exp}
use core::units::{Duration, Bytes, MiB}
use host::cache_core::{CacheCandidate, Policy}

emath policy AdaptiveCachePolicy:
    about:
        summary: "Dimension-safe cache scoring policy with generated derivatives and host adapter."

    inputs:
        candidate: CacheCandidate

    outputs:
        score: Float64

    state:
        alpha: NonNegative<Real>
        gamma: NonNegative<Per<Duration>>
        memory_penalty: NonNegative<Real>

    constructors:
        public fn new(
            alpha: Real,
            gamma: Per<Duration>,
            memory_penalty: Real,
        ) -> Result<Self, ConfigError>:
            require alpha >= 0
            require gamma >= 0 / s
            require memory_penalty >= 0
            Self:
                alpha = alpha
                gamma = gamma
                memory_penalty = memory_penalty

    definitions:
        score =
            candidate.reuse_probability^state.alpha
            * candidate.rebuild_cost / 1 ms
            * exp(-(state.gamma * candidate.age))
            / (1 + state.memory_penalty * candidate.bytes / 1 MiB)

    goals:
        evaluate <score>:
            produce rust.library

        differentiate <score>:
            wrt [state.alpha, state.gamma, state.memory_penalty]
            order 1

        benchmark <score>:
            against host::LruPolicy::score
            measure [latency, hit_rate, bytes_retained, token_cost]

    evidence:
        claim <finite_score>:
            statement is_finite(score)
            require guarded

        claim <nonnegative_score>:
            statement score >= 0
            require bounded

    compile:
        target rust
        representation Real => Float64(round = nearest, overflow = error)
        unresolved parametric

    exports:
        public type AdaptiveCachePolicy
        public function score
        public function gradient_score

    host:
        rust:
            implement cache_core::Policy for AdaptiveCachePolicy:
                method score(candidate: &CacheCandidate) -> f64:
                    evaluate score with candidate = candidate
"#;

#[test]
fn numeric_models_units_shapes_and_si_corpus() {
    boot();
    let mut p = Probe::new("numeric models admit, unit/shape/domain refusals are typed, SI corpus evaluates exact");
    p.case("models", |p| {
        let admitted = Source::from_str(
            "default-numeric",
            "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n",
        )
        .must_admit(p);
        p.eq(
            "default-strict",
            admitted.package.declarations[0].compile_spec.numeric.clone(),
            NumericProfile::StrictF64,
        );
        let admitted = Source::from_str(
            "interval-model",
            function_with_compile(
                "numeric interval-f64\n        precision 53\n        error-limit 1e-12",
                "",
                "y = t / 1 s",
            ),
        )
        .must_admit(p);
        p.eq(
            "interval-honored",
            admitted.package.declarations[0].compile_spec.numeric.clone(),
            NumericProfile::IntervalF64,
        );
        Source::from_str(
            "unknown-model",
            function_with_compile("numeric float128", "", "y = t / 1 s"),
        )
        .must_refuse(p, &["E-NUM-001"]);
        Source::from_str(
            "precision",
            function_with_compile(
                "numeric strict-f64\n        precision 128",
                "",
                "y = t / 1 s",
            ),
        )
        .must_refuse(p, &["E-NUM-002"]);
        Source::from_str(
            "error-limit",
            function_with_compile(
                "numeric strict-f64\n        error-limit 1e-20",
                "",
                "y = t / 1 s",
            ),
        )
        .must_refuse(p, &["E-NUM-003"]);
        Source::from_str(
            "representation",
            function_with_compile("representation Real", "", "y = t / 1 s"),
        )
        .must_refuse(p, &["E-NUM-004"]);
        Source::from_str(
            "e2e-neg",
            "emath function CacheLike:\n    inputs:\n        age: Duration\n    outputs:\n        y: Float64\n    definitions:\n        y = age / 1 s\n    compile:\n        numeric float128\n",
        )
        .must_refuse(p, &["E-NUM-001"]);
    });
    p.case("units-refuse", |p| {
        Source::from_str(
            "furlong",
            function_with_compile("numeric strict-f64", "", "y = t / 1 furlong"),
        )
        .must_refuse(p, &["E-UNIT-104"]);
        Source::from_str(
            "mismatch",
            function_with_compile("numeric strict-f64", "bytes: MiB", "y = t + bytes"),
        )
        .must_refuse(p, &["E-UNIT-101"]);
        Source::from_str(
            "per",
            "emath function BadPer:\n    inputs:\n        rate: Per\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-UNIT-105"]);
        Source::from_str(
            "m-plus-s",
            function_with_compile("numeric strict-f64", "", "y = 1 m + 1 s"),
        )
        .must_refuse(p, &["E-UNIT-101"]);
        Source::from_str(
            "one-plus-mib",
            function_with_compile("numeric strict-f64", "", "y = 1 + 1 MiB"),
        )
        .must_refuse(p, &["E-UNIT-101"]);
        Source::from_str(
            "c-plus-c",
            function_with_compile("numeric strict-f64", "", "y = 1 degC + 1 degC"),
        )
        .must_refuse(p, &["E-UNIT-102"]);
        Source::from_str(
            "c-times-2",
            function_with_compile("numeric strict-f64", "", "y = (1 degC) * 2"),
        )
        .must_refuse(p, &["E-UNIT-102"]);
        Source::from_str(
            "unit-of",
            "emath function Q:\n    inputs:\n        x: Float64 in m\n    outputs:\n        y: Float64\n    definitions:\n        y = unit of x\n",
        )
        .must_refuse(p, &["E-TYPE-010"]);
        for (name, src) in [
            (
                "len-from-dur",
                "emath function Bad:\n    outputs:\n        y: Float64 in m\n    definitions:\n        y = 1 s\n",
            ),
            (
                "mib-from-f64",
                "emath function Bad:\n    outputs:\n        y: MiB\n    definitions:\n        y = 1.0\n",
            ),
        ] {
            let result = Source::from_str(name, src).check();
            let codes = error_codes(&result.diagnostics);
            p.demand(
                format!("{name}-dimensioned"),
                codes.iter().any(|code| *code == "E-TYPE-012" || *code == "E-UNIT-101"),
                format!("dimensioned output fed the wrong dimension must refuse, got {codes:?}"),
            );
        }
    });
    p.case("compound-units", |p| {
        for (name, definition) in [
            ("compound-accel", "y = 9.81 [unit m/s^2]"),
            ("c2-trap", "y = 1.0 [unit m/s*s]"),
            ("compound-paren", "y = 9.81 [unit m/(s*s)]"),
            ("compound-energy", "y = 100.0 [unit kg*m^2/s^2]"),
        ] {
            let result =
                Source::from_str(name, function_with_compile("numeric strict-f64", "", definition))
                    .check();
            let codes = error_codes(&result.diagnostics);
            p.demand(
                format!("{name}-known"),
                codes.contains(&"E-UNIT-104") == false,
                format!("known units must not produce E-UNIT-104, got {codes:?}"),
            );
        }
        Source::from_str(
            "compound-unknown",
            function_with_compile("numeric strict-f64", "", "y = 1.0 [unit m/furlong]"),
        )
        .must_refuse(p, &["E-UNIT-104"]);
        Source::from_str(
            "area-m-star-m",
            "emath function Area:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64 in m*m\n    definitions:\n        y = 1 m * 1 m\n",
        )
        .must_admit(p);
        Source::from_str(
            "area-m-squared",
            "emath function Area:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64 in m^2\n    definitions:\n        y = 1 m * 1 m\n",
        )
        .must_admit(p);
        Source::from_str(
            "type-c2",
            "emath function C2:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64 in m/s*s\n    definitions:\n        y = 1 m\n",
        )
        .must_admit(p);
    });
    p.case("shapes", |p| {
        Source::from_str(
            "tensor",
            "emath function BadTensor:\n    inputs:\n        x: Tensor<Float64, []>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-SHAPE-004"]);
        Source::from_str(
            "vec-arity",
            "emath function F:\n    inputs:\n        v: Vector[2, 3]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-SHAPE-004"]);
        Source::from_str(
            "mat-arity",
            "emath function F:\n    inputs:\n        m: Matrix[2]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-SHAPE-004"]);
        Source::from_str(
            "vec-zero",
            "emath function F:\n    inputs:\n        v: Vector[0]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-SHAPE-004"]);
        Source::from_str(
            "mat-zero",
            "emath function F:\n    inputs:\n        m: Matrix[0, 3]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_refuse(p, &["E-SHAPE-004"]);
        Source::from_str(
            "tensor-c10",
            "emath function F:\n    inputs:\n        t: Tensor<Float64, [2, 2, 2]>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_admit(p);
        Source::from_str(
            "vec-int",
            "emath function F:\n    inputs:\n        v: Vector<Int, 3>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_admit(p);
    });
    p.case("domains", |p| {
        Source::from_str(
            "domain",
            function_with_compile(
                "numeric strict-f64\n        domain 5..1",
                "",
                "y = t / 1 s",
            ),
        )
        .must_refuse(p, &["E-DOM-002"]);
        Source::from_str(
            "domain-input",
            "emath function f(x: Float64 in [0.0, 1.0]) -> Float64:\n    definitions:\n        f = x * x\n",
        )
        .must_admit(p);
        Source::from_str(
            "domain-non-numeric",
            "emath function f(x: Bool in [0.0, 1.0]) -> Float64:\n    definitions:\n        f = 1.0\n",
        )
        .must_refuse(p, &["E-TYPE-001"]);
    });
    p.case("claims", |p| {
        Source::from_str(
            "limit-claim",
            "emath function f(x: Float64) -> Float64:\n    definitions:\n        f = x * x\n    invariant:\n        limit x -> 0: sin(x) / x == 1\n",
        )
        .must_admit(p);
        Source::from_str(
            "limit-plus-claim",
            "emath function f(x: Float64) -> Float64:\n    definitions:\n        f = x * x\n    invariant:\n        limit x -> 0+: 1 / x > 0\n",
        )
        .must_admit(p);
        Source::from_str(
            "series-claim",
            "emath function f(n: Nat) -> Float64:\n    definitions:\n        f = 1 / (n + 1)\n    invariant:\n        series k in 0..100: 1 / (k + 1) < 10\n",
        )
        .must_admit(p);
        Source::from_str(
            "asymp-claim",
            "emath function f(n: Float64) -> Float64:\n    definitions:\n        f = n * n\n    invariant:\n        n * n ~~ n ^ 2.0\n",
        )
        .must_admit(p);
        let result = Source::from_str(
            "limit-in-defs",
            "emath function f(x: Float64) -> Float64:\n    definitions:\n        f = limit x -> 0: sin(x) / x\n",
        )
        .check();
        p.demand(
            "limit-in-defs-refused",
            result.diagnostics.has_errors(),
            "limit in definitions must error (a claim is not a computation)",
        );
    });
    p.case("surface", |p| {
        Source::from_str(
            "grad-admit",
            "emath function f(x: Float64, y: Float64) -> Vector[2]:\n    definitions:\n        f = grad(x * y + y * y)\n",
        )
        .must_admit(p);
        let result = Source::from_str(
            "grad-non-scalar",
            "emath function f(x: Float64, y: Float64) -> Vector[2]:\n    definitions:\n        v = [x, y]\n        f = grad(v)\n",
        )
        .check();
        p.demand(
            "grad-non-scalar-refused",
            result.diagnostics.has_errors(),
            "grad() on a non-scalar expression must error",
        );
        Source::from_str(
            "cases-admit",
            "emath function f(x: Float64) -> Float64:\n    definitions:\n        f = cases x:\n            | x > 0.0 => 1.0\n            | x < 0.0 => -1.0\n            | else => 0.0\n",
        )
        .must_admit(p);
        Source::from_str(
            "result-field",
            "emath function F:\n    inputs:\n        x: Result<Float64, Float64>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_admit(p);
        Source::from_str(
            "graph-field",
            "emath function F:\n    inputs:\n        g: Graph\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_admit(p);
        Source::from_str(
            "rat-field",
            "emath function F:\n    inputs:\n        q: Rat\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
        )
        .must_admit(p);
        Source::from_str(
            "ctor-result",
            "emath policy Affine:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    state:\n        s: Float64\n    constructors:\n        public fn new(s: Float64) -> Result<Self, ConfigError>:\n            require s >= 0\n            Self:\n                s = s\n    definitions:\n        y = state.s * x\n",
        )
        .must_admit(p);
    });
    p.case("e2e-admit", |p| {
        let admitted = Source::from_str(
            "e2e-units",
            "emath function CacheLike:\n    inputs:\n        age: Duration\n        bytes: MiB\n        rate: Per<Duration>\n    outputs:\n        y: Float64\n    definitions:\n        y = age / 1 s * bytes / 1 MiB * rate * 1 s\n    compile:\n        target rust\n        numeric interval-f64\n        precision 53\n        error-limit 1e-9\n        representation Real => Interval\n",
        )
        .must_admit(p);
        p.eq(
            "e2e-interval",
            admitted.package.declarations[0].compile_spec.numeric.clone(),
            NumericProfile::IntervalF64,
        );
        let result = Source::from_str("cache-policy", CACHE_POLICY).check();
        let messages: Vec<String> = result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.to_string())
            .collect();
        p.demand(
            "no-phase5-refusal",
            messages.iter().any(|message| message.contains("unit system arrives in Phase 5")) == false,
            format!("Duration/MiB must not be refused as a Phase 5 absence, got {messages:?}"),
        );
    });
    p.case("si-eval", |p| {
        Source::from_str(
            "km-plus-m",
            "emath function Rescale:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (1 km + n * 1 m) / 1 m\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 1001\n",
        )
        .eval_tests(p);
        Source::from_str(
            "ms-over-s",
            "emath function Ms:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (n * 1 ms) / (1 s)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 0.001\n",
        )
        .eval_tests(p);
        Source::from_str(
            "mib-over-b",
            "emath function Info:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (n * 1 MiB) / (1 B)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 1048576\n",
        )
        .eval_tests(p);
        Source::from_str(
            "rational-s",
            "emath function RatQ:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (n * (3//2 s)) / (1 s)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 1.5\n",
        )
        .eval_tests(p);
        Source::from_str(
            "m-times-m",
            "emath function Area:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (n * 1 m * 1 m) / (1 [unit m^2])\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 1\n",
        )
        .eval_tests(p);
        Source::from_str(
            "m-over-m",
            "emath function Cancel:\n    inputs:\n        n: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = (n * 1 m) / (1 m)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == 1\n",
        )
        .eval_tests(p);
        Source::from_str(
            "zero-c",
            "emath function Temp:\n    inputs:\n        n: Float64\n    outputs:\n        y: Bool\n    definitions:\n        y = (0 degC == n * 273.15 K)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == true\n",
        )
        .eval_tests(p);
        Source::from_str(
            "c-plus-k",
            "emath function Shift:\n    inputs:\n        n: Float64\n    outputs:\n        y: Bool\n    definitions:\n        y = (0 degC + n * 1 K == 1 degC)\n    tests:\n        example <si>:\n            given n = 1.0\n            expect y == true\n",
        )
        .eval_tests(p);
        Source::from_str(
            "fahrenheit-c13",
            "emath function Fahrenheit:\n    inputs:\n        n: Float64\n    outputs:\n        freezing: Bool\n        boiling: Bool\n    definitions:\n        freezing = (32 degF == n * 273.15 K)\n        boiling = (212 degF == n * 373.15 K)\n    tests:\n        example <c13>:\n            given n = 1.0\n            expect freezing == true\n            expect boiling == true\n",
        )
        .eval_tests(p);
        Source::from_str(
            "temperature-difference",
            "emath function TemperatureDifference:\n    inputs:\n        n: Float64\n    outputs:\n        delta: Float64\n    definitions:\n        delta = (22 degC - 10 degC) / (n * 1 K)\n    tests:\n        example <difference>:\n            given n = 1.0\n            expect delta == 12\n",
        )
        .eval_tests(p);
        Source::from_str(
            "litre-alias",
            "emath function LitreAlias:\n    inputs:\n        n: Float64\n    outputs:\n        american: Float64\n        british: Float64\n    definitions:\n        american = (n * 1 liter) / (1 L)\n        british = (1 litre) / (1 L)\n    tests:\n        example <aliases>:\n            given n = 1.0\n            expect american == 1\n            expect british == 1\n",
        )
        .eval_tests(p);
        Source::from_workspace("language/examples/intro/units.emath").eval_tests(p);
    });
    p.case("diagnostic-prose", |p| {
        let result = Source::from_str(
            "dur-to-len",
            "emath function Bad:\n    outputs:\n        y: Float64 in m\n    definitions:\n        y = 1 s\n",
        )
        .check();
        let messages: Vec<String> = result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.to_string())
            .collect();
        p.demand(
            "names-dimensions",
            messages.iter().any(|message| {
                message.contains("E-TYPE-012")
                    && message.contains("duration")
                    && message.contains("length")
                    && !message.contains("Infer::Unit")
            }),
            format!("duration vs length must be named, not Debug-dumped, got {messages:?}"),
        );
    });
    p.finish();
}
