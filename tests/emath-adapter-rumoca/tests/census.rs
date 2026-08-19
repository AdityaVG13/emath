//! Compiler-phase census tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-rumoca/src/census.rs.

use emath_adapter_rumoca::census::{PHASES, phase};
use emath_adapter_rumoca::{PhaseKind, Stability};

#[test]
fn no_phase_claims_upstream_stability_or_public_contract() {
    // Phase 1 has native stand-ins only; a phase marked Stable or
    // public_contract would claim an upstream Rumoca engine that is
    // not consumed.
    for record in &PHASES {
        assert_ne!(
            record.stability,
            Stability::Stable,
            "phase {:?} must not claim Stable without an upstream engine",
            record.kind
        );
        assert!(
            !record.public_contract,
            "phase {:?} must not claim a public upstream contract",
            record.kind
        );
    }
}

#[test]
fn census_phase_lookup_round_trips() {
    for record in &PHASES {
        assert_eq!(
            phase(record.kind),
            Some(record),
            "phase lookup must find every census row"
        );
    }
    assert_eq!(
        phase(PhaseKind::Resolve).unwrap().note,
        "no name resolver in Phase 1"
    );
}
