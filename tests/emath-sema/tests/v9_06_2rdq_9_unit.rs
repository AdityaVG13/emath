//! Diagnostics-as-teachers: unit mismatch carries pedagogy.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

#[test]
fn duration_plus_information_has_teacher_help() {
    install_source_parser();
    let source = "\
emath function Timed:
    inputs:
        t: Duration
        bytes: MiB
    outputs:
        y: Float64
    definitions:
        y = t + bytes
    compile:
        target rust
        profile library
        numeric strict-f64
";
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("mismatch", source);
    let error = result
        .diagnostics
        .errors()
        .find(|diagnostic| diagnostic.code == "E-UNIT-101")
        .expect("Duration + MiB must be E-UNIT-101");
    let pedagogy = error
        .pedagogy
        .as_ref()
        .expect("E-UNIT-101 must carry pedagogy");
    assert!(!pedagogy.understood.is_empty());
    assert!(!pedagogy.unknown.is_empty());
    assert!(!pedagogy.why.is_empty());
    assert!(!pedagogy.smallest_repair.is_empty());
    assert!(!pedagogy.alternatives.is_empty());
    assert!(pedagogy.example.as_ref().is_some());
    assert!(pedagogy.deeper_concept.as_ref().is_some());
    assert!(pedagogy.authority_consequence.as_ref().is_some());
    assert!(
        pedagogy
            .library_link
            .as_deref()
            .unwrap()
            .contains("types-units")
    );
    let help = error.help.as_deref().unwrap_or("");
    assert!(help.contains("understood:"), "{help}");
    assert!(help.contains("missing:"), "{help}");
    assert!(help.contains("smallest fix:"), "{help}");
    assert!(help.contains("library:"), "{help}");
    assert!(!help.to_lowercase().contains("stellar"), "{help}");
}
