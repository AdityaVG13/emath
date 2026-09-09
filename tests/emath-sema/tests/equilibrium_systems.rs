//! Equilibrium systems failure-first tests (04 §3.3; builds on the
//! CLOSED grammar).
//!
//! Contracts (each must FAIL against the pre-admission):
//! - an equilibrium constant line (`Ka: Measured<Real> in M = …`) is
//!   admitted in a `reaction_network` body and carries its uncertainty;
//!   a Ka without the uncertainty form refuses (E-CHEM-KA-EXACT) —
//!   uncertainty is the point of a measured equilibrium constant.
//! - `K == kf/kr` honesty triangle: a network declaring BOTH a reversible
//!   kinetic pair (`<->` with a rate) AND an equilibrium (`<=>` with a
//!   constant) must be consistent within combined uncertainty; violation
//!   = `E-CHEM-THERMO`. Without both sides, the gate stays silent.
//! - the `<=>` relation is RECORDED as recognized meaning (admission
//!   trace), never evaluated — Newton solving is the eval tier, fenced.

use emath_test_harness::{Probe, Source, boot};

const ACETIC: &str = "\
emath reaction_network AceticDissociation:\n    species:\n        CH3COOH\n        H2O\n        CH3COO\n        H3O\n    Ka: Measured<Real> in M = 1.75(3)e-5\n    reactions:\n        dissoc: CH3COOH + H2O <=> CH3COO + H3O\n";

const CONSISTENT: &str = "\
emath reaction_network ConsistentPair:\n    species:\n        A\n        B\n    K: Measured<Real> = 2.0(1)\n    rate:\n        kf = 4.0\n        kr = 2.0\n    reactions:\n        kinetic: A <-> B\n        equil: A <=> B\n";

#[test]
fn equilibrium_systems() {
    boot();
    let mut p = Probe::new("measured Ka admits, exact Ka and thermo violations refuse typed");
    p.case("ka-admits", |p| {
        Source::from_str("acetic", ACETIC).must_admit(p);
    });
    p.case("exact-ka-refuses", |p| {
        Source::from_workspace("tests/invalid/equilibrium_ka_specification.emath")
            .must_refuse(p, &["E-CHEM-KA-EXACT"]);
    });
    p.case("thermo-violation-refuses", |p| {
        Source::from_workspace("tests/invalid/equilibrium_thermodynamics.emath")
            .must_refuse(p, &["E-CHEM-THERMO"]);
    });
    p.case("consistent-admits", |p| {
        Source::from_str("consistent-pair", CONSISTENT).must_admit(p);
    });
    p.case("missing-constant-refuses", |p| {
        Source::from_workspace("tests/invalid/equilibrium_missing_constant.emath")
            .must_refuse(p, &["E-CHEM-THERMO"]);
    });
    p.finish();
}
