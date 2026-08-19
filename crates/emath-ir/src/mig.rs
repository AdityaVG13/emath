//! MIG: the mathematical intent graph (schema `emath.mig`, version 1).
//!
//! The MIG is the spine layer between HIR admission and SIR semantics: a
//! deterministic graph of what the package *means to do*, with one node per
//! intent-bearing element (declaration, field, definition, constructor
//! obligation, goal, test, compile target, export) and typed edges from
//! each declaration to the intent it owns. Every one of the six semantic
//! planes (definition / construction / goal / evidence / execution /
//! evolution) is represented losslessly by node kinds.
//!
//! Semantic identity excludes presentation-only data by construction: no
//! span, comment or formatting information enters the graph; expression
//! content enters through `canonical_expr`, which is span-free.

use crate::canonical::canonical_expr;
use crate::package::{Declaration, SemanticPackage};
use emath_core::{ContentId, SchemaId};

/// Versioned MIG schema id.
pub const MIG_SCHEMA: &str = "emath.mig";
/// MIG schema version.
pub const MIG_SCHEMA_VERSION: u32 = 1;

/// MIG node id (index into `Mig::nodes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MigNodeId(pub usize);

/// The intent plane a node belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigNodeKind {
    /// The declaration itself (owner of every other node).
    Declaration,
    /// Definition plane: an input field.
    Input,
    /// Definition plane: an output field.
    Output,
    /// Definition plane: a state field.
    State,
    /// Definition plane: a named definition body.
    Definition,
    /// Construction plane: a constructor.
    Constructor,
    /// Construction plane: a constructor obligation (require/ensure).
    Obligation,
    /// Construction plane: a `Self:` field assignment.
    Assignment,
    /// Evidence plane: a declaration invariant.
    Invariant,
    /// Goal plane: a resolution goal.
    Goal,
    /// Evidence plane: an example test.
    Test,
    /// Execution plane: the compile specification.
    CompileSpec,
    /// Evolution plane: an export.
    Export,
}

impl MigNodeKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::Input => "input",
            Self::Output => "output",
            Self::State => "state",
            Self::Definition => "definition",
            Self::Constructor => "constructor",
            Self::Obligation => "obligation",
            Self::Assignment => "assignment",
            Self::Invariant => "invariant",
            Self::Goal => "goal",
            Self::Test => "test",
            Self::CompileSpec => "compile-spec",
            Self::Export => "export",
        }
    }
}

/// One intent node: kind, semantic label and optional expression content
/// (canonical, span-free).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigNode {
    /// Node id.
    pub id: MigNodeId,
    /// Intent plane.
    pub kind: MigNodeKind,
    /// Semantic label (name, target or specification token). Never a span.
    pub label: String,
    /// Canonical content id of the carried expression, when the node
    /// carries one.
    pub content: Option<ContentId>,
}

/// Typed intent edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigEdgeKind {
    /// Declaration owns a field / constructor / compile spec / export.
    Owns,
    /// Declaration defines a named body.
    Defines,
    /// Constructor requires an obligation before field init.
    Requires,
    /// Constructor (or declaration) ensures an obligation after init.
    Ensures,
    /// Constructor assigns a state field.
    Assigns,
    /// Declaration targets a goal.
    Targets,
    /// Declaration is checked by an example test.
    Tests,
}

impl MigEdgeKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Owns => "owns",
            Self::Defines => "defines",
            Self::Requires => "requires",
            Self::Ensures => "ensures",
            Self::Assigns => "assigns",
            Self::Targets => "targets",
            Self::Tests => "tests",
        }
    }
}

/// One edge in the intent graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigEdge {
    /// Source node.
    pub from: MigNodeId,
    /// Edge kind.
    pub kind: MigEdgeKind,
    /// Destination node.
    pub to: MigNodeId,
}

/// The mathematical intent graph of a semantic package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mig {
    /// Versioned schema id (`emath.mig.v1`).
    pub schema: SchemaId,
    /// Nodes in deterministic derivation order.
    pub nodes: Vec<MigNode>,
    /// Edges in deterministic derivation order.
    pub edges: Vec<MigEdge>,
}

impl Mig {
    /// Derives the intent graph from an admitted package. Derivation is
    /// deterministic: declaration order, field order, sorted definition /
    /// assignment maps, goal/test id order.
    #[must_use]
    pub fn from_package(package: &SemanticPackage) -> Self {
        let mut graph = Self {
            schema: SchemaId(format!("{MIG_SCHEMA}.v{MIG_SCHEMA_VERSION}")),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        for declaration in &package.declarations {
            graph.add_declaration(package, declaration);
        }
        graph
    }

    fn push_node(
        &mut self,
        kind: MigNodeKind,
        label: impl Into<String>,
        content: Option<ContentId>,
    ) -> MigNodeId {
        let id = MigNodeId(self.nodes.len());
        self.nodes.push(MigNode {
            id,
            kind,
            label: label.into(),
            content,
        });
        id
    }

    fn push_edge(&mut self, from: MigNodeId, kind: MigEdgeKind, to: MigNodeId) {
        self.edges.push(MigEdge { from, kind, to });
    }

    fn add_declaration(&mut self, package: &SemanticPackage, declaration: &Declaration) {
        let owner = self.push_node(MigNodeKind::Declaration, declaration.name.0.clone(), None);
        for (fields, kind) in [
            (&declaration.inputs, MigNodeKind::Input),
            (&declaration.outputs, MigNodeKind::Output),
            (&declaration.state, MigNodeKind::State),
        ] {
            for field in fields {
                let node = self.push_node(kind, field.name.clone(), None);
                self.push_edge(owner, MigEdgeKind::Owns, node);
            }
        }
        for (name, expr) in &declaration.definitions {
            let content = Some(canonical_expr(package, *expr));
            let node = self.push_node(MigNodeKind::Definition, name.clone(), content);
            self.push_edge(owner, MigEdgeKind::Defines, node);
        }
        for constructor in &declaration.constructors {
            let node = self.push_node(MigNodeKind::Constructor, constructor.name.clone(), None);
            self.push_edge(owner, MigEdgeKind::Owns, node);
            for precondition in &constructor.preconditions {
                let content = Some(canonical_expr(package, *precondition));
                let obligation = self.push_node(MigNodeKind::Obligation, "require", content);
                self.push_edge(node, MigEdgeKind::Requires, obligation);
            }
            for (field, expr) in &constructor.assignments {
                let content = Some(canonical_expr(package, *expr));
                let assignment = self.push_node(MigNodeKind::Assignment, field.clone(), content);
                self.push_edge(node, MigEdgeKind::Assigns, assignment);
            }
            for postcondition in &constructor.postconditions {
                let content = Some(canonical_expr(package, *postcondition));
                let obligation = self.push_node(MigNodeKind::Obligation, "ensure", content);
                self.push_edge(node, MigEdgeKind::Ensures, obligation);
            }
        }
        for invariant in &declaration.invariants {
            let content = Some(canonical_expr(package, *invariant));
            let node = self.push_node(MigNodeKind::Invariant, "invariant", content);
            self.push_edge(owner, MigEdgeKind::Ensures, node);
        }
        for goal_id in &declaration.goals {
            if let Some(goal) = package.goals.get(goal_id.index()) {
                let label = format!(
                    "{}:{}:{}",
                    goal.kind.as_str(),
                    goal.target,
                    goal.requirements.produce
                );
                let content = goal
                    .expression
                    .map(|expression| canonical_expr(package, expression));
                let node = self.push_node(MigNodeKind::Goal, label, content);
                self.push_edge(owner, MigEdgeKind::Targets, node);
            }
        }
        for test_id in &declaration.tests {
            if let Some(test) = package.tests.get(test_id.index()) {
                let content = Some(canonical_expr(package, test.expect));
                let node = self.push_node(MigNodeKind::Test, test.name.clone(), content);
                self.push_edge(owner, MigEdgeKind::Tests, node);
            }
        }
        let spec = &declaration.compile_spec;
        let compile_label = format!(
            "{}:{}:{}:{}",
            spec.target,
            spec.profile,
            spec.numeric.as_str(),
            spec.safety.as_str()
        );
        let compile = self.push_node(MigNodeKind::CompileSpec, compile_label, None);
        self.push_edge(owner, MigEdgeKind::Owns, compile);
        for export in &declaration.exports {
            let node = self.push_node(
                MigNodeKind::Export,
                format!("{}:{}", export.kind, export.name),
                None,
            );
            self.push_edge(owner, MigEdgeKind::Owns, node);
        }
    }

    /// Deterministic canonical encoding: schema header, one row per node
    /// (kind, label, content), one row per edge. Span-free by construction.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.schema.0);
        out.push('\n');
        for node in &self.nodes {
            out.push_str("node ");
            out.push_str(&node.id.0.to_string());
            out.push(' ');
            out.push_str(node.kind.name());
            out.push(' ');
            out.push_str(&node.label);
            out.push(' ');
            out.push_str(node.content.as_ref().map_or("-", |content| &content.0));
            out.push('\n');
        }
        for edge in &self.edges {
            out.push_str("edge ");
            out.push_str(&edge.from.0.to_string());
            out.push(' ');
            out.push_str(edge.kind.name());
            out.push(' ');
            out.push_str(&edge.to.0.to_string());
            out.push('\n');
        }
        out
    }

    /// Semantic identity of the intent graph (excludes presentation-only
    /// changes: spans and formatting never enter the derivation).
    #[must_use]
    pub fn identity(&self) -> ContentId {
        emath_core::hash::bootstrap_content_id(self.canonical().as_bytes())
    }
}

// MIG intent-graph tests moved to `tests/emath-ir/tests/mig.rs`.
