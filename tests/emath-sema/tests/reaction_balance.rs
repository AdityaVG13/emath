//! admission-side contracts (sema tier).
//!
//! Species closure and element balance are checked at admission, statically:
//! - every species in a reaction line must be declared in `species:`
//!   (world-closing; undeclared = `E-CHEM-SPECIES`, never implicit);
//! - element balance: summed atoms per element must match across the arrow
//!   (imbalance = `E-CHEM-BALANCE`); the balanced combustion line admits.

use emath_test_harness::{Probe, Source, boot};

#[test]
fn reaction_species_closure_and_balance() {
    boot();
    let mut p = Probe::new("reaction species closure and element balance refuse with typed codes");
    // 2H2 + O2 -> 2H2O balances 4 H and 2 O across the arrow, so it admits.
    Source::from_str(
        "balanced",
        "emath reaction_network HydrogenCombustion:\n    species:\n        H2\n        O2\n        H2O\n    reactions:\n        r1: 2H2 + O2 -> 2H2O\n",
    )
    .must_admit(&mut p);
    Source::from_workspace("tests/invalid/reaction_undeclared_species.emath")
        .must_refuse(&mut p, &["E-CHEM-SPECIES"]);
    Source::from_workspace("tests/invalid/reaction_imbalance.emath")
        .must_refuse(&mut p, &["E-CHEM-BALANCE"]);
    Source::from_workspace("tests/invalid/reaction_unknown_section.emath")
        .must_refuse(&mut p, &["E-KIND-027"]);
    Source::from_workspace("tests/invalid/reaction_nonbare_species.emath")
        .must_refuse(&mut p, &["E-CHEM-SPECIES"]);
    p.finish();
}
