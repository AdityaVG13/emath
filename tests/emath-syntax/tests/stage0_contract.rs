use emath_core::{FileId, limits::Limits};
use emath_syntax::{
    EXCLUDED_DOMAIN_FORMS, STAGE0_FORMS, forbidden_domain_matches, format_lossless, parse_lossless,
    unknown_glyphs,
};

use emath_test_harness::{Probe, boot};

#[test]
fn stage0_contract() {
    boot();
    let mut probe = Probe::new("stage0_contract syntax surface");
    probe.case("generic_capsule_and_program_forms_are_parse_stable", |p| {
    let f0 = p.failures().len();

    let source = "emath feature Add:\n    schema: \"emath.feature-capsule\"\n    feature_id: \"std.capability.math.add\"\n\nemath function AddExact:\n    inputs:\n        left: Int\n        right: Int\n    definitions:\n        result = left + right\n";
    let first = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!first.diagnostics.has_errors(), format!(
        "{:?}",
        first.diagnostics.items()
    ));
    if p.failures().len() != f0 { return; }
    let formatted = format_lossless(&first);
    let second = parse_lossless(&formatted, FileId(0), &Limits::default());
    p.demand("2",!second.diagnostics.has_errors(), stringify!(!second.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.eq("3", format_lossless(&second), formatted);
    p.demand("4",STAGE0_FORMS.contains(&"generic-declaration"), stringify!(STAGE0_FORMS.contains(&"generic-declaration")));
    if p.failures().len() != f0 { return; }
    p.demand("5",STAGE0_FORMS.contains(&"generic-binder"), stringify!(STAGE0_FORMS.contains(&"generic-binder")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unknown_glyphs_preserve_exact_utf8_bytes_and_spans_without_meaning", |p| {
    let f0 = p.failures().len();

    let source = "emath function f:\n    definitions:\n        result = left ⊛ right\n";
    let glyphs = unknown_glyphs(source);
    let glyph = glyphs.iter().find(|glyph| glyph.text == "⊛").unwrap();
    p.demand("1", (&source[glyph.start as usize..glyph.end as usize]) == ("⊛"), format!("expected {:?}, got {:?}", ("⊛"), (&source[glyph.start as usize..glyph.end as usize])));
    if p.failures().len() != f0 { return; }
    p.demand("2", (glyph.end - glyph.start) == ("⊛".len() as u32), format!("expected {:?}, got {:?}", ("⊛".len() as u32), (glyph.end - glyph.start)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("limits_refuse_with_typed_diagnostics", |p| {
    let f0 = p.failures().len();

    let tiny = Limits {
        max_source_bytes: 8,
        max_tokens: 8,
        max_nesting: 2,
    };
    let source = "emath function deeply_nested:\n    definitions:\n        result = (((1)))\n";
    let parsed = parse_lossless(source, FileId(0), &tiny);
    p.demand("1",parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-116"), stringify!(parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-116")));
    if p.failures().len() != f0 { return; }

    let token_limited = Limits {
        max_source_bytes: 1024,
        max_tokens: 4,
        max_nesting: 16,
    };
    let parsed = parse_lossless(
        "emath function f:\n    definitions:\n        x = 1 + 2\n",
        FileId(0),
        &token_limited,
    );
    p.demand("2",parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-108"), stringify!(parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-108")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("structural_gate_detects_domain_named_nucleus_branches", |p| {
    let f0 = p.failures().len();

    p.eq("1", EXCLUDED_DOMAIN_FORMS.len(), 12);
    p.demand("2", (forbidden_domain_matches("match name { \"softmax\" => branch() }")) == (vec!["softmax"]), format!("expected {:?}, got {:?}", (vec!["softmax"]), (forbidden_domain_matches("match name { \"softmax\" => branch() }"))));
    if p.failures().len() != f0 { return; }
    p.demand("3",forbidden_domain_matches("registry.resolve(feature_id)").is_empty(), stringify!(forbidden_domain_matches("registry.resolve(feature_id)").is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
