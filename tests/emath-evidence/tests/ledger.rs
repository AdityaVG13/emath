//! Assumption-ledger tests.
//!
//! Moved from #[cfg(test)] in crates/emath-evidence/src/ledger.rs.

use emath_evidence::{Assumption, AssumptionLedger, PremiseClass};

fn assumption(id: &str, class: PremiseClass) -> Assumption {
    Assumption {
        id: id.into(),
        statement: "assumption statement".into(),
        class,
        provenance: "examples/02".into(),
    }
}

#[test]
fn reclassifying_an_assumption_is_refused() {
    let mut ledger = AssumptionLedger::default();
    for (id, class) in [
        ("a1", PremiseClass::Math),
        ("b1", PremiseClass::Numeric),
        ("c1", PremiseClass::System),
        ("d1", PremiseClass::Environment),
        ("e1", PremiseClass::Host),
    ] {
        ledger.register(assumption(id, class)).unwrap();
    }
    let ids: Vec<&str> = ledger
        .assumptions()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids, ["a1", "b1", "c1", "d1", "e1"]);
    assert!(ledger.canonical().contains("a1:M:"));
    assert_eq!(ledger.counts()[0], (PremiseClass::Math, 1));

    let error = ledger
        .register(assumption("a1", PremiseClass::Numeric))
        .unwrap_err();
    assert_eq!(error.code, "E-EVID-405");
    assert_eq!(ledger.assumptions().len(), 5);

    ledger
        .register(assumption("a1", PremiseClass::Math))
        .unwrap();
    assert_eq!(ledger.assumptions().len(), 5);
}
