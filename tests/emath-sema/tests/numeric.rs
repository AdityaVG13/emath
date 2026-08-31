//! Numeric-model admission, unit/shape/domain refusals, and e2e corpus.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_ir::NumericProfile;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn errors_of(name: &str, source: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

fn function_with_compile(compile: &str, extra_inputs: &str, definitions: &str) -> String {
    format!(
        "\
emath function Timed:
    inputs:
        t: Duration
        {extra_inputs}
    outputs:
        y: Float64
    definitions:
        {definitions}
    compile:
        target rust
        profile library
        {compile}
"
    )
}

#[test]
fn omitted_numeric_defaults_to_strict_f64() {
    let source = "\
emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("default-numeric", source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(codes.is_empty(), "bare function must admit, got {codes:?}");
    assert_eq!(
        result.package.declarations[0].compile_spec.numeric,
        NumericProfile::StrictF64
    );
}

#[test]
fn explicit_interval_model_is_honored() {
    let source = function_with_compile(
        "numeric interval-f64\n        precision 53\n        error-limit 1e-12",
        "",
        "y = t / 1 s",
    );
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("interval-model", &source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "units + interval-f64 must admit, got {codes:?}"
    );
    assert_eq!(
        result.package.declarations[0].compile_spec.numeric,
        NumericProfile::IntervalF64
    );
}

#[test]
fn unknown_numeric_model_is_e_num_001() {
    let source = function_with_compile("numeric float128", "", "y = t / 1 s");
    let codes = errors_of("unknown-model", &source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-001"),
        "unknown model must be E-NUM-001, got {codes:?}"
    );
}

#[test]
fn precision_demand_no_model_can_honor_is_e_num_002() {
    let source = function_with_compile(
        "numeric strict-f64\n        precision 128",
        "",
        "y = t / 1 s",
    );
    let codes = errors_of("precision", &source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-002"),
        "precision 128 must be E-NUM-002, got {codes:?}"
    );
}

#[test]
fn error_limit_no_model_can_honor_is_e_num_003() {
    let source = function_with_compile(
        "numeric strict-f64\n        error-limit 1e-20",
        "",
        "y = t / 1 s",
    );
    let codes = errors_of("error-limit", &source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-003"),
        "tiny error-limit must be E-NUM-003, got {codes:?}"
    );
}

#[test]
fn representation_real_without_model_is_e_num_004() {
    let source = function_with_compile("representation Real", "", "y = t / 1 s");
    let codes = errors_of("representation", &source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-004"),
        "bare representation Real must be E-NUM-004, got {codes:?}"
    );
}

#[test]
fn unknown_quantity_unit_is_e_unit_104() {
    let source = function_with_compile("numeric strict-f64", "", "y = t / 1 furlong");
    let codes = errors_of("furlong", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-104"),
        "unknown unit must be E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn dimension_mismatch_is_e_unit_101() {
    let source = function_with_compile("numeric strict-f64", "bytes: MiB", "y = t + bytes");
    let codes = errors_of("mismatch", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-101"),
        "Duration + MiB must be E-UNIT-101, got {codes:?}"
    );
}

#[test]
fn ill_formed_per_is_e_unit_105() {
    let source = "\
emath function BadPer:
    inputs:
        rate: Per
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("per", source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-105"),
        "Per without inner unit must be E-UNIT-105, got {codes:?}"
    );
}

#[test]
fn empty_tensor_shape_is_e_shape_004() {
    let source = "\
emath function BadTensor:
    inputs:
        x: Tensor<Float64, []>
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("tensor", source);
    assert!(
        codes.iter().any(|code| code == "E-SHAPE-004"),
        "empty tensor shape must be E-SHAPE-004, got {codes:?}"
    );
}

#[test]
fn inverted_domain_is_e_dom_002() {
    let source =
        function_with_compile("numeric strict-f64\n        domain 5..1", "", "y = t / 1 s");
    let codes = errors_of("domain", &source);
    assert!(
        codes.iter().any(|code| code == "E-DOM-002"),
        "inverted domain must be E-DOM-002, got {codes:?}"
    );
}

#[test]
fn units_plus_explicit_model_e2e_admits() {
    let source = "\
emath function CacheLike:
    inputs:
        age: Duration
        bytes: MiB
        rate: Per<Duration>
    outputs:
        y: Float64
    definitions:
        y = age / 1 s * bytes / 1 MiB * rate * 1 s
    compile:
        target rust
        numeric interval-f64
        precision 53
        error-limit 1e-9
        representation Real => Interval
";
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("e2e-units", source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "units + explicit model corpus must admit, got {codes:?}"
    );
    assert_eq!(
        result.package.declarations[0].compile_spec.numeric,
        NumericProfile::IntervalF64
    );
}

#[test]
fn cache_policy_example_no_longer_refuses_units_as_absent() {
    let source = r#"
use core::math::{Real, Probability, NonNegative, exp}
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
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("cache-policy", source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        !result.diagnostics.errors().any(|diagnostic| diagnostic
            .message
            .contains("unit system arrives in Phase 5")),
        "Duration/MiB must not be refused as a Phase 5 absence, got {codes:?}"
    );
}

#[test]
fn matching_negative_refuses_unknown_model() {
    let source = "\
emath function CacheLike:
    inputs:
        age: Duration
    outputs:
        y: Float64
    definitions:
        y = age / 1 s
    compile:
        numeric float128
";
    let codes = errors_of("e2e-neg", source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-001"),
        "matching negative must refuse with E-NUM-001, got {codes:?}"
    );
}

// ─── B04+B06+B18: claims in invariant (limit, series, asymp) ───

#[test]
fn limit_claim_admitted_in_invariant() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = x * x
    invariant:
        limit x -> 0: sin(x) / x == 1
";
    let codes = errors_of("limit-claim", source);
    assert!(
        codes.is_empty(),
        "limit claim in invariant should be admitted, got errors: {codes:?}"
    );
}

#[test]
fn one_sided_limit_claim_admitted_in_invariant() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = x * x
    invariant:
        limit x -> 0+: 1 / x > 0
";
    let codes = errors_of("limit-plus-claim", source);
    assert!(
        codes.is_empty(),
        "one-sided limit claim in invariant should be admitted, got errors: {codes:?}"
    );
}

#[test]
fn series_claim_admitted_in_invariant() {
    let source = "\
emath function f(n: Nat) -> Float64:
    definitions:
        f = 1 / (n + 1)
    invariant:
        series k in 0..100: 1 / (k + 1) < 10
";
    let codes = errors_of("series-claim", source);
    assert!(
        codes.is_empty(),
        "series claim in invariant should be admitted, got errors: {codes:?}"
    );
}

#[test]
fn asymp_claim_admitted_in_invariant() {
    let source = "\
emath function f(n: Float64) -> Float64:
    definitions:
        f = n * n
    invariant:
        n * n ~~ n ^ 2.0
";
    let codes = errors_of("asymp-claim", source);
    assert!(
        codes.is_empty(),
        "asymptotic equivalence claim in invariant should be admitted, got errors: {codes:?}"
    );
}

#[test]
fn limit_in_definitions_still_errors() {
    // limit in definitions (computation context) must still error —
    // it's a claim, not a computation. Use sample_limit instead.
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = limit x -> 0: sin(x) / x
";
    let codes = errors_of("limit-in-defs", source);
    assert!(
        !codes.is_empty(),
        "limit in definitions must error (it's a claim, not a computation)"
    );
}

// ─── reverse-mode AD ───

#[test]
fn grad_admits_in_definitions() {
    let source = "\
emath function f(x: Float64, y: Float64) -> Vector[2]:
    definitions:
        f = grad(x * y + y * y)
";
    let codes = errors_of("grad-admit", source);
    assert!(
        codes.is_empty(),
        "grad() should be admitted in definitions, got errors: {codes:?}"
    );
}

#[test]
fn grad_requires_scalar_expression() {
    // grad() on a vector expression should error.
    let source = "\
emath function f(x: Float64, y: Float64) -> Vector[2]:
    definitions:
        v = [x, y]
        f = grad(v)
";
    let codes = errors_of("grad-non-scalar", source);
    assert!(
        !codes.is_empty(),
        "grad() on a non-scalar expression must error"
    );
}

// ─── cases expression (U1) ───

#[test]
fn cases_admits_and_computes() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0.0 => 1.0
            | x < 0.0 => -1.0
            | else => 0.0
";
    let codes = errors_of("cases-admit", source);
    assert!(
        codes.is_empty(),
        "cases expression should be admitted in definitions, got errors: {codes:?}"
    );
}

// ─── domain declarations on inputs (U5) ───

#[test]
fn domain_annotated_input_admits() {
    let source = "\
emath function f(x: Float64 in [0.0, 1.0]) -> Float64:
    definitions:
        f = x * x
";
    let codes = errors_of("domain-input", source);
    assert!(
        codes.is_empty(),
        "domain-annotated input should be admitted, got errors: {codes:?}"
    );
}

#[test]
fn domain_on_non_numeric_type_errors() {
    let source = "\
emath function f(x: Bool in [0.0, 1.0]) -> Float64:
    definitions:
        f = 1.0
";
    let codes = errors_of("domain-non-numeric", source);
    assert!(
        codes.iter().any(|code| code == "E-TYPE-001"),
        "domain annotation on Bool must be E-TYPE-001, got {codes:?}"
    );
}

#[test]
fn compound_unit_acceleration_admits() {
    // `9.81 [unit m/s^2]` should parse and lower without unit errors.
    // E-TYPE-012 is expected (unit type vs Float64 output), but
    // E-UNIT-104 (unknown unit) must not appear.
    let source = function_with_compile("numeric strict-f64", "", "y = 9.81 [unit m/s^2]");
    let codes = errors_of("compound-accel", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in m/s^2 must not produce E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn compound_unit_c2_trap_admits_as_length() {
    // `1.0 [unit m/s*s]` is left-assoc: ((m/s)*s) = dimension length.
    // Should parse without unit errors (known units).
    let source = function_with_compile("numeric strict-f64", "", "y = 1.0 [unit m/s*s]");
    let codes = errors_of("c2-trap", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in m/s*s must not produce E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn compound_unit_parenthesized_admits() {
    // `9.81 [unit m/(s*s)]` — parenthesized denominator.
    // Should parse without unit errors.
    let source = function_with_compile("numeric strict-f64", "", "y = 9.81 [unit m/(s*s)]");
    let codes = errors_of("compound-paren", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in m/(s*s) must not produce E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn compound_unit_with_unknown_unit_errors() {
    // `1.0 [unit m/furlong]` — furlong is not a known unit.
    let source = function_with_compile("numeric strict-f64", "", "y = 1.0 [unit m/furlong]");
    let codes = errors_of("compound-unknown", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-104"),
        "unknown unit in compound expression must be E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn compound_unit_kg_m2_s2_admits() {
    // `100.0 [unit kg*m^2/s^2]` — energy (joules).
    // Should parse without unit errors.
    let source = function_with_compile("numeric strict-f64", "", "y = 100.0 [unit kg*m^2/s^2]");
    let codes = errors_of("compound-energy", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in kg*m^2/s^2 must not produce E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn result_as_input_is_e_type_010() {
    let source = "\
emath function F:
    inputs:
        x: Result<Float64, ConfigError>
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("result-field", source);
    assert!(
        codes.iter().any(|code| code == "E-TYPE-010"),
        "Result as a compute type must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn graph_and_rat_as_inputs_are_e_type_010() {
    let graph = "\
emath function F:
    inputs:
        g: Graph
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let rat = "\
emath function F:
    inputs:
        q: Rat
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let graph_codes = errors_of("graph-field", graph);
    let rat_codes = errors_of("rat-field", rat);
    assert!(
        graph_codes.iter().any(|code| code == "E-TYPE-010"),
        "Graph must be E-TYPE-010, got {graph_codes:?}"
    );
    assert!(
        rat_codes.iter().any(|code| code == "E-TYPE-010"),
        "Rat must be E-TYPE-010, got {rat_codes:?}"
    );
}

#[test]
fn vector_extra_extent_is_e_shape_004() {
    let source = "\
emath function F:
    inputs:
        v: Vector[2, 3]
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("vec-arity", source);
    assert!(
        codes.iter().any(|code| code == "E-SHAPE-004"),
        "Vector[2, 3] must be E-SHAPE-004, got {codes:?}"
    );
}

#[test]
fn matrix_one_extent_is_e_shape_004() {
    let source = "\
emath function F:
    inputs:
        m: Matrix[2]
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("mat-arity", source);
    assert!(
        codes.iter().any(|code| code == "E-SHAPE-004"),
        "Matrix[2] must be E-SHAPE-004, got {codes:?}"
    );
}

#[test]
fn vector_zero_extent_is_e_shape_004() {
    let source = "\
emath function F:
    inputs:
        v: Vector[0]
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("vec-zero", source);
    assert!(
        codes.iter().any(|code| code == "E-SHAPE-004"),
        "Vector[0] must be E-SHAPE-004, got {codes:?}"
    );
}

#[test]
fn matrix_zero_extent_is_e_shape_004() {
    let source = "\
emath function F:
    inputs:
        m: Matrix[0, 3]
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("mat-zero", source);
    assert!(
        codes.iter().any(|code| code == "E-SHAPE-004"),
        "Matrix[0, 3] must be E-SHAPE-004, got {codes:?}"
    );
}

#[test]
fn tensor_bracket_list_extent_admits() {
    let source = "\
emath function F:
    inputs:
        t: Tensor<Float64, [2, 2, 2]>
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("tensor-c10", source);
    assert!(
        codes.is_empty(),
        "Tensor<Float64, [2, 2, 2]> must admit, got {codes:?}"
    );
}

#[test]
fn vector_int_element_admits() {
    let source = "\
emath function F:
    inputs:
        v: Vector<Int, 3>
    outputs:
        y: Float64
    definitions:
        y = 1
";
    let codes = errors_of("vec-int", source);
    assert!(
        codes.is_empty(),
        "Vector<Int, 3> must admit Int as the element type, got {codes:?}"
    );
}

#[test]
fn constructor_result_return_still_admits() {
    let source = "\
emath policy Affine:
    inputs:
        x: Float64
    outputs:
        y: Float64
    state:
        s: Float64
    constructors:
        public fn new(s: Float64) -> Result<Self, ConfigError>:
            require s >= 0
            Self:
                s = s
    definitions:
        y = state.s * x
";
    let codes = errors_of("ctor-result", source);
    assert!(
        codes.is_empty(),
        "constructor `-> Result<Self, ConfigError>` must still admit, got {codes:?}"
    );
}

fn eval_output_f64(name: &str, source: &str, output: &str) -> f64 {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "{name} must admit, got {codes:?}: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    match test.outputs.get(output) {
        Some(Value::F64(value)) => *value,
        other => panic!(
            "{name}: expected F64 `{output}`, got {other:?} (verdict {})",
            test.verdict
        ),
    }
}

#[test]
fn kilometre_plus_metre_rescales_to_si() {
    // Unit-rescaling invariance: `1 km + 1 m` is 1001 m, not 2.
    let source = "\
emath function Rescale:
    outputs:
        y: Float64
    definitions:
        y = (1 km + 1 m) / 1 m
    tests:
        example <si>:
            expect y == 1001
";
    let y = eval_output_f64("km-plus-m", source, "y");
    assert_eq!(y, 1001.0, "1 km + 1 m must be 1001 m, got {y}");
}

#[test]
fn millisecond_scale_is_applied() {
    let source = "\
emath function Ms:
    outputs:
        y: Float64
    definitions:
        y = (1 ms) / (1 s)
    tests:
        example <si>:
            expect y == 0.001
";
    let y = eval_output_f64("ms-over-s", source, "y");
    assert_eq!(y, 0.001, "1 ms / 1 s must be 0.001, got {y}");
}

#[test]
fn mib_over_byte_rescales() {
    let source = "\
emath function Info:
    outputs:
        y: Float64
    definitions:
        y = (1 MiB) / (1 B)
    tests:
        example <si>:
            expect y == 1048576
";
    let y = eval_output_f64("mib-over-b", source, "y");
    assert_eq!(y, 1_048_576.0, "1 MiB / 1 B must be 1048576, got {y}");
}

#[test]
fn rational_quantity_evaluates_as_si() {
    // Pass 2 parse: `3//2 s` is a quantity. Under strict-f64 it is 1.5 s.
    let source = "\
emath function RatQ:
    outputs:
        y: Float64
    definitions:
        y = (3//2 s) / (1 s)
    tests:
        example <si>:
            expect y == 1.5
";
    let y = eval_output_f64("rational-s", source, "y");
    assert_eq!(y, 1.5, "3//2 s / 1 s must be 1.5, got {y}");
}

#[test]
fn metre_plus_second_is_e_unit_101() {
    let source = function_with_compile("numeric strict-f64", "", "y = 1 m + 1 s");
    let codes = errors_of("m-plus-s", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-101"),
        "1 m + 1 s must be E-UNIT-101, got {codes:?}"
    );
}

#[test]
fn mib_plus_dimensionless_is_e_unit_101() {
    let source = function_with_compile("numeric strict-f64", "", "y = 1 + 1 MiB");
    let codes = errors_of("one-plus-mib", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-101"),
        "1 + 1 MiB must be E-UNIT-101, got {codes:?}"
    );
}

#[test]
fn length_output_rejects_duration_value() {
    let source = "\
emath function Bad:
    outputs:
        y: Float64 in m
    definitions:
        y = 1 s
";
    let codes = errors_of("len-from-dur", source);
    assert!(
        codes
            .iter()
            .any(|code| code == "E-TYPE-012" || code == "E-UNIT-101"),
        "assigning Duration to Length must refuse, got {codes:?}"
    );
}

#[test]
fn mib_output_rejects_dimensionless() {
    let source = "\
emath function Bad:
    outputs:
        y: MiB
    definitions:
        y = 1.0
";
    let codes = errors_of("mib-from-f64", source);
    assert!(
        codes
            .iter()
            .any(|code| code == "E-TYPE-012" || code == "E-UNIT-101"),
        "assigning dimensionless to MiB must refuse, got {codes:?}"
    );
}

#[test]
fn unit_of_is_named_refuse() {
    let source = "\
emath function Q:
    inputs:
        x: Float64 in m
    outputs:
        y: Float64
    definitions:
        y = unit of x
";
    let codes = errors_of("unit-of", source);
    assert!(
        codes.iter().any(|code| code == "E-TYPE-010"),
        "`unit of` must be a named refuse, got {codes:?}"
    );
}

fn error_messages(name: &str, source: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.to_string())
        .collect()
}

fn eval_output_bool(name: &str, source: &str, output: &str) -> bool {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    let codes: Vec<&str> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(
        codes.is_empty(),
        "{name} must admit, got {codes:?}: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    match test.outputs.get(output) {
        Some(Value::Bool(value)) => *value,
        other => panic!(
            "{name}: expected Bool `{output}`, got {other:?} (verdict {})",
            test.verdict
        ),
    }
}

#[test]
fn metre_times_metre_is_area() {
    let source = "\
emath function Area:
    outputs:
        y: Float64
    definitions:
        y = (1 m * 1 m) / (1 [unit m^2])
    tests:
        example <si>:
            expect y == 1
";
    let y = eval_output_f64("m-times-m", source, "y");
    assert_eq!(y, 1.0, "1 m * 1 m must be 1 m^2, got {y}");
}

#[test]
fn metre_times_metre_admits_as_m_star_m() {
    let source = "\
emath function Area:
    outputs:
        y: Float64 in m*m
    definitions:
        y = 1 m * 1 m
";
    let codes = errors_of("area-ann", source);
    assert!(
        codes.is_empty(),
        "`Float64 in m*m` must match 1 m * 1 m, got {codes:?}"
    );
}

#[test]
fn metre_times_metre_admits_as_m_squared() {
    let source = "\
emath function Area:
    outputs:
        y: Float64 in m^2
    definitions:
        y = 1 m * 1 m
";
    let codes = errors_of("area-pow", source);
    assert!(
        codes.is_empty(),
        "`Float64 in m^2` must match 1 m * 1 m, got {codes:?}"
    );
}

#[test]
fn units_example_computes() {
    let source = include_str!("../../../language/examples/intro/units.emath");
    let rescale = eval_output_f64("units-ex-rescale", source, "rescale");
    let area = eval_output_f64("units-ex-area", source, "area");
    let cancelled = eval_output_f64("units-ex-cancelled", source, "cancelled");
    let celsius = eval_output_bool("units-ex-celsius", source, "celsius");
    assert_eq!(rescale, 1001.0);
    assert_eq!(area, 1.0);
    assert_eq!(cancelled, 1.0);
    assert!(celsius);
}

#[test]
fn cancelled_length_is_dimensionless() {
    let source = "\
emath function Cancel:
    outputs:
        y: Float64
    definitions:
        y = (1 m) / (1 m)
    tests:
        example <si>:
            expect y == 1
";
    let y = eval_output_f64("m-over-m", source, "y");
    assert_eq!(y, 1.0, "1 m / 1 m must be dimensionless 1, got {y}");
}

#[test]
fn duration_assigned_to_length_names_the_dimensions() {
    let source = "\
emath function Bad:
    outputs:
        y: Float64 in m
    definitions:
        y = 1 s
";
    let messages = error_messages("dur-to-len", source);
    assert!(
        messages.iter().any(|message| {
            message.contains("E-TYPE-012")
                && message.contains("duration")
                && message.contains("length")
                && !message.contains("Infer::Unit")
        }),
        "duration vs length must be named, not Debug-dumped, got {messages:?}"
    );
}

#[test]
fn type_c2_trap_is_length_not_acceleration() {
    let source = "\
emath function C2:
    outputs:
        y: Float64 in m/s*s
    definitions:
        y = 1 m
";
    let codes = errors_of("type-c2", source);
    assert!(
        codes.is_empty(),
        "`in m/s*s` must be length (C2), matching 1 m, got {codes:?}"
    );
}

#[test]
fn zero_celsius_equals_kelvin_offset() {
    let source = "\
emath function Temp:
    outputs:
        y: Bool
    definitions:
        y = (0 degC == 273.15 K)
    tests:
        example <si>:
            expect y == true
";
    let y = eval_output_bool("zero-c", source, "y");
    assert!(y, "0 degC must equal 273.15 K");
}

#[test]
fn celsius_plus_celsius_is_e_unit_102() {
    let source = function_with_compile("numeric strict-f64", "", "y = 1 degC + 1 degC");
    let codes = errors_of("c-plus-c", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-102"),
        "1 degC + 1 degC must be E-UNIT-102, got {codes:?}"
    );
}

#[test]
fn celsius_times_scalar_is_e_unit_102() {
    let source = function_with_compile("numeric strict-f64", "", "y = (1 degC) * 2");
    let codes = errors_of("c-times-2", &source);
    assert!(
        codes.iter().any(|code| code == "E-UNIT-102"),
        "1 degC * 2 must be E-UNIT-102, got {codes:?}"
    );
}

#[test]
fn celsius_plus_kelvin_interval_shifts_the_point() {
    let source = "\
emath function Shift:
    outputs:
        y: Bool
    definitions:
        y = (0 degC + 1 K == 1 degC)
    tests:
        example <si>:
            expect y == true
";
    let y = eval_output_bool("c-plus-k", source, "y");
    assert!(y, "0 degC + 1 K must equal 1 degC");
}

#[test]
fn fahrenheit_uses_offset_before_scale() {
    let source = "\
emath function Fahrenheit:
    outputs:
        freezing: Bool
        boiling: Bool
    definitions:
        freezing = (32 degF == 273.15 K)
        boiling = (212 degF == 373.15 K)
    tests:
        example <c13>:
            expect freezing == true
            expect boiling == true
";
    assert!(eval_output_bool("fahrenheit-c13", source, "freezing"));
    assert!(eval_output_bool("fahrenheit-c13", source, "boiling"));
}

#[test]
fn affine_subtraction_is_a_linear_difference() {
    let source = "\
emath function TemperatureDifference:
    outputs:
        delta: Float64
    definitions:
        delta = (22 degC - 10 degC) / 1 K
    tests:
        example <difference>:
            expect delta == 12
";
    assert_eq!(eval_output_f64("temperature-difference", source, "delta"), 12.0);
}

#[test]
fn litre_spellings_are_identity_aliases() {
    let source = "\
emath function LitreAlias:
    outputs:
        american: Float64
        british: Float64
    definitions:
        american = (1 liter) / (1 L)
        british = (1 litre) / (1 L)
    tests:
        example <aliases>:
            expect american == 1
            expect british == 1
";
    assert_eq!(eval_output_f64("litre-alias", source, "american"), 1.0);
    assert_eq!(eval_output_f64("litre-alias", source, "british"), 1.0);
}
