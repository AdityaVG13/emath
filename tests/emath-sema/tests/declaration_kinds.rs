//! Spec-oracle: declaration kinds, sections, and admission
//! (`language/CAPABILITY.md` vs syntax + `emath-sema`).

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

struct Probe {
    parse_ok: bool,
    messages: Vec<String>,
    admitted_names: Vec<String>,
    admitted_labels: Vec<String>,
}

fn probe(name: &str, source: &str) -> Probe {
    let (_, parse_diags) = parse_str(source);
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    Probe {
        parse_ok: !parse_diags.has_errors(),
        messages: result
            .diagnostics
            .errors()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect(),
        admitted_names: result
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf().to_string())
            .collect(),
        admitted_labels: result
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.kind_label.clone())
            .collect(),
    }
}

fn square_body() -> &'static str {
    "    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
}

#[test]
fn emath_kind_validates_schema_and_does_not_run() {
    // CAPABILITY: `emath kind` parses, partial schema validation, does not run.
    let source = "\
emath kind Scoring:
    extends model

    schema:
        require section inputs
        allow section state

    lower:
        model.inputs = section.inputs
";
    let result = probe("kind-ok", source);
    assert!(
        result.parse_ok,
        "kind declaration must parse, got {:?}",
        result.messages
    );
    assert!(
        result.messages.is_empty(),
        "valid kind schema must admit with no errors, got {:?}",
        result.messages
    );
    assert_eq!(
        result.admitted_names,
        ["Scoring"],
        "valid kind schemas register a marker for later application diagnostics"
    );
    assert_eq!(
        result.admitted_labels,
        ["kind"],
        "the marker must remain distinguishable from a runnable function"
    );
}

#[test]
fn emath_kind_unknown_section_is_named_not_silent() {
    let source = "\
emath kind Scoring:
    inputs:
        x: Float64
";
    let result = probe("kind-bad-section", source);
    assert!(
        result.parse_ok,
        "kind with an extra section must still parse, got {:?}",
        result.messages
    );
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-SYN-101") && message.contains("inputs")),
        "unknown section on emath kind must be a named section refusal, got {:?}",
        result.messages
    );
    assert!(
        result.admitted_names.is_empty(),
        "kind must not be admitted as a function, got {:?}",
        result.admitted_names
    );
}

#[test]
fn emath_custom_refuses_or_treats_as_function_without_crash() {
    // CAPABILITY: parses; treats as function or refuses; does not run.
    let source = format!("emath custom Square:\n{body}", body = square_body());
    let result = probe("custom", &source);
    assert!(
        result.parse_ok,
        "emath custom must parse, got {:?}",
        result.messages
    );
    let treated_as_function = result.admitted_labels == ["function".to_string()]
        && result.admitted_names == ["Square".to_string()];
    let refused = result
        .messages
        .iter()
        .any(|message| message.contains("E-KIND-"))
        && result.admitted_names.is_empty();
    assert!(
        treated_as_function || refused,
        "emath custom must treat as function or refuse with a named error, got names={:?} labels={:?} messages={:?}",
        result.admitted_names,
        result.admitted_labels,
        result.messages
    );
    if refused {
        assert!(
            result
                .messages
                .iter()
                .any(|message| message.contains("`custom`")),
            "custom refusal must name `custom`, not an empty type, got {:?}",
            result.messages
        );
    }
}

#[test]
fn other_kind_refuses_with_named_error() {
    let source = format!("emath widget W:\n{body}", body = square_body());
    let result = probe("widget", &source);
    assert!(
        result.parse_ok,
        "other kinds must parse, got {:?}",
        result.messages
    );
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-KIND-")),
        "other kinds must refuse with a named E-KIND error, got {:?}",
        result.messages
    );
    assert!(
        result.admitted_names.is_empty(),
        "other kinds must not be admitted, got {:?}",
        result.admitted_names
    );
}

#[test]
fn transitions_and_events_parse_and_are_not_admitted() {
    let source = "\
emath function Hybrid:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    transitions:
        dummy = 1
    events:
        dummy = 1
";
    let result = probe("hybrid-sections", source);
    assert!(
        result.parse_ok,
        "transitions/events must parse, got {:?}",
        result.messages
    );
    // `events:`/`transitions:` are admitted Phase 1 sections (hybrid
    // events bead, r3-dynamical-03lh); malformed CONTENT is the refusal:
    // `dummy = 1` is not an `event Name(field: Type)` declaration
    // (E-SYN-101) and not an `on <Event>:` rule (E-TRANS-003).
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-SYN-101")),
        "malformed `events:` content must refuse with E-SYN-101, got {:?}",
        result.messages
    );
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-TRANS-003")),
        "malformed `transitions:` content must refuse with E-TRANS-003, got {:?}",
        result.messages
    );
}

#[test]
fn invariant_section_is_admitted() {
    let source = "\
emath function Bounded:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    invariant:
        x >= 0
";
    let result = probe("invariant", source);
    assert!(
        result.parse_ok && result.messages.is_empty(),
        "invariant: must admit, got {:?}",
        result.messages
    );
    assert_eq!(result.admitted_names, ["Bounded".to_string()]);
}

#[test]
fn invariants_plural_section_is_refused() {
    // CAPABILITY / reference spelling is `invariant:` (singular). The
    // plural is E-SEC-101, not an admitted alias.
    let source = "\
emath function Bounded:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    invariants:
        x >= 0
";
    let result = probe("invariants-plural", source);
    assert!(
        result.parse_ok,
        "invariants: must parse as a section head, got {:?}",
        result.messages
    );
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-SEC-101") && message.contains("invariants")),
        "invariants: must be E-SEC-101, got {:?}",
        result.messages
    );
}

#[test]
fn emath_kind_schema_shape_is_validated() {
    let source = "\
emath kind Scoring:
    schema:
        x = 1
";
    let result = probe("kind-schema-shape", source);
    assert!(
        result.parse_ok,
        "kind schema must parse, got {:?}",
        result.messages
    );
    assert!(
        result
            .messages
            .iter()
            .any(|message| message.contains("E-SYN-101") && message.contains("schema")),
        "assignment in schema: must be a named shape refusal, got {:?}",
        result.messages
    );
    assert!(
        result.admitted_names.is_empty(),
        "invalid kind schema must not run, got {:?}",
        result.admitted_names
    );
}
