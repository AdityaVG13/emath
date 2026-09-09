use emath_test_harness::{Probe, workspace_path};

#[test]
fn msc_field_pack_dumps_do_not_ship_under_language_spec() {
    let mut p = Probe::new("MSC2020 catalog dumps stay out of language/spec");
    let relative = "language/spec/field_packs/msc2020.emath";
    p.demand(relative, !workspace_path(relative).exists(), "must not ship");
    p.finish();
}
