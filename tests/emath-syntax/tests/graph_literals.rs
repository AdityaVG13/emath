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

fn parse_defn(content: &str) -> Result<Expr, String> {
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

use emath_test_harness::{Probe, boot};

#[test]
fn graph_literals() {
    boot();
    let mut probe = Probe::new("executable graph literals (B23). `graph { <nodes> ; <edges> }` is a parse-time desugar to a tuple shape lowered to a weighted adjacency matrix:");
    probe.case("directed_edge_desugars_with_default_weight", |p| {
    let f0 = p.failures().len();

    let graph = parse_defn("graph { 1, 2; 1 --> 2 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!(
            "graph literal desugars to (nodes, edges), got {:?}",
            graph.kind
        );
    };
    p.eq("1", top.len(), 2);
    let ExprKind::List(nodes) = &top[0].kind else {
        panic!("nodes list, got {:?}", top[0].kind);
    };
    p.eq("2", nodes.len(), 2);
    let ExprKind::List(edges) = &top[1].kind else {
        panic!("edges list, got {:?}", top[1].kind);
    };
    let ExprKind::List(edge) = &edges[0].kind else {
        panic!("edge triple, got {:?}", edges[0].kind);
    };
    p.eq("3", edge.len(), 4);
    p.demand("4",matches!(&edge[0].kind, ExprKind::Int(text) if text == "1"), stringify!(matches!(&edge[0].kind, ExprKind::Int(text) if text == "1")));
    if p.failures().len() != f0 { return; }
    p.demand("5",matches!(&edge[1].kind, ExprKind::Int(text) if text == "2"), stringify!(matches!(&edge[1].kind, ExprKind::Int(text) if text == "2")));
    if p.failures().len() != f0 { return; }
    p.demand("6",is_float(&edge[2], "1.0"), format!( "default weight"));
    if p.failures().len() != f0 { return; }
    p.demand("7",is_float(&edge[3], "1.0"), format!( "directed flag"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("weighted_directed_edge_carries_weight", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",is_float(&edge[2], "2.5"), format!( "declared weight"));
    if p.failures().len() != f0 { return; }
    p.demand("2",is_float(&edge[3], "1.0"), format!( "directed"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("undirected_edges_carry_zero_flag", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",is_float(&first[2], "1.0") && is_float(&first[3], "0.0"), stringify!(is_float(&first[2], "1.0") && is_float(&first[3], "0.0")));
    if p.failures().len() != f0 { return; }
    let ExprKind::List(second) = &edges[1].kind else {
        panic!("edge 1");
    };
    p.demand("2",is_float(&second[2], "3.0") && is_float(&second[3], "0.0"), stringify!(is_float(&second[2], "3.0") && is_float(&second[3], "0.0")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("edgeless_graph_admits", |p| {
    let f0 = p.failures().len();

    // Nodes only: the edges list is empty, not absent.
    let graph = parse_defn("graph { 1, 2 }").unwrap();
    let ExprKind::Tuple(top) = &graph.kind else {
        panic!("desugar shape");
    };
    p.eq("1", top.len(), 2);
    p.demand("2",matches!(&top[1].kind, ExprKind::List(items) if items.is_empty()), stringify!(matches!(&top[1].kind, ExprKind::List(items) if items.is_empty())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("graph_literal_admits_end_to_end", |p| {
    let f0 = p.failures().len();

    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned(
        "graph-literals",
        "emath function net:\n    definitions:\n        g = graph { 1, 2, 3; 1 --> 2, 2 -[0.5]-> 3, 1 -[2.0]- 3 }\n",
    );
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("graph evaluates");
    p.eq("2", values.get("g"), Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.0, 1.0, 2.0, 0.0, 0.0, 0.5, 2.0, 0.0, 0.0],
        }));

    });
    probe.case("single_arrow_is_not_an_edge", |p| {
    let f0 = p.failures().len();

    // G4 guard: the statement/lambda arrow `->` is NOT an edge
    // operator inside a graph literal — the edge spellings are `-->`,
    // `-[w]->`, `-`, `-[w]-`.
    let error = parse_defn("graph { 1, 2; 1 -> 2 }").unwrap_err();
    p.demand("1",error.contains("E-SYN-101"), format!(
        "`->` must refuse in edge position, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("edge_spelling_requires_edge_section", |p| {
    let f0 = p.failures().len();

    // Edge syntax exists ONLY between `;` and `}`: in the node section
    // `1 --> 2` is a syntax error, not a graph with zero nodes.
    let error = parse_defn("graph { 1 --> 2 }").unwrap_err();
    p.demand("1",error.contains("E-SYN-101"), format!(
        "edge spelling without `;` must refuse E-SYN-101, got {error}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("malformed_edge_refuses", |p| {
    let f0 = p.failures().len();

    for spelling in [
        "graph { 1, 2; 1 --> }",
        "graph { 1, 2; 1 -[]-> 2 }",
        "graph { 1, 2; 1 -[2.5-> 3 }",
    ] {
        let error = parse_defn(spelling).unwrap_err();
        p.demand("1",error.contains("E-SYN-101"), format!(
            "{spelling} must refuse E-SYN-101, got {error}"
        ));
        if p.failures().len() != f0 { return; }
    }

    });
    probe.case("double_minus_outside_braces_still_arithmetic", |p| {
    let f0 = p.failures().len();

    // G4 regression guard: `x--y` OUTSIDE a graph literal is binary
    // minus + unary negation (no `--` token was glued).
    let graph = parse_defn("x--y").map(|_| ());
    p.demand("1",graph.is_err() || true, format!( "shape check below"));
    if p.failures().len() != f0 { return; }
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n        y: Float64\n\n    definitions:\n        f = x--y\n",
    );
    p.demand("2",!diags.has_errors(), format!(
        "`x--y` must stay arithmetic, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }
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
    p.demand("3",matches!(
            &value.kind,
            ExprKind::Binary { op: emath_core::tree::BinaryOp::Sub, right, .. }
                if matches!(&right.kind, ExprKind::Unary { op: emath_core::tree::UnaryOp::Neg, .. })
        ), format!(
        "`x--y` must be Sub(Neg), got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("graph_literal_fixture_executes_graph_algorithms", |p| {
    let f0 = p.failures().len();

    use emath_exec_ir::interp::Value;
    use std::collections::BTreeMap;

    let mut session = CompilerSession::new(Limits::default());
    let source = include_str!("../../../tests/fixtures/language/intro/graph-literals.emath");
    let checked = session.check_owned("graph-literal-example", source);
    let codes: Vec<&str> = checked
        .diagnostics
        .errors()
        .map(|error| error.code)
        .collect();
    p.demand("1",codes.is_empty(), format!(
        "graph fixture must typecheck, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }
    let values = emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("graph algorithms evaluate directly over a graph literal");
    p.eq("2", values.get("reachable"), Some(&Value::Vector(vec![1.0, 1.0, 1.0])));
    p.eq("3", values.get("traversal"), Some(&Value::Vector(vec![0.0, 1.0, 2.0])));
    p.eq("4", values.get("distances"), Some(&Value::Vector(vec![0.0, 1.0, 1.5])));

    });
    probe.finish();
}
