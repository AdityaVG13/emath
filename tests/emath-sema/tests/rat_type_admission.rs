//! `Rat` / `Rational` at type sites are admitted as `TypeNode::Rational`.

use emath_test_harness::{boot, Probe, Source};

#[test]
fn rat_type_sites_are_admitted() {
    boot();
    let mut p = Probe::new("Rat/Rational spellings admit as exact rational types");
    let result = Source::from_str(
        "rat-sites",
        "emath function F:\n    inputs:\n        a: Rat\n        b: Rational\n    outputs:\n        r: Rat\n    definitions:\n        r = a * b\n",
    )
    .must_admit(&mut p);
    let messages: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.to_string())
        .collect();
    p.demand(
        "no-subset-refusal",
        messages.iter().any(|message| {
            message.contains("E-TYPE-001") || message.contains("outside the current")
        }) == false,
        format!("Rat/Rational must not hit the subset refusal, got {messages:?}"),
    );
    p.finish();
}
