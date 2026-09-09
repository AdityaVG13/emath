//! Biology dynamics thin slice (04 §4.3–4.5).
//!
//! Contracts:
//! - §4.3 measured rate params admit through the EXISTING measurement
//!   machinery: `k_div = 0.30 ± 0.12 ~ lognormal` (central value lowers
//!   strict; uncertainty + distribution recorded loudly E-MEAS-003,
//!   provenance Unstated). The `in unit` spelling on a definition ROW
//!   is not Phase-1 surface and must refuse with a message that names
//!   the unit-carrying surfaces (never a silent drop).
//! - §4.4/§4.5 spellings (`propensity <Name>:`, `seed 0x2A`,
//!   `dose <Name>:`, `sample <Name>:`) refuse as typed diagnostics
//!   naming the bio field-pack follow-up — not the generic
//!   definitions-row shape error. The `events:` section (ch7) admits
//!   today; `on <trigger>:` rules are that lane's named slice.
//!
//! Failure-first evidence (live probes before this slice): propensity
//! and seed rows emitted the generic `E-SYN-101 only name = expression
//! definitions are allowed in Phase 1`; `k_div in 1/day = 0.30 ± 0.12
//! 1/day` was dropped SILENTLY by the parser (only a downstream
//! `E-TYPE-002 unknown variable k_div` appeared).

fn check(text: &str, name: &str) -> Vec<String> {
    Source::from_str(name, text)
        .check()
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

const MEASURED_RATE_PARAM: &str = "\
emath function BioRate:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        k_div = 0.30 ± 0.12 ~ lognormal
        y = k_div * x
";

const PROPENSITY_ROW: &str = "\
emath function BioPropensity:
    inputs:
        alpha: Float64

    outputs:
        y: Float64

    definitions:
        propensity Birth: alpha * 1.0
        y = alpha
";

const SEED_ROW: &str = "\
emath function BioSeed:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        seed 0x2A
        y = x
";

const DOSE_ROW: &str = "\
emath function BioDose:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        dose Dose1: x bolus 50 mg at t = 0 h
        y = x
";

const UNITS_ON_DEFINITION_ROW: &str = "\
emath function BioUnits:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        k_div in 1/day = 0.30 ± 0.12
        y = k_div * x
";

use emath_test_harness::{Probe, Source, boot};

#[test]
fn biology_dynamics() {
    boot();
    let mut probe = Probe::new("Biology dynamics thin slice (04 §4.3–4.5). Contracts: - §4.3 measured rate params admit through the EXISTING measurement machinery: `k_div = 0.30 ±");
    probe.case("measured_rate_param_with_distribution_tag_admits", |p| {
    let f0 = p.failures().len();

    let errors = check(MEASURED_RATE_PARAM, "bio-rate-admit");
    p.demand("1",errors.is_empty(), format!(
        "measured rate param must admit through the existing ± literal + \
         ~ lognormal tag machinery; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("propensity_row_refuses_naming_field_pack", |p| {
    let f0 = p.failures().len();

    let errors = check(PROPENSITY_ROW, "bio-propensity-fence");
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("field-pack") && e.contains("propensity")), format!(
        "propensity rows must refuse naming the bio field-pack slice; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("seed_row_refuses_naming_field_pack", |p| {
    let f0 = p.failures().len();

    let errors = check(SEED_ROW, "bio-seed-fence");
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("field-pack") && e.contains("seed")), format!(
        "seed rows must refuse naming the bio field-pack slice (mandatory \
         seed / E-SIM-SEED rides with it); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("dose_row_refuses_naming_field_pack", |p| {
    let f0 = p.failures().len();

    let errors = check(DOSE_ROW, "bio-dose-fence");
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("field-pack") && (e.contains("dose") || e.contains("schedule"))), format!(
        "dose schedule rows must refuse naming the bio field-pack slice; got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unit_carrying_definition_row_refuses_typed", |p| {
    let f0 = p.failures().len();

    let errors = check(UNITS_ON_DEFINITION_ROW, "bio-units-fence");
    // The row itself must refuse with a units message at its own site
    // (previously it was dropped silently — only a downstream
    // unknown-variable error appeared). The downstream `E-TYPE-002`
    // for the refused name is expected: the row did not bind, so its
    // uses are legitimately unknown.
    p.demand("1",errors
            .iter()
            .any(|e| e.contains("definition rows") && e.contains("unit")), format!(
        "a `name in unit = value` definition row must refuse with a units \
         message at its own site (never a silent drop); got: {errors:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
