//! Chemistry surface failure-first tests (04 §3.4 + §3.5; the
//! §3.6 `record … where` surface needs a cross-lane Declaration change and
//! is intentionally out of this test file).
//!
//! Contracts (each must FAIL against the pre-admission):
//! - §3.5: a named rate-law form (`v = michaelis_menten(Vmax, Km, [S])`)
//!   is admitted in a `rate:` section. The form is non-mass-action, so
//!   without a declared `assumptions:` section it carries a WARNING
//!   receipt (W-CHEM-RATELAW) — never a silent admit, never a refusal.
//! - §3.5: `assumptions: quasi_steady_state` is admitted as a declared
//!   approximation; with it present the warning receipt stays silent.
//! - §3.4 (context-scoped minimal): `[S]` inside rate-law arguments is
//!   the concentration-of-S reading when S is a declared species; an
//!   undeclared `[Q]` inside the rate context refuses E-NOTATION-AMBIG
//!   (outside `rate:`/reaction contexts `[x]` stays the list/index
//!   reading — the parser is untouched, so no other suite changes).

use emath_core::Severity;
use emath_test_harness::{Probe, Source, boot, error_codes};

const MM_PLAIN: &str = "\
emath reaction_network MichaelisMenten:
    species:
        S
        P
        E
        ES
    Vmax: Measured<Real> = 1.0(1)
    Km: Measured<Real> = 0.5(5)
    rate:
        v = michaelis_menten(Vmax, Km, [S])
    reactions:
        cat: ES -> E + P
";

const MM_ASSUMED: &str = "\
emath reaction_network MichaelisMentenAssumed:
    species:
        S
        P
        E
        ES
    Vmax: Measured<Real> = 1.0(1)
    Km: Measured<Real> = 0.5(5)
    rate:
        v = michaelis_menten(Vmax, Km, [S])
    assumptions:
        quasi_steady_state
    reactions:
        cat: ES -> E + P
";

#[test]
fn chemical_rate_law_contracts() {
    boot();
    let mut p = Probe::new("named rate laws admit with declared assumptions; brackets read by context");
    p.case("warning-receipt", |p| {
        // Non-mass-action without a declared assumption admits with the
        // W-CHEM-RATELAW warning receipt — never a silent admit, never a refusal.
        let plain = Source::from_str("mm-plain", MM_PLAIN).must_admit(&mut *p);
        let warns: Vec<&str> = plain
            .diagnostics
            .items()
            .iter()
            .filter(|diag| diag.severity == Severity::Warning)
            .map(|diag| diag.code)
            .collect();
        p.demand(
            "ratelaw-warns",
            warns.contains(&"W-CHEM-RATELAW"),
            format!("non-mass-action rate law without assumptions must warn W-CHEM-RATELAW, got {warns:?}"),
        );
        // Declared species brackets in the rate context read as
        // concentration — no ambiguity refusal.
        p.demand(
            "no-ambig",
            error_codes(&plain.diagnostics).iter().all(|code| *code != "E-NOTATION-AMBIG"),
            "declared species bracket in rate context must read as concentration",
        );
    });
    p.case("assumptions-silence-warning", |p| {
        let assumed = Source::from_str("mm-assumed", MM_ASSUMED).must_admit(&mut *p);
        let warns: Vec<&str> = assumed
            .diagnostics
            .items()
            .iter()
            .filter(|diag| diag.severity == Severity::Warning)
            .map(|diag| diag.code)
            .collect();
        p.eq("no-warnings", warns.len(), 0);
    });
    p.case("typed-refusals", |p| {
        // Undeclared species inside the rate-context bracket: no silent guessing.
        Source::from_workspace("tests/invalid/chemical_rate_law_ambiguous.emath")
            .must_refuse(&mut *p, &["E-NOTATION-AMBIG"]);
        // A `rate:` entry value that is not a numeric literal feeds the
        // honesty gate: nothing is guessed.
        Source::from_workspace("tests/invalid/chemical_rate_nonnumeric.emath")
            .must_refuse(&mut *p, &["E-KIND-027"]);
        // An `assumptions:` entry that is not a bare name is not a
        // declaration (declared approximations hash by name).
        Source::from_workspace("tests/invalid/chemical_assumption_malformed.emath")
            .must_refuse(&mut *p, &["E-KIND-027"]);
    });
    p.finish();
}
