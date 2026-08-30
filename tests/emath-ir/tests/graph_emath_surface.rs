//! Bead `emath-r2-graphs-masa` — acceptance: the runnable router
//! example and the human reference chapter.
//!
//! Failure-first: both tests were RED (example file did not exist;
//! reference chapter had no graphs section) before this pass; fixing
//! them is the user-visible gap of the bead. The executable .emath
//! graph surface itself is proven in
//! `tests/emath-sema/tests/graph_emath_surface.rs` (this package's
//! surface tests share the emath-cli dep; sema-tests is cli-free).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;

/// The runnable router example: the language truth for this bead.
const ROUTER_EXAMPLE: &str =
    include_str!("../../../language/examples/numerical/graph-router.emath");

/// The human reference chapter (graphs/Adjacency admission section).
const REFERENCE_CHAPTER: &str =
    include_str!("../../../language/reference/types-units-shapes-and-domains.md");

fn vector_eq(actual: &Value, want: &[f64]) {
    assert_eq!(actual, &Value::Vector(want.to_vec()), "vector mismatch");
}

/// The planted gap (was RED): the router example exists, checks clean,
/// and its graph surface computes the documented answers.
#[test]
fn graph_router_example_is_runnable() {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-router.emath", ROUTER_EXAMPLE);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "router example must admit: {errors:#?}");
    let values = eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("router example must evaluate: {fault}"));

    assert_eq!(
        values.get("g"),
        Some(&Value::Matrix {
            rows: 4,
            cols: 4,
            data: vec![
                0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0
            ],
        }),
        "example carrier"
    );
    vector_eq(values.get("r").expect("reachability"), &[1.0, 1.0, 1.0, 1.0]);
    vector_eq(values.get("b").expect("bfs order"), &[0.0, 1.0, 2.0, 3.0]);
    vector_eq(values.get("d").expect("distances"), &[0.0, 1.0, 1.0, 2.0]);
    vector_eq(values.get("o").expect("out degrees"), &[2.0, 1.0, 1.0, 0.0]);
}

/// The planted gap (was RED): the reference chapter documents the
/// graph carrier and the call surface.
#[test]
fn reference_documents_graph_admission() {
    assert!(
        REFERENCE_CHAPTER.contains("graph"),
        "reference chapter must document the graph carrier"
    );
    assert!(
        REFERENCE_CHAPTER.contains("reachability")
            && REFERENCE_CHAPTER.contains("shortest_distances"),
        "reference chapter must document the graph call names"
    );
}
