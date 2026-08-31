//! units tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::units::*;

/// Temperature dimension vector (the module's KELVIN spelling is private;
/// an embedder supplies its own dimension arrays).
const TEMPERATURE_DIMS: emath_core::units::Dims = [0, 0, 0, 0, 1, 0, 0];

#[test]
fn seed_resolves_temperature_family() {
    let table = seed_table();
    assert!(table.resolve("K").is_ok());
    assert!(table.resolve("degC").expect("degC").is_affine());
    assert!(table.resolve("degF").expect("degF").is_affine());
}

#[test]
fn c13_order_holds_in_conversion() {
    let table = seed_table();
    let fahrenheit = table.resolve("degF").expect("degF");
    assert!((fahrenheit.to_si(32.0) - 273.15).abs() < 1e-9);
}

#[test]
fn alias_cycle_is_refused_not_looped() {
    let mut table = UnitTable::new();
    table
        .declare_unit(UnitSpec::new("a", TEMPERATURE_DIMS, 1.0, 0.0))
        .expect("a");
    table.declare_alias("b", "a").expect("b->a");
    // Forcing a cycle: alias `a` onto `b` is refused because `a` is a
    // declared unit, so no cycle can be constructed through the public
    // API; resolution is therefore always terminating.
    assert!(table.declare_alias("a", "b").is_err());
    assert!(table.identity("b").is_ok());
}
