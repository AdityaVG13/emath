//! Tests for pdf.rs, migrated out of production code.
use emath_cli_lab::layout::{PdfPageFixture, PositionedGlyph, SpatialRelation, extract, reference_fixture};
use emath_test_harness::Probe;
fn glyph(g: &str, x: i64, y: i64, w: i64, h: i64, f: i64) -> PositionedGlyph { PositionedGlyph { glyph: g.to_string(), x, y, width: w, height: h, font_size: f } }
#[test]
fn probe() {
    let mut p = Probe::new("pdf extraction keeps relations, ambiguities, and prose silence");
    p.case("deterministic", |p| { let (a, b) = (extract(&reference_fixture()), extract(&reference_fixture())); p.eq("graph", a.graph_id(), b.graph_id()); p.eq("canonical", a.canonical(), b.canonical()); });
    p.case("superscript", |p| { p.demand("edge", extract(&reference_fixture()).edges().iter().any(|e| e.relation == SpatialRelation::SuperscriptOf), "fixture must emit SuperscriptOf"); });
    p.case("ambiguous", |p| { let g = extract(&PdfPageFixture { source_label: "ambiguous-band".into(), glyphs: vec![glyph("x", 0, 0, 800, 1000, 1000), glyph("2", 800, 300, 400, 600, 600)] }); let amb = g.ambiguities().iter().find(|i| i.reading_a == "superscript" && i.reading_b == "subscript").expect("ambiguity"); p.contains("band", &amb.reason, "20-45%"); });
    p.case("prose", |p| { let mut gs = vec![]; let mut x = 0; for ch in ['H', 'e', 'l', 'l', 'o', 'w', 'o', 'r', 'l', 'd'] { gs.push(glyph(&ch.to_string(), x, 0, 700, 1000, 1000)); x += 800; } p.eq("regions", extract(&PdfPageFixture { source_label: "prose-only".into(), glyphs: gs }).formula_regions().count(), 0); });
    p.finish();
}
