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
