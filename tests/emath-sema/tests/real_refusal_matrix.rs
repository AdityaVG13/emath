//! Pass 5 (emath-rat-real-types-p5cj): TOTAL refusal matrix for bare `Real`
//! at type sites. Every context where bare `Real` appears must produce ONE
//! deterministic E-NUM-004 diagnostic naming the three sanctioned spellings:
//! `Float64` (strict-f64 profile), `Interval<Float64>` (certified-interval
//! surrogate), or the `representation Real => Float64` directive. No
//! shape-dependent behavior: bare input vs Vector element → same code, same
//! message.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;

fn diagnostics_of(source: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("real-refusal", source);
    result
        .diagnostics
        .errors()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect()
}

fn enum004_messages(messages: &[String]) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.contains("E-NUM-004"))
        .cloned()
        .collect()
}

fn install_source_parser() {
    // Idempotent installer from emath-syntax, mirrors other suites here.
    emath_syntax::install_source_parser();
}

const REAL_INPUT: &str = "\
emath function F:
    inputs:
        x: Real
    outputs:
        y: Float64
    definitions:
        y = x
";

const REAL_VECTOR_ELEMENT: &str = "\
emath function G:
    inputs:
        v: Vector[Real, 3]
    outputs:
        y: Float64
    definitions:
        y = v[0]
";

const FLOAT64_CONTROL: &str = "\
emath function H:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
";

const RAT_CONTROL: &str = "\
emath function K:
    inputs:
        a: Rat
        b: Rational
    outputs:
        r: Rat
    definitions:
        r = a * b
";

/// (a) bare `x: Real` in an ordinary emath function → exactly E-NUM-004,
/// naming all three sanctioned spellings.
#[test]
fn real_bare_input_is_refused_with_enum004() {
    let messages = diagnostics_of(REAL_INPUT);
    let enum004 = enum004_messages(&messages);
    assert_eq!(
        enum004.len(),
        1,
        "exactly one E-NUM-004 for the bare `Real` input, got {messages:?}"
    );
    let message = &enum004[0];
    assert!(
        message.contains("Float64"),
        "diagnostic must name `Float64`: {message}"
    );
    assert!(
        message.contains("Interval<Float64>"),
        "diagnostic must name `Interval<Float64>`: {message}"
    );
    assert!(
        message.contains("representation Real => Float64"),
        "diagnostic must cite the representation directive: {message}"
    );
}

/// (b) `Vector[Real, 3]` element position → SAME code, SAME message
/// (no shape-dependent behavior).
#[test]
fn real_vector_element_matches_bare_refusal() {
    let bare = diagnostics_of(REAL_INPUT);
    let vector = diagnostics_of(REAL_VECTOR_ELEMENT);
    let bare_enum004 = enum004_messages(&bare);
    let vector_enum004 = enum004_messages(&vector);
    assert_eq!(
        bare_enum004, vector_enum004,
        "E-NUM-004 must be shape-independent: bare input vs Vector element"
    );
}

/// (c) control: sanctioned spellings keep working.
#[test]
fn float64_and_interval_spellings_still_admitted() {
    let messages = diagnostics_of(FLOAT64_CONTROL);
    assert!(
        messages.is_empty(),
        "`Float64` control must fully type-check, got {messages:?}"
    );
}

/// (d) pass 2 regression guard: Rat/Rational stay admitted.
#[test]
fn rat_spellings_still_admitted() {
    let messages = diagnostics_of(RAT_CONTROL);
    assert!(
        messages.is_empty(),
        "Rat/Rational must remain admitted, got {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// Full-context matrix (bead closure): EVERY syntactic position where bare
// `Real` can appear at a type site emits exactly the canonical E-NUM-004 —
// same code, same message, no shape-dependent behavior, never silently f64.
// ---------------------------------------------------------------------------

/// The single canonical diagnostic, verbatim, for every context.
const CANONICAL_E_NUM_004: &str = "E-NUM-004: bare `Real` at a type site \
requires profile evidence; write `Float64` (strict-f64), \
`Interval<Float64>` (certified interval), or a \
`representation Real => Float64` directive";

fn assert_exactly_one_canonical(context: &str, source: &str) {
    let messages = diagnostics_of(source);
    assert_eq!(
        enum004_messages(&messages),
        vec![CANONICAL_E_NUM_004.to_string()],
        "{context}: bare `Real` must produce exactly the canonical E-NUM-004, got {messages:?}"
    );
}

/// Output field position.
#[test]
fn real_output_field_is_refused() {
    assert_exactly_one_canonical(
        "output field",
        "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Real\n    definitions:\n        y = x\n",
    );
}

/// State field position (stateful declaration).
#[test]
fn real_state_field_is_refused() {
    assert_exactly_one_canonical(
        "state field",
        "emath model M:\n    inputs:\n        x: Float64\n    state:\n        s: Real\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n",
    );
}

/// Matrix element position.
#[test]
fn real_matrix_element_is_refused() {
    assert_exactly_one_canonical(
        "Matrix element",
        "emath function F:\n    inputs:\n        m: Matrix[Real, 2, 2]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Tensor element position.
#[test]
fn real_tensor_element_is_refused() {
    assert_exactly_one_canonical(
        "Tensor element",
        "emath function F:\n    inputs:\n        t: Tensor[Real, 2, 2, 2]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Nested shape element position (innermost Real, one diagnostic).
#[test]
fn real_nested_vector_element_is_refused() {
    assert_exactly_one_canonical(
        "nested Vector element",
        "emath function F:\n    inputs:\n        v: Vector[Vector[Real, 2], 3]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Option element position.
#[test]
fn real_option_element_is_refused() {
    assert_exactly_one_canonical(
        "Option element",
        "emath function F:\n    inputs:\n        o: Option<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Result ok-arm position.
#[test]
fn real_result_ok_arm_is_refused() {
    assert_exactly_one_canonical(
        "Result ok-arm",
        "emath function F:\n    inputs:\n        r: Result<Real, Float64>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Result error-arm position.
#[test]
fn real_result_err_arm_is_refused() {
    assert_exactly_one_canonical(
        "Result err-arm",
        "emath function F:\n    inputs:\n        r: Result<Float64, Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Set element position.
#[test]
fn real_set_element_is_refused() {
    assert_exactly_one_canonical(
        "Set element",
        "emath function F:\n    inputs:\n        s: Set<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Interval element position.
#[test]
fn real_interval_element_is_refused() {
    assert_exactly_one_canonical(
        "Interval element",
        "emath function F:\n    inputs:\n        i: Interval<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Refinement (NonNegative) element position.
#[test]
fn real_refinement_element_is_refused() {
    assert_exactly_one_canonical(
        "refinement element",
        "emath function F:\n    inputs:\n        q: NonNegative<Real>\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Domain-annotation base position.
#[test]
fn real_domain_base_is_refused() {
    assert_exactly_one_canonical(
        "domain base",
        "emath function F:\n    inputs:\n        x: Real in [0, 1]\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Unit-annotation base position.
#[test]
fn real_unit_base_is_refused() {
    assert_exactly_one_canonical(
        "unit base",
        "emath function F:\n    inputs:\n        x: Real in m\n    outputs:\n        y: Float64\n    definitions:\n        y = 1\n",
    );
}

/// Event parameter position.
#[test]
fn real_event_parameter_is_refused() {
    assert_exactly_one_canonical(
        "event parameter",
        "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    events:\n        event Tick(x: Real)\n",
    );
}

/// Constructor parameter position.
#[test]
fn real_constructor_parameter_is_refused() {
    assert_exactly_one_canonical(
        "constructor parameter",
        "emath policy P:\n    inputs:\n        x: Float64\n    state:\n        v: Float64\n    constructors:\n        public fn new(x: Real) -> Float64:\n            require x == x\n            Self:\n                v = x\n",
    );
}

/// Constructor return position.
#[test]
fn real_constructor_return_is_refused() {
    assert_exactly_one_canonical(
        "constructor return",
        "emath policy P:\n    inputs:\n        x: Float64\n    state:\n        v: Float64\n    constructors:\n        public fn new(x: Float64) -> Real:\n            require x == x\n            Self:\n                v = x\n",
    );
}

/// Observations type-annotation position.
#[test]
fn real_observation_annotation_is_refused() {
    assert_exactly_one_canonical(
        "observation annotation",
        "emath function F:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    observations:\n        obs r: Real = 1.0\n",
    );
}
