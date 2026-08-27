//! Binding provenance admission and identity behavior.

use emath_core::limits::Limits;
use emath_ir::{BindingSite, Provenance};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

fn source(reference: &str) -> String {
    format!(
        "\
emath function CalibratedLength:
    inputs:
        length: Float64
        correction: Float64
    outputs:
        adjusted: Float64
    definitions:
        adjusted = length + correction
    provenance:
        length:
            kind: \"Citation\"
            reference: \"{reference}\"
            adjustment: \"temperature corrected\"
        correction:
            kind: \"Assumed\"
            reason: \"small calibration offset\"
"
    )
}

#[test]
fn binding_provenance_is_admitted_and_changes_artifact_not_meaning_identity() {
    let first = check("provenance-a", &source("doi:10.1234/a"));
    let repeat = check("provenance-repeat", &source("doi:10.1234/a"));
    let changed = check("provenance-b", &source("doi:10.1234/b"));
    assert!(!first.diagnostics.has_errors());
    assert!(!changed.diagnostics.has_errors());

    assert_eq!(
        first.package.binding_provenance.get(&BindingSite::new(
            first.package.declarations[0].id,
            "length"
        )),
        Some(&Provenance::Citation {
            reference: "doi:10.1234/a".into(),
            adjustment: Some("temperature corrected".into()),
        })
    );
    assert_eq!(
        first.package.identity.as_ref().unwrap().content,
        repeat.package.identity.as_ref().unwrap().content,
        "same source and provenance must be deterministic"
    );
    assert_ne!(
        first.package.identity.as_ref().unwrap().content,
        changed.package.identity.as_ref().unwrap().content,
        "provenance is semantic artifact data"
    );
    assert_eq!(
        first.package.meaning_id(&[]).unwrap(),
        changed.package.meaning_id(&[]).unwrap(),
        "provenance does not change the admitted mathematical formula"
    );
}

#[test]
fn all_six_provenance_variants_admit() {
    let source = "\
emath function Sources:
    inputs:
        exact_value: Float64
        cited_value: Float64
        instrument_value: Float64
        fitted_value: Float64
        assumed_value: Float64
        unstated_value: Float64
    definitions:
        result = exact_value
    provenance:
        exact_value:
            kind: \"Exact\"
            source: \"SI definition\"
        cited_value:
            kind: \"Citation\"
            reference: \"doi:10.1234/example\"
        instrument_value:
            kind: \"InstrumentRun\"
            file: \"sha256:abc\"
            processing: \"raw\"
        fitted_value:
            kind: \"Fitted\"
            fit_id: \"sha256:def\"
        assumed_value:
            kind: \"Assumed\"
        unstated_value:
            kind: \"Unstated\"
";
    let result = check("all-provenance", source);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(result.package.binding_provenance.len(), 6);
}

#[test]
fn unknown_provenance_keys_and_bindings_refuse() {
    let source = "\
emath function BadSource:
    inputs:
        value: Float64
    provenance:
        value:
            kind: \"Citation\"
            urlish: \"not a declared key\"
        missing:
            kind: \"Unstated\"
";
    let result = check("bad-provenance", source);
    let codes = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&"E-SYN-152"));
    assert!(codes.contains(&"E-NAME-028"));
}
