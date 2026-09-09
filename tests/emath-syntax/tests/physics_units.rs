//! core::units_ext and core::physics.
//!
//! core::units_ext: SI prefixes work systematically over every known
//! spelling, astronomical (AU/pc/ly) and geodetic (nmi/mi/ft) scales are
//! exact by definition, angle units are dimensionless BY DECLARATION
//! (the SI radian policy, made explicit), Rankine extends the affine
//! temperature family, and currencies/time zones are a TYPED refusal in
//! core (they live in versioned packages, never the nucleus) — the
//! refusal is `E-UNIT-CURRENCY-1`, distinct from the generic
//! `E-UNIT-104` unknown-unit miss.
//!
//! core::physics: law contracts are UNDIRECTED relations over
//! quantities. The relation is machine-checked through the quantity
//! types (`residual = F - m * a` infers the force dimension; a seeded
//! wrong-output using velocity where acceleration belongs is a typed
//! `E-UNIT-101` dimension mismatch at admission). CODATA constants enter
//! as measured inputs pinned per package version (core::codata, 04jc).
//!
//! Failure-first baseline: at authoring time `AU`, `deg`, `kPa`, `C`,
//! `USD` were all the generic `E-UNIT-104` miss (USD indistinguishable
//! from a typo); the physics laws did not admit. Every pin below was RED
//! until the lookup_unit extension landed.

use emath_ir::{UnitDim, lookup_unit};
use emath_sema::CompilerSession;

fn check(source: &str) -> Vec<(String, String)> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    session
        .check_owned("r3_units_ext_physics", source)
        .diagnostics
        .items()
        .iter()
        .map(|diagnostic| {
            (
                format!("{:?}", diagnostic.severity),
                diagnostic.code.to_string(),
            )
        })
        .collect()
}

fn fn_with_input(annotation: &str) -> String {
    format!(
        "emath function U:\n    inputs:\n        x: Float64 in {annotation}\n    outputs:\n        y: Float64\n    definitions:\n        y = 2.0\n"
    )
}

fn errors_of(source: &str) -> Vec<String> {
    check(source)
        .into_iter()
        .filter(|(severity, _)| severity == "Error")
        .map(|(_, code)| code)
        .collect()
}

// --- Rust-level pins: scales and dimension vectors are exact ----------

// --- Typed refusal: currencies and time zones are packages, not core --

// --- Surface: extended spellings admit through the annotation layer ---

// --- core::physics: undirected relations over quantities --------------

const VALID: &str = include_str!("../../../tests/valid/physics_units.emath");
const INVALID: &str = include_str!("../../../tests/invalid/physics_units.emath");

use emath_test_harness::{Probe, boot};

#[test]
fn physics_units() {
    boot();
    let mut probe = Probe::new("core::units_ext and core::physics. core::units_ext: SI prefixes work systematically over every known spelling, astronomical (AU/pc/ly)");
    probe.case("astro_and_geodetic_scales_are_exact_by_definition", |p| {

        let au = lookup_unit("AU").unwrap();
        p.eq("1", au.scale, 1.495_978_707e11);
        p.eq("2", au.dimensions(), UnitDim::base(1, 0, 0, 0, 0, 0, 0));
        p.eq("3", lookup_unit("pc").unwrap().scale, 3.085_677_581_491_367_3e16);
        p.eq("4", lookup_unit("ly").unwrap().scale, 9.460_730_472_580_8e15);
        p.eq("5", lookup_unit("nmi").unwrap().scale, 1_852.0);
        p.eq("6", lookup_unit("mi").unwrap().scale, 1_609.344);
        p.eq("7", lookup_unit("ft").unwrap().scale, 0.3048);
    
    });
    probe.case("angle_units_are_dimensionless_by_declaration", |p| {
    let f0 = p.failures().len();

        for (spelling, scale) in [
            ("rad", 1.0),
            ("deg", std::f64::consts::PI / 180.0),
            ("arcmin", std::f64::consts::PI / 10_800.0),
            ("arcsec", std::f64::consts::PI / 648_000.0),
            ("grad", std::f64::consts::PI / 200.0),
            ("turn", 2.0 * std::f64::consts::PI),
        ] {
            let unit = lookup_unit(spelling).unwrap();
            p.eq("1", unit.dimensions(), UnitDim::one());
            p.demand("2",(unit.scale - scale).abs() < 1e-15, format!( "{spelling} scale"));
            if p.failures().len() != f0 { return; }
        }
    
    });
    probe.case("si_prefixes_apply_systematically", |p| {
    let f0 = p.failures().len();

        for (spelling, base, factor) in [
            ("nm", "m", 1e-9),
            ("mm", "m", 1e-3),
            ("cm", "m", 1e-2),
            ("us", "s", 1e-6),
            ("ns", "s", 1e-9),
            ("mg", "g", 1e-3),
            ("kPa", "Pa", 1e3),
            ("MPa", "Pa", 1e6),
            ("MJ", "J", 1e6),
            ("mK", "K", 1e-3),
            ("mA", "A", 1e-3),
            ("mmol", "mol", 1e-3),
            ("nC", "C", 1e-9),
            ("kN", "N", 1e3),
            ("mV", "V", 1e-3),
        ] {
            let unit = lookup_unit(spelling).unwrap();
            let expected = lookup_unit(base).unwrap();
            p.demand("1",(unit.scale - expected.scale * factor).abs() < expected.scale.abs() * 1e-9, format!(
                "{spelling} scale"
            ));
            if p.failures().len() != f0 { return; }
            p.eq("2", unit.dimensions(), expected.dimensions());
        }
    
    });
    probe.case("exact_spellings_win_over_prefix_fallback", |p| {

        // kg is the SI base (1.0), NOT kilo-gram via `g` (which would also
        // land on 1.0 but must not be produced that way); ms and km keep
        // their historical names.
        p.eq("1", lookup_unit("kg").unwrap().scale, 1.0);
        p.eq("2", lookup_unit("ms").unwrap().scale, 1e-3);
        p.eq("3", lookup_unit("km").unwrap().scale, 1e3);
        p.eq("4", lookup_unit("MiB").unwrap().scale, 1_048_576.0);
    
    });
    probe.case("rankine_extends_the_affine_temperature_family", |p| {
    let f0 = p.failures().len();

        let deg_r = lookup_unit("degR").unwrap();
        p.eq("1", deg_r.dimensions(), UnitDim::base(0, 0, 0, 0, 1, 0, 0));
        p.demand("2",(deg_r.scale - 5.0 / 9.0).abs() < 1e-15, stringify!((deg_r.scale - 5.0 / 9.0).abs() < 1e-15));
        if p.failures().len() != f0 { return; }
        p.eq("3", deg_r.offset, 0.0);
    
    });
    probe.case("electronvolt_is_the_exact_si2019_value", |p| {
    let f0 = p.failures().len();

        let ev = lookup_unit("eV").unwrap();
        p.demand("1",(ev.scale - 1.602_176_634e-19).abs() < 1e-36, stringify!((ev.scale - 1.602_176_634e-19).abs() < 1e-36));
        if p.failures().len() != f0 { return; }
        p.eq("2", ev.dimensions(), UnitDim::base(2, 1, -2, 0, 0, 0, 0));
    
    });
    probe.case("currency_in_core_is_a_distinct_typed_refusal", |p| {
    let f0 = p.failures().len();

        p.demand("1", (errors_of(&fn_with_input("USD"))) == (vec!["E-UNIT-CURRENCY-1".to_string()]), format!("expected {:?}, got {:?}", (vec!["E-UNIT-CURRENCY-1".to_string()]), (errors_of(&fn_with_input("USD")))));
        if p.failures().len() != f0 { return; }
        p.demand("2", (errors_of(&fn_with_input("EUR"))) == (vec!["E-UNIT-CURRENCY-1".to_string()]), format!("expected {:?}, got {:?}", (vec!["E-UNIT-CURRENCY-1".to_string()]), (errors_of(&fn_with_input("EUR")))));
        if p.failures().len() != f0 { return; }
        p.demand("3", (errors_of(&fn_with_input("UTC"))) == (vec!["E-UNIT-CURRENCY-1".to_string()]), format!("expected {:?}, got {:?}", (vec!["E-UNIT-CURRENCY-1".to_string()]), (errors_of(&fn_with_input("UTC")))));
        if p.failures().len() != f0 { return; }
    
    });
    probe.case("currency_behind_a_prefix_keeps_the_policy_refusal", |p| {
    let f0 = p.failures().len();

        // mUSD is still a currency: the policy refusal survives prefixing
        // instead of degrading to the generic unknown-unit miss.
        p.demand("1", (errors_of(&fn_with_input("mUSD"))) == (vec!["E-UNIT-CURRENCY-1".to_string()]), format!("expected {:?}, got {:?}", (vec!["E-UNIT-CURRENCY-1".to_string()]), (errors_of(&fn_with_input("mUSD")))));
        if p.failures().len() != f0 { return; }
    
    });
    probe.case("unknown_units_still_miss_generically", |p| {
    let f0 = p.failures().len();

        // The gate must not swallow genuine unknowns.
        p.demand("1", (errors_of(&fn_with_input("Flurble"))) == (vec!["E-UNIT-104".to_string()]), format!("expected {:?}, got {:?}", (vec!["E-UNIT-104".to_string()]), (errors_of(&fn_with_input("Flurble")))));
        if p.failures().len() != f0 { return; }
    
    });
    probe.case("extended_annotations_admit", |p| {
    let f0 = p.failures().len();

        for annotation in [
            "AU", "pc", "ly", "nmi", "mi", "ft", "rad", "deg", "arcsec", "nm", "kPa", "MJ", "degR",
            "C", "mol", "Pa",
        ] {
            p.demand("1",errors_of(&fn_with_input(annotation)).is_empty(), format!(
                "`in {annotation}` must admit"
            ));
            if p.failures().len() != f0 { return; }
        }
    
    });
    probe.case("angle_dimension_policy_is_explicit_in_comparisons", |p| {
    let f0 = p.failures().len();

        // deg and rad share the dimensionless vector: a dimension equality
        // between them computes true (admits with a receipt, no error).
        let source = "emath function A:\n    inputs:\n        x: Float64 in deg\n        y: Float64 in rad\n    outputs:\n        z: Float64\n    definitions:\n        z = 0.0\n    constraints:\n        dimension of x == dimension of y\n";
        p.demand("1",errors_of(source).is_empty(), format!(
            "deg and rad must compare dimension-equal under the declared policy"
        ));
        if p.failures().len() != f0 { return; }
    
    });
    probe.case("physics_law_contracts_admit_and_carry_the_relation", |p| {
    let f0 = p.failures().len();

        let errors = errors_of(VALID);
        p.demand("1",errors.is_empty(), format!(
            "physics law contracts must admit: {errors:?}"
        ));
        if p.failures().len() != f0 { return; }
    
    });
    probe.case("seeded_wrong_output_refuses_with_dimension_mismatch", |p| {
    let f0 = p.failures().len();

        // Velocity (m/s) where acceleration (m/s^2) belongs: the residual
        // `F - m*a` infers kg*m/s against the force output — a typed refusal
        // at admission, never a silently-true law.
        let errors = errors_of(INVALID);
        p.demand("1",errors.iter().any(|code| code == "E-UNIT-101"), format!(
            "seeded wrong-output must refuse with E-UNIT-101: {errors:?}"
        ));
        if p.failures().len() != f0 { return; }
    
    });
    probe.finish();
}
