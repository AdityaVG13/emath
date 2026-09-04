//!: executable graph literals (B23).
//!
//! `graph { <nodes> ; <edges> }` is a parse-time desugar to a tuple
//! shape lowered to a weighted adjacency matrix: `[nodes…, edges…]`,
//! where each edge is
//! `[from, to, weight, directed]` (weight defaults to 1.0; directed is
//! 1.0 for `-->`/`-[w]->` and 0.0 for the undirected `-`/`-[w]-`
//! spellings). Edge operands are postfix expressions (compound nodes
//! need parentheses); edge syntax exists ONLY between the `;` and the
//! closing brace, so `x--y` outside braces is untouched arithmetic
//! (G4).
//!
//! Failure-first: RED until the `graph {` contextual arm + EdgeArrow
//! land (`-->` previously lexed Minus+Arrow and refused).

use emath_core::limits::Limits;
use emath_core::tree::{Expr, ExprKind, StmtKind};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn parse_defn(content: &str) -> Result<Expr, String> {
    install_source_parser();
    let source = format!("emath function f:\n    definitions:\n        g = {content}\n");
    let (tree, diags) = emath_syntax::parse_str(&source);
    if diags.has_errors() {
        return Err(diags
            .errors()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("; "));
    }
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        return Err("no declaration".into());
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .ok_or("no definitions")?;
    for stmt in &defs.suite.statements {
        if let StmtKind::Assign { value, .. } = &stmt.kind {
            return Ok(value.clone());
        }
    }
    Err("no assignment".into())
}

fn is_float(expr: &Expr, text: &str) -> bool {
    matches!(&expr.kind, ExprKind::Float(value) if value == text)
}

#[test]
fn directed_edge_desugars_with_default_weight() {
    let graph = parse_defn("graph { 1, 2; 1 --> 2 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!(
            "graph literal desugars to (nodes, edges), got {:?}",
            graph.kind
        );
    };
    assert_eq!(top.len(), 2);
    let ExprKind::List(nodes) = &top[0].kind else {
        panic!("nodes list, got {:?}", top[0].kind);
    };
    assert_eq!(nodes.len(), 2);
    let ExprKind::List(edges) = &top[1].kind else {
        panic!("edges list, got {:?}", top[1].kind);
    };
    let ExprKind::List(edge) = &edges[0].kind else {
        panic!("edge triple, got {:?}", edges[0].kind);
    };
    assert_eq!(edge.len(), 4);
    assert!(matches!(&edge[0].kind, ExprKind::Int(text) if text == "1"));
    assert!(matches!(&edge[1].kind, ExprKind::Int(text) if text == "2"));
    assert!(is_float(&edge[2], "1.0"), "default weight");
    assert!(is_float(&edge[3], "1.0"), "directed flag");
}

#[test]
fn weighted_directed_edge_carries_weight() {
    let graph = parse_defn("graph { 1, 3; 1 -[2.5]-> 3 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!("desugar shape");
    };
    let ExprKind::List(edges) = &top[1].kind else {
        panic!("edges list");
    };
    let ExprKind::List(edge) = &edges[0].kind else {
        panic!("edge");
    };
    assert!(is_float(&edge[2], "2.5"), "declared weight");
    assert!(is_float(&edge[3], "1.0"), "directed");
}

#[test]
fn undirected_edges_carry_zero_flag() {
    // Bare `-` and weighted `-[w]-` are the undirected spellings.
    let graph = parse_defn("graph { 1, 2, 3; 1 - 2, 1 -[3.0]- 3 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!("desugar shape");
    };
    let ExprKind::List(edges) = &top[1].kind else {
        panic!("edges list");
    };
    let ExprKind::List(first) = &edges[0].kind else {
        panic!("edge 0");
    };
    assert!(is_float(&first[2], "1.0") && is_float(&first[3], "0.0"));
    let ExprKind::List(second) = &edges[1].kind else {
        panic!("edge 1");
    };
    assert!(is_float(&second[2], "3.0") && is_float(&second[3], "0.0"));
}

#[test]
fn edgeless_graph_admits() {
    // Nodes only: the edges list is empty, not absent.
    let graph = parse_defn("graph { 1, 2 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!("desugar shape");
    };
    assert_eq!(top.len(), 2);
    assert!(matches!(&top[1].kind, ExprKind::List(items) if items.is_empty()));
}

#[test]
fn graph_literal_admits_end_to_end() {
    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(
        "graph-literals",
        "emath function net:\n    definitions:\n        g = graph { 1, 2, 3; 1 --> 2, 2 -[0.5]-> 3, 1 -[2.0]- 3 }\n",
    );
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("graph evaluates");
    assert_eq!(
        values.get("g"),
        Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.0, 1.0, 2.0, 0.0, 0.0, 0.5, 2.0, 0.0, 0.0],
        })
    );
}

#[test]
fn single_arrow_is_not_an_edge() {
    // G4 guard: the statement/lambda arrow `->` is NOT an edge
    // operator inside a graph literal — the edge spellings are `-->`,
    // `-[w]->`, `-`, `-[w]-`.
    let error = parse_defn("graph { 1, 2; 1 -> 2 }").unwrap_err();
    assert!(
        error.contains("E-SYN-101"),
        "`->` must refuse in edge position, got {error}"
    );
}

#[test]
fn edge_spelling_requires_edge_section() {
    // Edge syntax exists ONLY between `;` and `}`: in the node section
    // `1 --> 2` is a syntax error, not a graph with zero nodes.
    let error = parse_defn("graph { 1 --> 2 }").unwrap_err();
    assert!(
        error.contains("E-SYN-101"),
        "edge spelling without `;` must refuse E-SYN-101, got {error}"
    );
}

#[test]
fn malformed_edge_refuses() {
    for spelling in [
        "graph { 1, 2; 1 --> }",
        "graph { 1, 2; 1 -[]-> 2 }",
        "graph { 1, 2; 1 -[2.5-> 3 }",
    ] {
        let error = parse_defn(spelling).unwrap_err();
        assert!(
            error.contains("E-SYN-101"),
            "{spelling} must refuse E-SYN-101, got {error}"
        );
    }
}

#[test]
fn double_minus_outside_braces_still_arithmetic() {
    // G4 regression guard: `x--y` OUTSIDE a graph literal is binary
    // minus + unary negation (no `--` token was glued).
    let graph = parse_defn("x--y").map(|_| ());
    assert!(graph.is_err() || true, "shape check below");
    install_source_parser();
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n        y: Float64\n\n    definitions:\n        f = x--y\n",
    );
    assert!(
        !diags.has_errors(),
        "`x--y` must stay arithmetic, got {diags:?}"
    );
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("decl");
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .expect("definitions");
    let StmtKind::Assign { value, .. } = &defs.suite.statements[0].kind else {
        panic!("assign");
    };
    assert!(
        matches!(
            &value.kind,
            ExprKind::Binary { op: emath_core::tree::BinaryOp::Sub, right, .. }
                if matches!(&right.kind, ExprKind::Unary { op: emath_core::tree::UnaryOp::Neg, .. })
        ),
        "`x--y` must be Sub(Neg), got {:?}",
        value.kind
    );
}

#[test]
fn graph_literal_fixture_executes_graph_algorithms() {
    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = include_str!("../../../tests/fixtures/language/intro/graph-literals.emath");
    let checked = session.check_owned("graph-literal-example", source);
    let codes: Vec<&str> = checked
        .diagnostics
        .errors()
        .map(|error| error.code)
        .collect();
    assert!(
        codes.is_empty(),
        "graph fixture must typecheck, got {codes:?}"
    );
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("graph algorithms evaluate directly over a graph literal");
    assert_eq!(
        values.get("reachable"),
        Some(&Value::Vector(vec![1.0, 1.0, 1.0]))
    );
    assert_eq!(
        values.get("traversal"),
        Some(&Value::Vector(vec![0.0, 1.0, 2.0]))
    );
    assert_eq!(
        values.get("distances"),
        Some(&Value::Vector(vec![0.0, 1.0, 1.5]))
    );
}
