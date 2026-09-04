//! ELP experimental-lane gates: `@capabilities` / `@experimental` item
//! attributes and their typed refusals (see `elps/README.md`).
//!
//! Intent: experimental syntax must never compile silently in a stable
//! package — every gate below proves either the refusal (E-PKG-064 /
//! E-SYN-117 / E-SYN-118 / E-PKG-065) or the declared-capability admit.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<String> {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("capabilities", source);
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

fn function_source(prefix: &str) -> String {
    format!(
        "{prefix}emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
    )
}

/// Negative control: `@experimental` without the capability is E-PKG-064.
#[test]
fn experimental_without_capability_refuses() {
    let codes = check(&function_source("@experimental\n"));
    assert!(
        codes.contains(&"E-PKG-064".to_string()),
        "expected E-PKG-064, got {codes:?}"
    );
}

/// Positive control: declaring `experimental-syntax` admits the item.
#[test]
fn experimental_with_capability_admits() {
    let codes = check(&function_source(
        "@capabilities(experimental-syntax)\n@experimental\n",
    ));
    assert!(
        !codes.contains(&"E-PKG-064".to_string()),
        "capability declared, expected no E-PKG-064, got {codes:?}"
    );
    assert!(
        !codes.iter().any(|code| code.starts_with("E-PKG-06")),
        "no experimental-lane refusal expected, got {codes:?}"
    );
}

/// The capability is file-scoped: declaring it on one item admits an
/// `@experimental` item elsewhere in the same source file.
#[test]
fn capability_is_file_scoped() {
    let source = format!(
        "{}{}",
        function_source("@capabilities(experimental-syntax)\n"),
        function_source("@experimental\n")
    );
    let codes = check(&source);
    assert!(
        !codes.contains(&"E-PKG-064".to_string()),
        "file-scoped capability must admit, got {codes:?}"
    );
}

/// Quoted capability keys parse the same as bare identifiers.
#[test]
fn quoted_capability_key_is_accepted() {
    let codes = check(&function_source(
        "@capabilities(\"experimental-syntax\")\n@experimental\n",
    ));
    assert!(
        !codes.contains(&"E-PKG-064".to_string()),
        "quoted key must admit, got {codes:?}"
    );
}

/// Unknown attributes are refused, never silently dropped.
#[test]
fn unknown_attribute_refuses() {
    let codes = check(&function_source("@bogus\n"));
    assert!(
        codes.contains(&"E-SYN-118".to_string()),
        "expected E-SYN-118, got {codes:?}"
    );
}

/// Unknown capability keys are refused.
#[test]
fn unknown_capability_key_refuses() {
    let codes = check(&function_source("@capabilities(teleportation)\n"));
    assert!(
        codes.contains(&"E-PKG-065".to_string()),
        "expected E-PKG-065, got {codes:?}"
    );
}

/// The `experimental` attribute takes no arguments.
#[test]
fn experimental_with_args_refuses() {
    let codes = check(&function_source("@experimental(deep)\n"));
    assert!(
        codes.contains(&"E-SYN-117".to_string()),
        "expected E-SYN-117, got {codes:?}"
    );
}

/// Malformed attribute arguments are a parser-level refusal.
#[test]
fn attribute_argument_subset_enforced() {
    let codes = check(&function_source("@capabilities(x = 1)\n"));
    assert!(
        codes.contains(&"E-SYN-117".to_string()),
        "expected E-SYN-117, got {codes:?}"
    );
}
