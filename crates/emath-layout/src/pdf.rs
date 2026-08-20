//! PDF positioned-glyph fixture frontend (SG-12).

use std::fmt::Write as _;

use crate::graph::{
    GraphBuilder, LayoutContent, LayoutError, MathLayoutGraph, NodeId, SpatialRelation,
};
use crate::latex::to_binder_term;

/// One positioned glyph in milli-units (integers, so Eq/Hash stay exact).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PositionedGlyph {
    /// Glyph text (one character or a named operator such as `Σ`).
    pub glyph: String,
    /// Horizontal origin, milli-units.
    pub x: i64,
    /// Vertical origin, milli-units (larger is higher, PDF-style).
    pub y: i64,
    /// Advance width, milli-units.
    pub width: i64,
    /// Box height, milli-units.
    pub height: i64,
    /// Nominal font size, milli-units.
    pub font_size: i64,
}

/// A page of positioned glyphs plus a stable label (not a PDF binary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageFixture {
    /// Glyphs in any order; extraction sorts deterministically.
    pub glyphs: Vec<PositionedGlyph>,
    /// Stable fixture identity retained as the graph source header.
    pub source_label: String,
}

/// The supplied 2D formula fixture: `E = Σ_{i=1}^{3} i²`.
#[must_use]
pub fn reference_fixture() -> PdfPageFixture {
    PdfPageFixture {
        source_label: "reference-2d-sum".to_string(),
        glyphs: vec![
            glyph("E", 0, 0, 800, 1000, 1000),
            glyph("=", 1200, 0, 600, 1000, 1000),
            glyph("Σ", 2400, 0, 900, 1000, 1000),
            glyph("i", 2400, -600, 400, 600, 600),
            glyph("=", 2800, -600, 400, 600, 600),
            glyph("1", 3200, -600, 400, 600, 600),
            glyph("3", 3000, 600, 400, 600, 600),
            glyph("i", 4000, 0, 400, 1000, 1000),
            glyph("2", 4400, 600, 400, 600, 600),
        ],
    }
}

/// Extract a layout graph from a positioned-glyph fixture.
///
/// Ambiguous y-offsets are retained, never resolved. Where the formula does
/// not lower into the structured subset, the region is still emitted and an
/// [`LayoutError::Unlowered`] reason is retained on the graph.
#[must_use]
pub fn extract(fixture: &PdfPageFixture) -> MathLayoutGraph {
    let source = fixture_source(fixture);
    let mut builder = GraphBuilder::new(source);
    if fixture.glyphs.is_empty() {
        return builder.finish();
    }

    let node_ids: Vec<NodeId> = fixture
        .glyphs
        .iter()
        .map(|item| {
            builder.add_node(
                glyph_content(&item.glyph),
                (0, item.glyph.len()),
            )
        })
        .collect();

    let max_font = fixture
        .glyphs
        .iter()
        .map(|item| item.font_size)
        .max()
        .unwrap_or(1)
        .max(1);
    let baseline_cut = max_font * 80 / 100;

    let mut bases: Vec<usize> = fixture
        .glyphs
        .iter()
        .enumerate()
        .filter(|(_, item)| item.font_size >= baseline_cut)
        .map(|(index, _)| index)
        .collect();
    if bases.is_empty() {
        bases = (0..fixture.glyphs.len()).collect();
    }

    let mut attachments: Vec<Attachment> = vec![Attachment::Inline; fixture.glyphs.len()];
    for index in 0..fixture.glyphs.len() {
        if bases.contains(&index) {
            continue;
        }
        let Some(base) = nearest_base(&fixture.glyphs, &bases, index) else {
            continue;
        };
        let child = &fixture.glyphs[index];
        let parent = &fixture.glyphs[base];
        let offset = (child.y - parent.y).abs();
        attachments[index] = classify_offset(offset, child.y, parent.y, parent.font_size, child.font_size);
        match attachments[index] {
            Attachment::Super => {
                builder.add_edge(node_ids[base], node_ids[index], SpatialRelation::SuperscriptOf);
            }
            Attachment::Sub => {
                builder.add_edge(node_ids[base], node_ids[index], SpatialRelation::SubscriptOf);
            }
            Attachment::Ambiguous => {
                builder.add_ambiguity(
                    node_ids[index],
                    "superscript",
                    "subscript",
                    "y-offset in 20-45% font-size band",
                );
            }
            Attachment::Inline => {}
        }
    }

    let mut line_of = vec![0_usize; fixture.glyphs.len()];
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let band = max_font / 2;
    let mut sorted_bases = bases.clone();
    sorted_bases.sort_by_key(|&index| (fixture.glyphs[index].y, fixture.glyphs[index].x, index));
    for index in sorted_bases {
        let y = fixture.glyphs[index].y;
        if let Some(line) = lines.last_mut() {
            let anchor = fixture.glyphs[line[0]].y;
            if (y - anchor).abs() <= band {
                let line_index = line_of[line[0]];
                line.push(index);
                line_of[index] = line_index;
                continue;
            }
        }
        line_of[index] = lines.len();
        lines.push(vec![index]);
    }
    for (index, _) in fixture.glyphs.iter().enumerate() {
        if bases.contains(&index) {
            continue;
        }
        let Some(base) = nearest_base(&fixture.glyphs, &bases, index) else {
            continue;
        };
        line_of[index] = line_of[base];
    }
    for line in &mut lines {
        let members: Vec<usize> = fixture
            .glyphs
            .iter()
            .enumerate()
            .filter(|(index, _)| line_of[*index] == line_of[line[0]])
            .map(|(index, _)| index)
            .collect();
        *line = members;
        line.sort_by_key(|&index| (fixture.glyphs[index].x, index));
    }

    let gap = max_font * 2;
    for line in &lines {
        let runs = split_runs(fixture, line, gap);
        for run in runs {
            let is_formula = run.iter().any(|&index| {
                is_math_seed(&fixture.glyphs[index].glyph)
                    || !matches!(attachments[index], Attachment::Inline)
            });
            if !is_formula {
                continue;
            }
            let top: Vec<usize> = run
                .iter()
                .copied()
                .filter(|&index| {
                    matches!(attachments[index], Attachment::Inline | Attachment::Ambiguous)
                })
                .collect();
            if top.is_empty() {
                continue;
            }
            let region = builder.add_node(LayoutContent::FormulaRegion, (0, 0));
            let row = builder.add_node(LayoutContent::Row, (0, 0));
            builder.add_edge(region, row, SpatialRelation::Contains);
            let mut previous: Option<NodeId> = None;
            for index in top {
                builder.add_edge(row, node_ids[index], SpatialRelation::Contains);
                if let Some(prev) = previous {
                    builder.add_edge(prev, node_ids[index], SpatialRelation::RightOf);
                }
                previous = Some(node_ids[index]);
            }
        }
    }

    let mut graph = builder.finish();
    let region_id = graph.formula_regions().next().map(|region| region.id);
    if region_id.is_some() {
        if let Err(LayoutError::Unlowered { reason }) = to_binder_term(&graph) {
            if let Some(id) = region_id {
                graph.retain_unlowered(id, reason);
            }
        }
    }
    graph
}

fn glyph(text: &str, x: i64, y: i64, width: i64, height: i64, font_size: i64) -> PositionedGlyph {
    PositionedGlyph {
        glyph: text.to_string(),
        x,
        y,
        width,
        height,
        font_size,
    }
}

fn fixture_source(fixture: &PdfPageFixture) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", fixture.source_label);
    for item in &fixture.glyphs {
        let _ = writeln!(
            out,
            "g:{}:{}:{}:{}:{}:{}",
            item.glyph, item.x, item.y, item.width, item.height, item.font_size
        );
    }
    out
}

fn glyph_content(text: &str) -> LayoutContent {
    match text {
        "Σ" | "∑" => LayoutContent::BigOp("sum".to_string()),
        "∏" | "Π" => LayoutContent::BigOp("product".to_string()),
        "∫" => LayoutContent::BigOp("integral".to_string()),
        "→" => LayoutContent::Glyph("to".to_string()),
        other => LayoutContent::Glyph(other.to_string()),
    }
}

fn is_math_seed(text: &str) -> bool {
    matches!(
        text,
        "=" | "+" | "-" | "*" | "/" | "Σ" | "∑" | "∏" | "Π" | "∫"
    )
}

fn nearest_base(glyphs: &[PositionedGlyph], bases: &[usize], index: usize) -> Option<usize> {
    let x = glyphs[index].x;
    bases.iter().copied().min_by_key(|base| {
        ((glyphs[*base].x - x).abs(), glyphs[*base].x, *base)
    })
}

#[derive(Debug, Clone, Copy)]
enum Attachment {
    Super,
    Sub,
    Ambiguous,
    Inline,
}

fn classify_offset(
    offset: i64,
    child_y: i64,
    parent_y: i64,
    parent_font: i64,
    child_font: i64,
) -> Attachment {
    let font = parent_font.max(1);
    if offset * 100 >= font * 20 && offset * 100 < font * 45 {
        return Attachment::Ambiguous;
    }
    if offset * 100 >= font * 45 && child_font < parent_font {
        if child_y > parent_y {
            Attachment::Super
        } else {
            Attachment::Sub
        }
    } else {
        Attachment::Inline
    }
}

fn split_runs(fixture: &PdfPageFixture, line: &[usize], gap: i64) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for &index in line {
        if let Some(run) = runs.last_mut() {
            let prev = *run.last().expect("run non-empty");
            let dx = fixture.glyphs[index].x
                - (fixture.glyphs[prev].x + fixture.glyphs[prev].width);
            if dx <= gap {
                run.push(index);
                continue;
            }
        }
        runs.push(vec![index]);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::{PositionedGlyph, PdfPageFixture, extract, reference_fixture};
    use crate::graph::SpatialRelation;

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
}
