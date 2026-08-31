//! `emath-r3-units-ext-physics-8u7h`: core::units_ext and core::physics
//! (Phase 13-14).
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
use emath_syntax::install_source_parser;

fn check(source: &str) -> Vec<(String, String)> {
    install_source_parser();
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

mod r3_units_ext_physics {
    use super::*;

    // --- Rust-level pins: scales and dimension vectors are exact ----------

    #[test]
    fn astro_and_geodetic_scales_are_exact_by_definition() {
        let au = lookup_unit("AU").unwrap();
        assert_eq!(au.scale, 1.495_978_707e11, "IAU 2012 exact");
        assert_eq!(au.dimensions(), UnitDim::base(1, 0, 0, 0, 0, 0, 0));
        assert_eq!(lookup_unit("pc").unwrap().scale, 3.085_677_581_491_367_3e16);
        assert_eq!(lookup_unit("ly").unwrap().scale, 9.460_730_472_580_8e15);
        assert_eq!(lookup_unit("nmi").unwrap().scale, 1_852.0);
        assert_eq!(lookup_unit("mi").unwrap().scale, 1_609.344);
        assert_eq!(lookup_unit("ft").unwrap().scale, 0.3048);
    }

    #[test]
    fn angle_units_are_dimensionless_by_declaration() {
        for (spelling, scale) in [
            ("rad", 1.0),
            ("deg", std::f64::consts::PI / 180.0),
            ("arcmin", std::f64::consts::PI / 10_800.0),
            ("arcsec", std::f64::consts::PI / 648_000.0),
            ("grad", std::f64::consts::PI / 200.0),
            ("turn", 2.0 * std::f64::consts::PI),
        ] {
            let unit = lookup_unit(spelling).unwrap();
            assert_eq!(
                unit.dimensions(),
                UnitDim::one(),
                "{spelling} must carry the dimensionless vector (SI radian = m/m policy)"
            );
            assert!((unit.scale - scale).abs() < 1e-15, "{spelling} scale");
        }
    }

    #[test]
    fn si_prefixes_apply_systematically() {
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
            assert!((unit.scale - expected.scale * factor).abs() < expected.scale.abs() * 1e-9, "{spelling} scale");
            assert_eq!(unit.dimensions(), expected.dimensions(), "{spelling} dims");
        }
    }

    #[test]
    fn exact_spellings_win_over_prefix_fallback() {
        // kg is the SI base (1.0), NOT kilo-gram via `g` (which would also
        // land on 1.0 but must not be produced that way); ms and km keep
        // their historical names.
        assert_eq!(lookup_unit("kg").unwrap().scale, 1.0);
        assert_eq!(lookup_unit("ms").unwrap().scale, 1e-3);
        assert_eq!(lookup_unit("km").unwrap().scale, 1e3);
        assert_eq!(lookup_unit("MiB").unwrap().scale, 1_048_576.0);
    }

    #[test]
    fn rankine_extends_the_affine_temperature_family() {
        let deg_r = lookup_unit("degR").unwrap();
        assert_eq!(deg_r.dimensions(), UnitDim::base(0, 0, 0, 0, 1, 0, 0));
        assert!((deg_r.scale - 5.0 / 9.0).abs() < 1e-15);
        assert_eq!(deg_r.offset, 0.0, "Rankine is absolute: no affine offset");
    }

    #[test]
    fn electronvolt_is_the_exact_si2019_value() {
        let ev = lookup_unit("eV").unwrap();
        assert!((ev.scale - 1.602_176_634e-19).abs() < 1e-36);
        assert_eq!(ev.dimensions(), UnitDim::base(2, 1, -2, 0, 0, 0, 0));
    }

    // --- Typed refusal: currencies and time zones are packages, not core --

    #[test]
    fn currency_in_core_is_a_distinct_typed_refusal() {
        assert_eq!(errors_of(&fn_with_input("USD")), vec!["E-UNIT-CURRENCY-1".to_string()]);
        assert_eq!(errors_of(&fn_with_input("EUR")), vec!["E-UNIT-CURRENCY-1".to_string()]);
        assert_eq!(errors_of(&fn_with_input("UTC")), vec!["E-UNIT-CURRENCY-1".to_string()]);
    }

    #[test]
    fn currency_behind_a_prefix_keeps_the_policy_refusal() {
        // mUSD is still a currency: the policy refusal survives prefixing
        // instead of degrading to the generic unknown-unit miss.
        assert_eq!(errors_of(&fn_with_input("mUSD")), vec!["E-UNIT-CURRENCY-1".to_string()]);
    }

    #[test]
    fn unknown_units_still_miss_generically() {
        // The gate must not swallow genuine unknowns.
        assert_eq!(errors_of(&fn_with_input("Flurble")), vec!["E-UNIT-104".to_string()]);
    }

    // --- Surface: extended spellings admit through the annotation layer ---

    #[test]
    fn extended_annotations_admit() {
        for annotation in ["AU", "pc", "ly", "nmi", "mi", "ft", "rad", "deg", "arcsec", "nm", "kPa", "MJ", "degR", "C", "mol", "Pa"] {
            assert!(
                errors_of(&fn_with_input(annotation)).is_empty(),
                "`in {annotation}` must admit"
            );
        }
    }

    #[test]
    fn angle_dimension_policy_is_explicit_in_comparisons() {
        // deg and rad share the dimensionless vector: a dimension equality
        // between them computes true (admits with a receipt, no error).
        let source = "emath function A:\n    inputs:\n        x: Float64 in deg\n        y: Float64 in rad\n    outputs:\n        z: Float64\n    definitions:\n        z = 0.0\n    constraints:\n        dimension of x == dimension of y\n";
        assert!(
            errors_of(source).is_empty(),
            "deg and rad must compare dimension-equal under the declared policy"
        );
    }

    // --- core::physics: undirected relations over quantities --------------

    const VALID: &str = include_str!("../../../tests/valid/physics_units.emath");
    const INVALID: &str = include_str!("../../../tests/invalid/physics_units.emath");

    #[test]
    fn physics_law_contracts_admit_and_carry_the_relation() {
        let errors = errors_of(VALID);
        assert!(
            errors.is_empty(),
            "physics law contracts must admit: {errors:?}"
        );
    }

    #[test]
    fn seeded_wrong_output_refuses_with_dimension_mismatch() {
        // Velocity (m/s) where acceleration (m/s^2) belongs: the residual
        // `F - m*a` infers kg*m/s against the force output — a typed refusal
        // at admission, never a silently-true law.
        let errors = errors_of(INVALID);
        assert!(
            errors.iter().any(|code| code == "E-UNIT-101"),
            "seeded wrong-output must refuse with E-UNIT-101: {errors:?}"
        );
    }
}
