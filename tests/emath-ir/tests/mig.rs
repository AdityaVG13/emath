//! MIG intent-graph witnesses: presentation-only span changes do not
//! alter identity, semantic expression changes do, and every intent plane
//! is represented and owned by (reachable from) the declaration node.

use emath_core::{FileId, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, Visibility};
use emath_ir::expression::{BinaryOp, ExprNode, Literal};
use emath_ir::goal::CompileSpec;
use emath_ir::ids::{DeclarationId, ExprId, TypeId};
use emath_ir::package::Declaration;
use emath_ir::types::TypeNode;
use emath_ir::{Mig, MigNodeKind, SemanticPackage};
use std::collections::BTreeMap;

/// A package exercising every intent plane; `span_offset` shifts every
/// span to prove presentation-only changes do not alter identity.
fn package(span_offset: u32, literal_bits: u64) -> SemanticPackage {
    let span = Span::new(FileId(0), span_offset, span_offset + 1);
    let mut package = SemanticPackage::new();
    package.types.push(TypeNode::Float64);
    package
        .exprs
        .push(ExprNode::Variable(QualifiedName::single("scale")));
    package
        .exprs
        .push(ExprNode::Literal(Literal::FloatBits(literal_bits)));
    package.exprs.push(ExprNode::Binary {
        operation: BinaryOp::Greater,
        left: ExprId(0),
        right: ExprId(1),
    });
    package.expr_spans = vec![span; package.exprs.len()];
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Scorer"),
        kind: QualifiedName::single("policy"),
        kind_label: "policy".to_string(),
        inputs: vec![Field {
            name: "x".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Public,
            source: span,
        }],
        outputs: Vec::new(),
        state: vec![Field {
            name: "scale".to_string(),
            ty: TypeId(0),
            visibility: Visibility::Private,
            source: span,
        }],
        constructors: vec![Constructor {
            name: "new".to_string(),
            parameters: vec![],
            preconditions: vec![ExprId(2)],
            assignments: BTreeMap::from([("scale".to_string(), ExprId(0))]),
            postconditions: vec![ExprId(2)],
            defaults: BTreeMap::new(),
            error_type: None,
            is_public: true,
            source: span,
        }],
        definitions: BTreeMap::from([("y".to_string(), ExprId(2))]),
        invariants: vec![ExprId(2)],
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: span,
    });
    package
}

#[test]
fn identity_excludes_presentation_only_span_changes() {
    let original = Mig::from_package(&package(0, 0.0_f64.to_bits()));
    let reformatted = Mig::from_package(&package(9000, 0.0_f64.to_bits()));
    assert_eq!(original.identity(), reformatted.identity());
    assert_eq!(original.canonical(), reformatted.canonical());
}

#[test]
fn identity_detects_semantic_expression_change() {
    let zero = Mig::from_package(&package(0, 0.0_f64.to_bits()));
    let one = Mig::from_package(&package(0, 1.0_f64.to_bits()));
    assert_ne!(zero.identity(), one.identity());
}

#[test]
fn every_intent_plane_is_represented_and_owned_by_the_declaration() {
    let graph = Mig::from_package(&package(0, 0.0_f64.to_bits()));
    for kind in [
        MigNodeKind::Declaration,
        MigNodeKind::Input,
        MigNodeKind::State,
        MigNodeKind::Definition,
        MigNodeKind::Constructor,
        MigNodeKind::Obligation,
        MigNodeKind::Assignment,
        MigNodeKind::Invariant,
        MigNodeKind::CompileSpec,
    ] {
        assert!(
            graph.nodes.iter().any(|node| node.kind == kind),
            "plane {} missing from the intent graph",
            kind.name()
        );
    }
    // Spine property: every non-declaration node is reachable from a
    // declaration node through the edge list.
    let declaration = graph.nodes[0].id;
    assert_eq!(graph.nodes[0].kind, MigNodeKind::Declaration);
    let mut reachable = vec![false; graph.nodes.len()];
    reachable[declaration.0] = true;
    // Edges are emitted parent-before-child, one forward pass suffices.
    for edge in &graph.edges {
        if reachable[edge.from.0] {
            reachable[edge.to.0] = true;
        }
    }
    assert!(
        reachable.iter().all(|seen| *seen),
        "unreachable intent nodes: {:?}",
        graph
            .nodes
            .iter()
            .filter(|node| !reachable[node.id.0])
            .collect::<Vec<_>>()
    );
}
