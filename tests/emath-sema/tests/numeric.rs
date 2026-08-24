//! Numeric-model admission, unit/shape/domain refusals, and e2e corpus.

use emath_core::limits::Limits;
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
    let source = function_with_compile("numeric strict-f64\n        precision 128", "", "y = t / 1 s");
    let codes = errors_of("precision", &source);
    assert!(
        codes.iter().any(|code| code == "E-NUM-002"),
        "precision 128 must be E-NUM-002, got {codes:?}"
    );
}

#[test]
fn error_limit_no_model_can_honor_is_e_num_003() {
    let source =
        function_with_compile("numeric strict-f64\n        error-limit 1e-20", "", "y = t / 1 s");
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
    let source = function_with_compile(
        "numeric strict-f64",
        "bytes: MiB",
        "y = t + bytes",
    );
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
    let source = function_with_compile("numeric strict-f64\n        domain 5..1", "", "y = t / 1 s");
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
        !result
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.message.contains("unit system arrives in Phase 5")),
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

// ─── reverse-mode AD (emath-xx0x.1) ───

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
        !codes.is_empty(),
        "domain annotation on non-numeric type must error"
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
    let source = function_with_compile(
        "numeric strict-f64",
        "",
        "y = 9.81 [unit m/(s*s)]",
    );
    let codes = errors_of("compound-paren", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in m/(s*s) must not produce E-UNIT-104, got {codes:?}"
    );
}

#[test]
fn compound_unit_with_unknown_unit_errors() {
    // `1.0 [unit m/furlong]` — furlong is not a known unit.
    let source = function_with_compile(
        "numeric strict-f64",
        "",
        "y = 1.0 [unit m/furlong]",
    );
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
    let source = function_with_compile(
        "numeric strict-f64",
        "",
        "y = 100.0 [unit kg*m^2/s^2]",
    );
    let codes = errors_of("compound-energy", &source);
    assert!(
        !codes.iter().any(|code| code == "E-UNIT-104"),
        "known units in kg*m^2/s^2 must not produce E-UNIT-104, got {codes:?}"
    );
}
