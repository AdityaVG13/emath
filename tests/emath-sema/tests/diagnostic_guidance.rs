//! Diagnostics-as-teachers: unit mismatch carries pedagogy.

use emath_test_harness::{Probe, Source, boot};

const TIMED: &str = "\
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

#[test]
fn unit_mismatch_teaches() {
    boot();
    let mut p = Probe::new("unit mismatch E-UNIT-101 carries teacher pedagogy");
    let result = Source::from_str("mismatch", TIMED).check();
    if let Some(error) = result.diagnostics.errors().find(|d| d.code == "E-UNIT-101") {
        if let Some(pedagogy) = error.pedagogy.as_ref() {
            p.eq("understood", pedagogy.understood.is_empty(), false);
            p.eq("unknown", pedagogy.unknown.is_empty(), false);
            p.eq("why", pedagogy.why.is_empty(), false);
            p.eq("smallest-repair", pedagogy.smallest_repair.is_empty(), false);
            p.eq("alternatives", pedagogy.alternatives.is_empty(), false);
            p.eq("example", pedagogy.example.as_ref().is_some(), true);
            p.eq("deeper-concept", pedagogy.deeper_concept.as_ref().is_some(), true);
            p.eq("authority", pedagogy.authority_consequence.as_ref().is_some(), true);
            p.eq(
                "library-link",
                pedagogy.library_link.as_deref().unwrap_or("").contains("types-units"),
                true,
            );
        } else {
            p.fail("pedagogy", "E-UNIT-101 must carry pedagogy");
        }
        let help = error.help.as_deref().unwrap_or("");
        p.contains("help/understood", help, "understood:");
        p.contains("help/missing", help, "missing:");
        p.contains("help/fix", help, "smallest fix:");
        p.contains("help/library", help, "library:");
        p.eq("help/no-stellar", help.to_lowercase().contains("stellar"), false);
    } else {
        p.fail("unit-101", "Duration + MiB must refuse as E-UNIT-101");
    }
    p.finish();
}
