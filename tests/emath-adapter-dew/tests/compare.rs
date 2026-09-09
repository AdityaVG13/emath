//! Focused tests for the shared compare helpers.
use emath_adapter_dew_tests::{ToleranceClass, bytes_eq, canonical_lines, classify_tolerance, f64_bits};
use emath_test_harness::{Case, Probe, check_all};

#[test]
fn probe() {
    let mut p = Probe::new("dew compare helpers expose bit identity, structural lines, and tolerance buckets");
    p.eq("zero-bits", f64_bits(0.0), 0x0000_0000_0000_0000);
    p.eq("neg-zero-bits", f64_bits(-0.0), 0x8000_0000_0000_0000);
    p.ne("signed-zero-distinct", f64_bits(0.0), f64_bits(-0.0));
    p.demand("eq-blind", 0.0_f64 == -0.0_f64, "== must be blind to sign");
    p.eq("empty", canonical_lines(""), String::new());
    p.eq("blank", canonical_lines("   \n\n \n"), String::new());
    let canon = canonical_lines("{\n  \"a\": 1,\n  \"b\": [2, 3],\n}\n");
    p.demand("ragged", bytes_eq(canon.as_bytes(), canonical_lines("{\n   \"a\": 1,   \n\n \n  \"b\": [2, 3],\n}\n\n").as_bytes()), "ragged must canonicalize");
    p.demand("blanks", bytes_eq(canon.as_bytes(), canonical_lines("{\n  \"a\": 1,\n\n\n  \"b\": [2, 3],\n}\n\n\n").as_bytes()), "blanks must canonicalize");
    if let Err(m) = check_all(
        &[
            Case::new("exact", (100.0, 100.2, 0.5, 1.0), ToleranceClass::Exact),
            Case::new("loose", (100.0, 100.8, 0.5, 1.0), ToleranceClass::Loose),
            Case::new("oor", (100.0, 102.0, 0.5, 1.0), ToleranceClass::OutOfRange),
            Case::new("edge-tight", (1.0, 1.5, 0.5, 1.0), ToleranceClass::Exact),
            Case::new("edge-loose", (1.0, 2.0, 0.5, 1.0), ToleranceClass::Loose),
            Case::new("edge-oor", (1.0, 2.1, 0.5, 1.0), ToleranceClass::OutOfRange),
        ],
        |i| classify_tolerance(i.0, i.1, i.2, i.3),
    ) {
        p.fail("tolerance", m);
    } else {
        p.demand("tolerance", true, "ok");
    }
    p.finish();
}
