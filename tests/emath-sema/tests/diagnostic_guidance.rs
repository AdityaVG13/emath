//! Diagnostics for unit-typed inputs after the constructor cutover: unit
//! type names are not constructor primitives, so a file that declares
//! `Duration`/`MiB` inputs refuses at typing (`E-KIND-GONE`) with the
//! primitive guidance in the message. The `E-UNIT-101` teacher-pedagogy
//! lane belonged to the pre-cutover admission pipeline, which no user
//! file can reach: unit arithmetic cannot occur because unit types
//! never survive typing.

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
fn unit_typed_inputs_refuse_at_typing() {
    boot();
    let mut p = Probe::new("unit-typed inputs refuse as non-constructor primitives");
    let result = Source::from_str("mismatch", TIMED).check();
    match result.diagnostics.errors().find(|d| d.code == "E-KIND-GONE") {
        Some(error) => {
            p.contains("names-type", &error.message, "Duration");
            p.contains("guidance", &error.message, "Bool, Int, Rat, Float64");
        }
        None => {
            p.fail(
                "kind-gone",
                "Duration input must refuse as a non-constructor primitive",
            );
        }
    }
    p.finish();
}
