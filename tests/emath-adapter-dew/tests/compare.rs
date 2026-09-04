//! Focused tests for the shared compare helpers.
//!
//! Each test exercises a single helper with its defining intent: bit-level
//! f64 identity, structural (not lexical) line comparison, and the
//! Exact/Loose/OutOfRange tolerance buckets.

use emath_adapter_dew_tests::{
    ToleranceClass, bytes_eq, canonical_lines, classify_tolerance, f64_bits,
};

#[test]
fn f64_bits_distinguishes_zero_signs() {
    // +0.0 == -0.0 under `==`, but they are distinct bit patterns. The
    // helper must expose the bit-level identity so a harness can tell them
    // apart (bit-exactness intent, not arithmetic equality).
    assert_eq!(f64_bits(0.0), 0x0000_0000_0000_0000);
    assert_eq!(f64_bits(-0.0), 0x8000_0000_0000_0000);
    assert_ne!(f64_bits(0.0), f64_bits(-0.0));
    assert_eq!(0.0_f64 == -0.0_f64, true); // `==` cannot see the difference.
}

#[test]
fn canonical_lines_ignores_trailing_whitespace_and_blanks() {
    let structured = "{\n  \"a\": 1,\n  \"b\": [2, 3],\n}\n";
    let ragged = "{\n   \"a\": 1,   \n\n \n  \"b\": [2, 3],\n}\n\n";
    let trailing_blanks = "{\n  \"a\": 1,\n\n\n  \"b\": [2, 3],\n}\n\n\n";

    let canonical = canonical_lines(structured);
    assert!(bytes_eq(
        canonical.as_bytes(),
        canonical_lines(ragged).as_bytes()
    ));
    assert!(bytes_eq(
        canonical.as_bytes(),
        canonical_lines(trailing_blanks).as_bytes()
    ));

    // Empty and all-whitespace input reduce to a single empty (no) line.
    assert_eq!(canonical_lines(""), "");
    assert_eq!(canonical_lines("   \n\n \n"), "");
}

#[test]
fn classify_tolerance_covers_all_three_arms() {
    // tight < loose, so the Exact/Loose/OutOfRange buckets are disjoint.
    let context = "all three arms";
    let tight = 0.5;
    let loose = 1.0;

    assert_eq!(
        classify_tolerance(100.0, 100.2, tight, loose),
        ToleranceClass::Exact,
        "{context}"
    );
    assert_eq!(
        classify_tolerance(100.0, 100.8, tight, loose),
        ToleranceClass::Loose,
        "{context}"
    );
    assert_eq!(
        classify_tolerance(100.0, 102.0, tight, loose),
        ToleranceClass::OutOfRange,
        "{context}"
    );

    // Boundary ordering: at exactly tight it is still Exact (<=), beyond loose it is not.
    assert_eq!(
        classify_tolerance(1.0, 1.5, tight, loose),
        ToleranceClass::Exact
    );
    assert_eq!(
        classify_tolerance(1.0, 2.0, tight, loose),
        ToleranceClass::Loose
    );
    assert_eq!(
        classify_tolerance(1.0, 2.1, tight, loose),
        ToleranceClass::OutOfRange
    );
}
