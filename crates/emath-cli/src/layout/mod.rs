#![forbid(unsafe_code)]

//! Math layout graph plus structured LaTeX and PDF-fixture frontends.
//!
//! SG-11 imports a structured LaTeX subset into [`MathLayoutGraph`] and
//! lowers it to [`emath_genesis::BinderTerm`]. SG-12 extracts the same
//! graph from positioned-glyph fixtures, retaining spatial ambiguities
//! instead of resolving them.

pub mod graph;
pub mod latex;
pub mod pdf;

pub use graph::{
    LAYOUT_SCHEMA, LAYOUT_VERSION, LayoutContent, LayoutEdge, LayoutError, LayoutNode,
    MathLayoutGraph, NodeId, RetainedAmbiguity, SpatialRelation, UnloweredRegion, check_version,
};
pub use latex::{parse_latex, to_binder_term};
pub use pdf::{PdfPageFixture, PositionedGlyph, extract, reference_fixture};
