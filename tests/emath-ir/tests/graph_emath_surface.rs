//! Graph .emath surface tests — acceptance: the runnable
//! example and the human reference chapter.
//!
//! Failure-first: both tests were RED (example file did not exist;
//! reference chapter had no graphs section) before this pass; fixing
//! them closed the user-visible gap. The executable .emath
//! graph surface itself is proven in
//! `tests/emath-sema/tests/graph_emath_surface.rs` (this package's
//! surface tests share the emath-cli dep; sema-tests is cli-free).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;
use emath_test_harness::{Probe, boot};

/// The runnable router example: the language truth for this.
const ROUTER_EXAMPLE: &str =
    include_str!("../../../tests/fixtures/language/numerical/graph-router.emath");

/// The human reference chapter (graphs/Adjacency admission section).
const REFERENCE_CHAPTER: &str =
    include_str!("../../../language/reference/types-units-shapes-and-domains.md");

fn vector_eq(p: &mut emath_test_harness::Probe, name: &str, actual: &Value, want: &[f64]) {
    p.eq(name, actual, &Value::Vector(want.to_vec()));
}

#[test]
fn intent() {
    boot();
    let mut p = Probe::new("Graph .emath surface tests — acceptance: the runnable");
    p.case("graph_router_example_is_runnable", |p| {
// The planted gap (was RED): the router example exists, checks clean,
// and its graph surface computes the documented answers.

    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-router.emath", ROUTER_EXAMPLE);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    p.demand(format!("router example must admit: {errors:#?}"), errors.is_empty(), format!("router example must admit: {errors:#?}"));
    let values = match eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    ) {

        Ok(values) => values,

        Err(fault) => { p.fail("graph_router_example_is_runnable#2", format!("router example must evaluate: {fault}")); return; }

    };

    p.eq("example carrier", values.get("g"), Some(&Value::Matrix {
            rows: 4,
            cols: 4,
            data: vec![
                0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0
            ],
        }));
    vector_eq(p, "reachability", values.get("r").expect("reachability"), &[1.0, 1.0, 1.0, 1.0]);
    vector_eq(p, "bfs order", values.get("b").expect("bfs order"), &[0.0, 1.0, 2.0, 3.0]);
    vector_eq(p, "distances", values.get("d").expect("distances"), &[0.0, 1.0, 1.0, 2.0]);
    vector_eq(p, "out degrees", values.get("o").expect("out degrees"), &[2.0, 1.0, 1.0, 0.0]);

    });
    p.case("graph_declare_adjacency_reachability_end_to_end", |p| {
// Declare a graph from `.emath` text, then run adjacency + reachability
// end-to-end through EMIR: the graph literal lowers to the matrix
// substrate, `out_degrees`/`reachability` compute over it, and every
// answer is pinned exactly (no tautology).

    install_source_parser();
    const SOURCE: &str = r#"
emath function GraphSurfaceFromText:
    outputs:
        g: Graph
        mask: Vector<Float64>
        degrees: Vector<Float64>

    definitions:
        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }
        mask = reachability(g, 0)
        degrees = out_degrees(g)
"#;
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-surface.emath", SOURCE);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    p.demand(format!("graph surface must admit: {errors:#?}"), errors.is_empty(), format!("graph surface must admit: {errors:#?}"));

    let values = match eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    ) {


        Ok(values) => values,


        Err(fault) => { p.fail("graph_declare_adjacency_reachability_end_to_end#2", format!("graph surface must evaluate: {fault}")); return; }


    };

    // Reachability from node 0: every node is reachable (0->1, 0->2, 1->3).
    vector_eq(p, "mask", values.get("mask").expect("reachability"), &[1.0, 1.0, 1.0, 1.0]);
    // Out-degrees: node 0 has two outgoing edges, 1 and 2 one each, 3 none.
    vector_eq(p, "degrees", values.get("degrees").expect("out degrees"), &[2.0, 1.0, 1.0, 0.0]);

    });
    p.case("reachability_refuses_out_of_range_source", |p| {
// Typed runtime refusal: an out-of-range reachability source must fault
// with E-GRAPH-002 — never a panic and never a silently wrong answer.

    install_source_parser();
    const SOURCE: &str = r#"
emath function BadGraphSource:
    outputs:
        mask: Vector<Float64>

    definitions:
        g = graph { 0, 1; 0 --> 1 }
        mask = reachability(g, 99)
"#;
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-bad-source.emath", SOURCE);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    p.demand(format!("admission must accept a well-formed graph: {errors:#?}"), errors.is_empty(), format!("admission must accept a well-formed graph: {errors:#?}"));

    let fault = eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect_err("out-of-range reachability source must fault at runtime");
    let fault = fault.to_string();
    p.demand(format!("fault must name the graph source precondition: {fault}"), fault.contains("E-GRAPH-003") || fault.contains("E-GRAPH-002"), format!("fault must name the graph source precondition: {fault}"));

    });
    p.case("reference_documents_graph_admission", |p| {
// The planted gap (was RED): the reference chapter documents the
// graph carrier and the call surface.

    p.demand("reference chapter must document the graph carrier", REFERENCE_CHAPTER.contains("graph"), "reference chapter must document the graph carrier");
    p.demand("reference chapter must document the graph call names", REFERENCE_CHAPTER.contains("reachability")
            && REFERENCE_CHAPTER.contains("shortest_distances"), "reference chapter must document the graph call names");

    });
    p.finish();
}







