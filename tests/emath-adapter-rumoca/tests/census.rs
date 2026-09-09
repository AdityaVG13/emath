//! Compiler-phase census tests.
use emath_adapter_rumoca::census::{PHASES, phase};
use emath_adapter_rumoca::{PhaseKind, Stability};
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("rumoca phase census claims no upstream stability or public contract");
    p.case("no-stable-claim", |p| {
        for r in &PHASES {
            p.demand(format!("{:?}/stable", r.kind), r.stability != Stability::Stable, "must not claim Stable without upstream engine");
            p.demand(format!("{:?}/contract", r.kind), !r.public_contract, "must not claim public contract");
        }
    });
    p.case("lookup", |p| {
        for r in &PHASES {
            p.eq(format!("{:?}", r.kind), phase(r.kind), Some(r));
        }
        p.eq("resolve-note", phase(PhaseKind::Resolve).unwrap().note, "no name resolver in Phase 1");
    });
    p.finish();
}
