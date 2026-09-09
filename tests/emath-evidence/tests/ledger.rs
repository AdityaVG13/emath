//! Assumption-ledger tests.

use emath_evidence::{Assumption, AssumptionLedger, PremiseClass};
use emath_test_harness::Probe;

fn assumption(id: &str, class: PremiseClass) -> Assumption {
    Assumption {
        id: id.into(),
        statement: "assumption statement".into(),
        class,
        provenance: "examples/02".into(),
    }
}

#[test]
fn assumption_ledger() {
    let mut p = Probe::new("reclassifying a registered assumption is refused");
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
    p.case("register", |p| {
        let ids: Vec<&str> = ledger.assumptions().iter().map(|e| e.id.as_str()).collect();
        p.eq("order", ids, ["a1", "b1", "c1", "d1", "e1"].to_vec());
        p.contains("canonical", &ledger.canonical(), "a1:M:");
        p.eq("counts", ledger.counts()[0], (PremiseClass::Math, 1));
    });
    p.case("reclassify-refused", |p| {
        let error = ledger.register(assumption("a1", PremiseClass::Numeric)).unwrap_err();
        p.eq("code", error.code, "E-EVID-405");
        p.eq("len-held", ledger.assumptions().len(), 5);
    });
    p.case("idempotent-reregister", |p| {
        ledger.register(assumption("a1", PremiseClass::Math)).unwrap();
        p.eq("len", ledger.assumptions().len(), 5);
    });
    p.finish();
}
