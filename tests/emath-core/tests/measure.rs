//! measure tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::measure::*;

#[test]
fn authority_lattice_orders_and_meets() {
    assert!(DataAuthority::Unstated < DataAuthority::Structural);
    assert!(DataAuthority::Structural < DataAuthority::Certified);
    assert_eq!(
        DataAuthority::min(DataAuthority::Certified, DataAuthority::Unstated),
        DataAuthority::Unstated
    );
}

#[test]
fn provenance_canonical_distinguishes_variants() {
    let instrument = DataProvenance::InstrumentRun {
        file: "a".to_string(),
        processing: "p".to_string(),
    };
    let citation = DataProvenance::Citation {
        source: "a".to_string(),
        adjustment: None,
    };
    assert_ne!(instrument.canonical(), citation.canonical());
    assert_eq!(DataProvenance::Unstated.canonical(), "unstated");
}

#[test]
fn header_parsing_handles_bare_and_annotated() {
    assert_eq!(
        parse_header_cell("time (s)"),
        ("time".to_string(), Some("s".to_string()))
    );
    assert_eq!(parse_header_cell("label"), ("label".to_string(), None));
}
