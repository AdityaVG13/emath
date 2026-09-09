//! Diagnostics-as-teachers: understood, missing, smallest fix, library link.

use emath_syntax::parse_str;

use emath_test_harness::{Probe, boot};

#[test]
fn diagnostic_guidance() {
    boot();
    let mut probe = Probe::new("Diagnostics-as-teachers: understood, missing, smallest fix, library link.");
    probe.case("scratch_refusal_carries_pedagogy", |p| {
    let f0 = p.failures().len();

    let (_, diagnostics) = parse_str("this is not emath at all\n");
    let error = diagnostics
        .errors()
        .find(|error| error.code == "E-SYN-145" || error.code == "E-SYN-148")
        .expect("scratch junk must refuse");
    let help = error.help.as_deref().unwrap_or("");
    p.demand("1",help.contains("understood:"), format!( "{help}"));
    if p.failures().len() != f0 { return; }
    p.demand("2",help.contains("missing:"), format!( "{help}"));
    if p.failures().len() != f0 { return; }
    p.demand("3",help.contains("smallest fix:"), format!( "{help}"));
    if p.failures().len() != f0 { return; }
    p.demand("4",help.contains("library:"), format!( "{help}"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("example_file_parses", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/scratch.emath");
    let (_, diagnostics) = parse_str(source);
    p.demand("1",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("hidden_desugar_still_refused", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/diagnostic_guidance.emath");
    let (_, diagnostics) = parse_str(source);
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-144"), stringify!(diagnostics.errors().any(|error| error.code == "E-SYN-144")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
