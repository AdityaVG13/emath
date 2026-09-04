use emath_core::{FileId, limits::Limits};
use emath_syntax::{
    EXCLUDED_DOMAIN_FORMS, STAGE0_FORMS, forbidden_domain_matches, format_lossless, parse_lossless,
    unknown_glyphs,
};

#[test]
fn generic_capsule_and_program_forms_are_parse_stable() {
    let source = "emath feature Add:\n    schema: \"emath.feature-capsule\"\n    feature_id: \"std.capability.math.add\"\n\nemath function AddExact:\n    inputs:\n        left: Int\n        right: Int\n    definitions:\n        result = left + right\n";
    let first = parse_lossless(source, FileId(0), &Limits::default());
    assert!(
        !first.diagnostics.has_errors(),
        "{:?}",
        first.diagnostics.items()
    );
    let formatted = format_lossless(&first);
    let second = parse_lossless(&formatted, FileId(0), &Limits::default());
    assert!(!second.diagnostics.has_errors());
    assert_eq!(format_lossless(&second), formatted);
    assert!(STAGE0_FORMS.contains(&"generic-declaration"));
    assert!(STAGE0_FORMS.contains(&"generic-binder"));
}

#[test]
fn unknown_glyphs_preserve_exact_utf8_bytes_and_spans_without_meaning() {
    let source = "emath function f:\n    definitions:\n        result = left ⊛ right\n";
    let glyphs = unknown_glyphs(source);
    let glyph = glyphs.iter().find(|glyph| glyph.text == "⊛").unwrap();
    assert_eq!(&source[glyph.start as usize..glyph.end as usize], "⊛");
    assert_eq!(glyph.end - glyph.start, "⊛".len() as u32);
}

#[test]
fn limits_refuse_with_typed_diagnostics() {
    let tiny = Limits {
        max_source_bytes: 8,
        max_tokens: 8,
        max_nesting: 2,
    };
    let source = "emath function deeply_nested:\n    definitions:\n        result = (((1)))\n";
    let parsed = parse_lossless(source, FileId(0), &tiny);
    assert!(
        parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-116")
    );

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
    assert!(
        parsed
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-SYN-108")
    );
}

#[test]
fn structural_gate_detects_domain_named_nucleus_branches() {
    assert_eq!(EXCLUDED_DOMAIN_FORMS.len(), 12);
    assert_eq!(
        forbidden_domain_matches("match name { \"softmax\" => branch() }"),
        vec!["softmax"]
    );
    assert!(forbidden_domain_matches("registry.resolve(feature_id)").is_empty());
}
