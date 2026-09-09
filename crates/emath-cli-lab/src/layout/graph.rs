//! Shared math layout graph (schema `emath.math-layout-graph`).

use std::fmt::{self, Write as _};

use emath_core::fnv1a64_bytes;

/// Schema id; matches the disclosed `emath-schema` registry string.
pub const LAYOUT_SCHEMA: &str = "emath.math-layout-graph";
/// Layout graph schema version. Bump on any change to the canonical
/// encoding; consumers refuse versions they do not know.
pub const LAYOUT_VERSION: u32 = 1;

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), LayoutError> {
    if version == LAYOUT_VERSION {
        Ok(())
    } else {
        Err(LayoutError::UnknownVersion { version })
    }
}

/// Stable node identity within one graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// One layout node with a byte span into [`MathLayoutGraph::source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutNode {
    /// Stable identity.
    pub id: NodeId,
    /// Structural content.
    pub content: LayoutContent,
    /// Inclusive-start, exclusive-end byte span into the retained source.
    pub source_span: (usize, usize),
}

/// Node payload for the structured math subset.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutContent {
    /// A named glyph (letter, digit, operator, or Greek name).
    Glyph(String),
    /// Horizontal grouping.
    Row,
    /// Superscript wrapper.
    Superscript,
    /// Subscript wrapper.
    Subscript,
    /// Fraction wrapper.
    Fraction,
    /// Radical wrapper.
    Radical,
    /// Big operator whose name is a binder kind (`sum`, `product`, ...).
    BigOp(String),
    /// A detected formula region.
    FormulaRegion,
}

/// How a child is placed relative to its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpatialRelation {
    /// Child sits to the right of the parent.
    RightOf,
    /// Child sits above the parent.
    Above,
    /// Child sits below the parent.
    Below,
    /// Child is a superscript of the parent.
    SuperscriptOf,
    /// Child is a subscript of the parent.
    SubscriptOf,
    /// Child is contained by the parent.
    Contains,
}

impl SpatialRelation {
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::RightOf => "right-of",
            Self::Above => "above",
            Self::Below => "below",
            Self::SuperscriptOf => "superscript-of",
            Self::SubscriptOf => "subscript-of",
            Self::Contains => "contains",
        }
    }
}

/// Directed parent-child edge labeled with a spatial relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEdge {
    /// Parent node.
    pub parent: NodeId,
    /// Child node.
    pub child: NodeId,
    /// Spatial placement of `child` relative to `parent`.
    pub relation: SpatialRelation,
}

/// An ambiguity retained on the graph instead of being resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedAmbiguity {
    /// Node the two readings attach to.
    pub node_id: NodeId,
    /// First reading (e.g. `"superscript"`).
    pub reading_a: String,
    /// Second reading (e.g. `"subscript"`).
    pub reading_b: String,
    /// Why both readings were kept.
    pub reason: String,
}

/// A formula region that could not be lowered to a binder term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloweredRegion {
    /// Formula-region node.
    pub node_id: NodeId,
    /// Typed reason; never a fabricated term.
    pub reason: String,
}

/// Typed refusals for layout construction, parsing, and lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// Token outside the structured subset.
    UnexpectedToken {
        /// Offending token text.
        token: String,
        /// Byte offset in the source.
        offset: usize,
    },
    /// A `$` opener with no matching closer.
    UnterminatedDollar {
        /// Byte offset of the opener.
        offset: usize,
    },
    /// A `\[` opener with no matching `\]`.
    UnterminatedDisplay {
        /// Byte offset of the opener.
        offset: usize,
    },
    /// Macro outside the structured subset.
    UnknownMacro {
        /// Macro name without the leading backslash.
        name: String,
        /// Byte offset of the backslash.
        offset: usize,
    },
    /// Graph retained, but no binder/term lowering is licensed.
    Unlowered {
        /// Why lowering was refused.
        reason: String,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVersion { version } => {
                write!(f, "unknown layout version {version}")
            }
            Self::UnexpectedToken { token, offset } => {
                write!(f, "unexpected token {token:?} at byte {offset}")
            }
            Self::UnterminatedDollar { offset } => {
                write!(f, "unterminated $ at byte {offset}")
            }
            Self::UnterminatedDisplay { offset } => {
                write!(f, "unterminated \\[ at byte {offset}")
            }
            Self::UnknownMacro { name, offset } => {
                write!(f, "unknown macro \\{name} at byte {offset}")
            }
            Self::Unlowered { reason } => write!(f, "unlowered: {reason}"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Ordered math layout graph with retained source and ambiguities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathLayoutGraph {
    nodes: Vec<LayoutNode>,
    edges: Vec<LayoutEdge>,
    ambiguities: Vec<RetainedAmbiguity>,
    unlowered: Vec<UnloweredRegion>,
    source: String,
}

impl MathLayoutGraph {
    /// Original source bytes retained for the preservation law.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Nodes in stable id order.
    #[must_use]
    pub fn nodes(&self) -> &[LayoutNode] {
        &self.nodes
    }

    /// Edges in stable (parent, child, relation) order.
    #[must_use]
    pub fn edges(&self) -> &[LayoutEdge] {
        &self.edges
    }

    /// Retained ambiguities; never silently resolved.
    #[must_use]
    pub fn ambiguities(&self) -> &[RetainedAmbiguity] {
        &self.ambiguities
    }

    /// Formula regions that were not lowered to a term.
    #[must_use]
    pub fn unlowered(&self) -> &[UnloweredRegion] {
        &self.unlowered
    }

    /// Formula-region nodes.
    pub fn formula_regions(&self) -> impl Iterator<Item = &LayoutNode> {
        self.nodes
            .iter()
            .filter(|node| matches!(node.content, LayoutContent::FormulaRegion))
    }

    /// Deterministic versioned canonical text encoding.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{LAYOUT_SCHEMA}.v{LAYOUT_VERSION}");
        let _ = writeln!(out, "source:{}", escape(&self.source));
        for node in &self.nodes {
            let _ = writeln!(
                out,
                "node:{}:{}:{}:{}",
                node.id.0,
                node.source_span.0,
                node.source_span.1,
                content_canonical(&node.content)
            );
        }
        for edge in &self.edges {
            let _ = writeln!(
                out,
                "edge:{}:{}:{}",
                edge.parent.0,
                edge.child.0,
                edge.relation.canonical()
            );
        }
        for amb in &self.ambiguities {
            let _ = writeln!(
                out,
                "amb:{}:{}:{}:{}",
                amb.node_id.0,
                escape(&amb.reading_a),
                escape(&amb.reading_b),
                escape(&amb.reason)
            );
        }
        for region in &self.unlowered {
            let _ = writeln!(
                out,
                "unlowered:{}:{}",
                region.node_id.0,
                escape(&region.reason)
            );
        }
        out
    }

    /// FNV-1a64 over the versioned canonical form (single owner:
    /// Tier-0 core).
    #[must_use]
    pub fn graph_id(&self) -> u64 {
        fnv1a64_bytes(self.canonical().as_bytes())
    }

    pub(crate) fn retain_unlowered(&mut self, node_id: NodeId, reason: String) {
        self.unlowered.push(UnloweredRegion { node_id, reason });
        self.unlowered.sort_by_key(|region| region.node_id);
    }

    pub(crate) fn node(&self, id: NodeId) -> Option<&LayoutNode> {
        self.nodes
            .binary_search_by_key(&id, |node| node.id)
            .ok()
            .map(|index| &self.nodes[index])
    }

    pub(crate) fn related(&self, parent: NodeId, relation: SpatialRelation) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = self
            .edges
            .iter()
            .filter(|edge| edge.parent == parent && edge.relation == relation)
            .map(|edge| edge.child)
            .collect();
        ids.sort_by_key(|id| {
            self.node(*id)
                .map_or((usize::MAX, id.0), |node| (node.source_span.0, id.0))
        });
        ids
    }

    pub(crate) fn is_script_target(&self, id: NodeId) -> bool {
        self.edges.iter().any(|edge| {
            edge.child == id
                && matches!(
                    edge.relation,
                    SpatialRelation::SuperscriptOf | SpatialRelation::SubscriptOf
                )
        })
    }
}

pub(crate) struct GraphBuilder {
    nodes: Vec<LayoutNode>,
    edges: Vec<LayoutEdge>,
    ambiguities: Vec<RetainedAmbiguity>,
    unlowered: Vec<UnloweredRegion>,
    source: String,
    next_id: u64,
}

impl GraphBuilder {
    pub(crate) fn new(source: String) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            ambiguities: Vec::new(),
            unlowered: Vec::new(),
            source,
            next_id: 1,
        }
    }

    pub(crate) fn add_node(
        &mut self,
        content: LayoutContent,
        source_span: (usize, usize),
    ) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.push(LayoutNode {
            id,
            content,
            source_span,
        });
        id
    }

    pub(crate) fn add_edge(&mut self, parent: NodeId, child: NodeId, relation: SpatialRelation) {
        self.edges.push(LayoutEdge {
            parent,
            child,
            relation,
        });
    }

    pub(crate) fn add_ambiguity(
        &mut self,
        node_id: NodeId,
        reading_a: impl Into<String>,
        reading_b: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.ambiguities.push(RetainedAmbiguity {
            node_id,
            reading_a: reading_a.into(),
            reading_b: reading_b.into(),
            reason: reason.into(),
        });
    }

    pub(crate) fn finish(mut self) -> MathLayoutGraph {
        self.nodes.sort_by_key(|node| node.id);
        self.edges.sort_by(|left, right| {
            (left.parent, left.child, left.relation.canonical()).cmp(&(
                right.parent,
                right.child,
                right.relation.canonical(),
            ))
        });
        self.ambiguities.sort_by(|left, right| {
            (
                left.node_id,
                left.reading_a.as_str(),
                left.reading_b.as_str(),
            )
                .cmp(&(
                    right.node_id,
                    right.reading_a.as_str(),
                    right.reading_b.as_str(),
                ))
        });
        self.unlowered.sort_by_key(|region| region.node_id);
        MathLayoutGraph {
            nodes: self.nodes,
            edges: self.edges,
            ambiguities: self.ambiguities,
            unlowered: self.unlowered,
            source: self.source,
        }
    }
}

fn content_canonical(content: &LayoutContent) -> String {
    match content {
        LayoutContent::Glyph(text) => format!("glyph({})", escape(text)),
        LayoutContent::Row => "row".to_string(),
        LayoutContent::Superscript => "superscript".to_string(),
        LayoutContent::Subscript => "subscript".to_string(),
        LayoutContent::Fraction => "fraction".to_string(),
        LayoutContent::Radical => "radical".to_string(),
        LayoutContent::BigOp(name) => format!("bigop({})", escape(name)),
        LayoutContent::FormulaRegion => "formula-region".to_string(),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ',' => out.push_str("\\,"),
            ':' => out.push_str("\\:"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}
