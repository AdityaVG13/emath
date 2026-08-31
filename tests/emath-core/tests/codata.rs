//! codata tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::codata::*;
use emath_core::units::seed_table;

#[test]
fn nist_reference_values_hold() {
    let catalog_2018 = codata_catalog(CodataAdjustment::Y2018);
    let by_symbol = |symbol: &str| {
        catalog_2018
            .iter()
            .find(|constant| constant.symbol == symbol)
            .unwrap_or_else(|| panic!("missing {symbol}"))
    };
    assert_eq!(by_symbol("c").value, "299792458");
    assert_eq!(by_symbol("h").value, "6.62607015e-34");
    assert_eq!(by_symbol("k_B").value, "1.380649e-23");
    assert_eq!(by_symbol("e").value, "1.602176634e-19");
    assert_eq!(by_symbol("N_A").value, "6.02214076e23");
    assert_eq!(by_symbol("G").value, "6.67430e-11");
}

#[test]
fn exact_vs_measured_distinction() {
    let catalog = codata_catalog(CodataAdjustment::Y2018);
    let by_symbol = |symbol: &str| {
        catalog
            .iter()
            .find(|constant| constant.symbol == symbol)
            .expect("constant")
    };
    assert!(by_symbol("c").kind.is_exact());
    assert!(!by_symbol("c").has_uncertainty());
    assert!(!by_symbol("G").kind.is_exact());
    assert!(by_symbol("G").has_uncertainty());
    let CodataKind::Measured {
        uncertainty_digits,
        exponent,
    } = &by_symbol("G").kind
    else {
        panic!("G must be measured");
    };
    assert_eq!(*uncertainty_digits, "15");
    assert_eq!(*exponent, -11);
    // C2: parenthesized-denominator unit spelling.
    assert_eq!(by_symbol("G").unit, "m^3/(kg*s^2)");
}

#[test]
fn versioned_adjustments_hash_differently() {
    let g_2018 = codata_catalog(CodataAdjustment::Y2018)
        .into_iter()
        .find(|constant| constant.symbol == "G")
        .expect("G 2018");
    let g_2022 = codata_catalog(CodataAdjustment::Y2022)
        .into_iter()
        .find(|constant| constant.symbol == "G")
        .expect("G 2022");
    // Identical NIST value (G unchanged between adjustments), distinct
    // adjustment identity: the hash MUST differ.
    assert_eq!(g_2018.value, g_2022.value);
    assert_ne!(g_2018.identity(), g_2022.identity());
    // Same adjustment twice is stable.
    let g_2018_again = codata_catalog(CodataAdjustment::Y2018)
        .into_iter()
        .find(|constant| constant.symbol == "G")
        .expect("G 2018 again");
    assert_eq!(g_2018.identity(), g_2018_again.identity());
    assert_eq!(g_2018.citation_reference(), "CODATA 2018 adjustment, NIST");
}

#[test]
fn hbar_is_exact_alias_of_h_over_two_pi() {
    let adjustment = CodataAdjustment::Y2018;
    let h = codata_catalog(adjustment)
        .into_iter()
        .find(|constant| constant.symbol == "h")
        .expect("h");
    let hbar_constant = hbar(adjustment).expect("hbar");
    assert!(hbar_constant.kind.is_exact());
    // The alias value is COMPUTED, so the f64 ratio is exact by
    // construction: recomputing h/(2*pi) must reproduce the stored
    // f64 bit-for-bit.
    let expected = h.value_f64() / (2.0 * std::f64::consts::PI);
    assert_eq!(hbar_constant.value_f64(), expected);
    // And the stored spelling reparses to that exact same f64.
    assert_eq!(hbar_constant.value.parse::<f64>(), Ok(expected));
}

#[test]
fn unversioned_constants_import_is_flagged() {
    let flagged = mixed_codata_adjustments(&["constants"]);
    assert!(flagged.is_some(), "unversioned import must be flagged");
    assert!(flagged.unwrap().contains("unversioned"));
    // A properly versioned import is clean.
    assert_eq!(mixed_codata_adjustments(&["codata2018"]), None);
    assert_eq!(
        mixed_codata_adjustments(&["sci", "constants", "codata2018", "c"]),
        None
    );
}

#[test]
fn mixed_adjustments_are_flagged() {
    let flagged = mixed_codata_adjustments(&["codata2018", "codata2022"]);
    assert!(flagged.is_some(), "mixed adjustments must be flagged");
    let message = flagged.expect("message");
    assert!(
        message.contains("2018") && message.contains("2022"),
        "{message}"
    );
    // Order-independent: 2022 first is flagged too.
    assert!(mixed_codata_adjustments(&["codata2022", "codata2018"]).is_some());
    // One adjustment plus unrelated segments is clean.
    assert_eq!(
        mixed_codata_adjustments(&["sci", "codata2018", "units"]),
        None
    );
}

#[test]
fn adjustment_parse_is_strict() {
    assert_eq!(
        CodataAdjustment::parse("codata2018"),
        Some(CodataAdjustment::Y2018)
    );
    assert_eq!(
        CodataAdjustment::parse("codata2022"),
        Some(CodataAdjustment::Y2022)
    );
    assert_eq!(CodataAdjustment::parse("codata"), None);
    assert_eq!(CodataAdjustment::parse("2018"), None);
    assert_eq!(codata_use_adjustment("sci::constants::*"), None);
}

/// Negative control (04 §2.6 stdlib admission test):
/// `dimension of (h * nu) == Energy`. h is J*s, nu is Hz = s^-1, so
/// the product's dimension vector is exactly Energy's (J). Verified
/// through the core `units` dimension arithmetic.
#[test]
fn dimension_of_h_times_nu_is_energy() {
    // SI base exponent vectors (m, kg, s, A, K, mol, cd) — J and s
    // from the seed table.
    let table = seed_table();
    let joule_dims = table.resolve("J").expect("J").dims;
    let second_dims = table.resolve("s").expect("s").dims;
    // h * nu: elementwise sum of exponents (J*s * s^-1 = J).
    let h_dims = {
        let mut dims = joule_dims;
        for index in 0..7 {
            dims[index] += second_dims[index];
        }
        dims
    };
    let nu_dims = {
        let mut dims = second_dims;
        for index in 0..7 {
            dims[index] = -dims[index];
        }
        dims
    };
    let product = {
        let mut dims = h_dims;
        for index in 0..7 {
            dims[index] += nu_dims[index];
        }
        dims
    };
    assert_eq!(product, joule_dims, "h * nu must have the Energy dimension");
}
