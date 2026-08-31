//! Tests for pdf.rs, migrated out of production code.
//! All items under test are public crate surface.

use emath_layout::{PdfPageFixture, PositionedGlyph, extract, reference_fixture, SpatialRelation};

    #[test]
    fn pdf_reference_fixture_graph_id_deterministic() {
        let first = extract(&reference_fixture());
        let second = extract(&reference_fixture());
        assert_eq!(first.graph_id(), second.graph_id());
        assert_eq!(first.canonical(), second.canonical());
    }

    #[test]
    fn pdf_superscript_detection_emits_relation() {
        let graph = extract(&reference_fixture());
        assert!(
            graph
                .edges()
                .iter()
                .any(|edge| edge.relation == SpatialRelation::SuperscriptOf),
            "reference fixture must emit SuperscriptOf"
        );
    }

    #[test]
    fn pdf_ambiguous_band_retains_both_readings() {
        let fixture = PdfPageFixture {
            source_label: "ambiguous-band".to_string(),
            glyphs: vec![
                PositionedGlyph {
                    glyph: "x".to_string(),
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 1000,
                    font_size: 1000,
                },
                PositionedGlyph {
                    glyph: "2".to_string(),
                    x: 800,
                    y: 300,
                    width: 400,
                    height: 600,
                    font_size: 600,
                },
            ],
        };
        let graph = extract(&fixture);
        let amb = graph
            .ambiguities()
            .iter()
            .find(|item| item.reading_a == "superscript" && item.reading_b == "subscript")
            .expect("retained ambiguity");
        assert!(amb.reason.contains("20-45%"));
    }

    #[test]
    fn pdf_prose_only_has_zero_formula_regions() {
        let mut glyphs = Vec::new();
        let mut x = 0;
        for ch in ['H', 'e', 'l', 'l', 'o', 'w', 'o', 'r', 'l', 'd'] {
            glyphs.push(PositionedGlyph {
                glyph: ch.to_string(),
                x,
                y: 0,
                width: 700,
                height: 1000,
                font_size: 1000,
            });
            x += 800;
        }
        let fixture = PdfPageFixture {
            source_label: "prose-only".to_string(),
            glyphs,
        };
        let graph = extract(&fixture);
        assert_eq!(graph.formula_regions().count(), 0);
    }
