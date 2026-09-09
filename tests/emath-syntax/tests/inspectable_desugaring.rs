//! Inspectable desugaring across progressive-exactness levels.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::Item;
use emath_syntax::formatter::format;
use emath_syntax::{expand_scratch, parse_lossless, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn inspectable_desugaring() {
    boot();
    let mut probe = Probe::new("Inspectable desugaring across progressive-exactness levels.");
    probe.case("l0_expansion_is_visible_and_round_trips", |p| {
    let f0 = p.failures().len();

    let source = "2+2\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), stringify!(expansion.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("2",expansion.expanded.contains("emath function Scratch:"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",expansion
            .diagnostics
            .items()
            .iter()
            .any(|item| item.code == "N-SCRATCH-001"), format!(
        "desugar must not be silent"
    ));
    if p.failures().len() != f0 { return; }
    let parsed = parse_lossless(&expansion.expanded, FileId(0), &Limits::default());
    p.demand("4",!parsed.diagnostics.has_errors(), stringify!(!parsed.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let once = format(&parsed.tree, &parsed.comments);
    let twice = format(
        &parse_lossless(&once, FileId(0), &Limits::default()).tree,
        &[],
    );
    p.eq("5", &twice, &once);

    });
    probe.case("l0_and_l3_share_declaration_center", |p| {

    let l0 = parse_str("2+2\n").0;
    // Canonical no-input wrapped form: E-SEC-130 refuses a synthesized
    // `outputs:` without a declared I/O surface, so the L0 wrap (and this
    // literal) carry `definitions:` only.
    let l3 = parse_str("emath function Scratch:\n    definitions:\n        result = 2+2\n").0;
    let Item::Declaration(a) = &l0.items[0] else {
        panic!("l0");
    };
    let Item::Declaration(b) = &l3.items[0] else {
        panic!("l3");
    };
    p.eq("1", &a.name, &b.name);
    p.eq("2", a.body.len(), b.body.len());

    });
    probe.case("inspectable_example_file_expands", |p| {
    let f0 = p.failures().len();

    let source = "2 + 2\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), stringify!(expansion.rewritten()));
    if p.failures().len() != f0 { return; }
    p.demand("2",expansion.expanded.contains("result = 2 + 2"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    let (_, diagnostics) = parse_str(source);
    p.demand("3",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("hidden_desugar_is_e_syn_144", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/inspectable_desugaring.emath");
    p.demand("1",has_error(source, "E-SYN-144"), format!(
        "hidden desugar must refuse with E-SYN-144"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
