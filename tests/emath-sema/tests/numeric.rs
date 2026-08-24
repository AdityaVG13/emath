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
