//! Diagnostics-as-teachers: understood, missing, smallest fix, library link.

use emath_syntax::parse_str;

#[test]
fn scratch_refusal_carries_pedagogy() {
    let (_, diagnostics) = parse_str("this is not emath at all\n");
    let error = diagnostics
        .errors()
        .find(|error| error.code == "E-SYN-145" || error.code == "E-SYN-148")
        .expect("scratch junk must refuse");
    let help = error.help.as_deref().unwrap_or("");
    assert!(help.contains("understood:"), "{help}");
    assert!(help.contains("missing:"), "{help}");
    assert!(help.contains("smallest fix:"), "{help}");
    assert!(help.contains("library:"), "{help}");
}

#[test]
fn example_file_parses() {
    let source = include_str!("../../../language/examples/intro/scratch.emath");
    let (_, diagnostics) = parse_str(source);
    assert!(!diagnostics.has_errors());
}

#[test]
fn hidden_desugar_still_refused() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_9.emath");
    let (_, diagnostics) = parse_str(source);
    assert!(diagnostics.errors().any(|error| error.code == "E-SYN-144"));
}
