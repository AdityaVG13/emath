//! — the executable .emath graph surface:
//! dense carriers (graph literals), the closed graph call names
//! (`reachability`, `bfs_order`, `shortest_distances`, `out_degrees`,
//! `graph_laplacian`, `graph_symmetrize`, `bellman_ford`,
//! `sparse_triplets`, `sparse_from_triplets`), typed refusals,
//! deterministic ordering, and vertex-relabel metamorphic laws.
//!
//! The kernels are proven in `tests/emath-ir/tests/*`; these tests
//! prove the USER surface end to end from `.emath` source. The
//! runnable example + reference chapter acceptance live in
//! `tests/emath-ir/tests/graph_emath_surface.rs` (cli-dependent
//! package); this package is cli-free so the surface runs even while
//! the calibration lane's emath-cli WIP is mid-flight.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;

/// The slice reference carrier: edges 0→1, 0→2, 1→3, 2→3 (unweighted).
const ROUTER_SOURCE: &str = "emath function router:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n        b = bfs_order(g, 0)\n        d = shortest_distances(g, 0)\n        o = out_degrees(g)\n";

/// Check a source and evaluate its definitions.
fn eval(source: &str) -> BTreeMap<String, Value> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-surface.emath", source);
    let errors = checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "graph source must admit: {errors:#?}\nsource:\n{source}"
    );
    eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("graph source must evaluate: {fault}"))
}

/// Evaluate a source that must REFUSE at eval, returning the fault text.
fn eval_refusal(source: &str) -> String {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-refusal.emath", source);
    if checked.diagnostics.errors().next().is_some() {
        panic!("source must ADMIT and refuse at eval, got diagnostics");
    }
    let fault = eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    match fault {
        Err(message) => message.to_string(),
        Ok(values) => panic!("evaluation must refuse, got {values:?}"),
    }
}

/// Check a source that must REFUSE at ADMISSION, returning the
/// diagnostics for the caller to assert on.
fn admit_refusal(source: &str) -> Vec<String> {
    {
        // Capsule admission resolves only through the installed language
        // distribution; install per thread before any session (rat_cells pattern).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
            .expect("load capsule distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install capsule-active kernels");
    }
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let checked = session.check_owned("graph-admit-refusal.emath", source);
    checked
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect()
}

fn matrix_eq(actual: &Value, rows: usize, cols: usize, data: &[f64]) {
    assert_eq!(
        actual,
        &Value::Matrix {
            rows,
            cols,
            data: data.to_vec(),
        },
        "matrix mismatch"
    );
}

fn vector_eq(actual: &Value, want: &[f64]) {
    assert_eq!(actual, &Value::Vector(want.to_vec()), "vector mismatch");
}

/// P2: the dense carrier (graph literal) admits and the closed call
/// names execute from .emath source.
#[test]
fn emath_dense_carrier_and_call_surface() {
    let values = eval(ROUTER_SOURCE);
    matrix_eq(
        values.get("g").expect("graph literal"),
        4,
        4,
        &[
            0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        ],
    );
    vector_eq(
        values.get("r").expect("reachability"),
        &[1.0, 1.0, 1.0, 1.0],
    );
    vector_eq(values.get("b").expect("bfs order"), &[0.0, 1.0, 2.0, 3.0]);
    vector_eq(values.get("d").expect("distances"), &[0.0, 1.0, 1.0, 2.0]);
    vector_eq(values.get("o").expect("out degrees"), &[2.0, 1.0, 1.0, 0.0]);
}

/// P3: BFS is breadth-first, not depth-first, and deterministic —
/// two evaluations of the same program are bit-identical, and the
/// discriminator graph forces the ordering law (a DFS would visit 3
/// before 2).
#[test]
fn emath_bfs_is_breadth_first_and_deterministic() {
    let first = eval(ROUTER_SOURCE);
    let second = eval(ROUTER_SOURCE);
    assert_eq!(first, second, "evaluation is deterministic");
    vector_eq(first.get("b").expect("order"), &[0.0, 1.0, 2.0, 3.0]);
    // Isolated vertex: unreachable vertices stay absent from the
    // order and 0 in the mask.
    let isolated = eval(
        "emath function iso:\n    definitions:\n        g = graph { 0, 1, 2, 3, 4; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n        b = bfs_order(g, 0)\n",
    );
    vector_eq(isolated.get("r").expect("mask"), &[1.0, 1.0, 1.0, 1.0, 0.0]);
    vector_eq(isolated.get("b").expect("order"), &[0.0, 1.0, 2.0, 3.0]);
    // The mutation discriminator: 0→1, 0→2, 1→3, 2→4. A breadth-first
    // queue discovers [0, 1, 2, 3, 4]; a depth-first LIFO stack pops 2
    // before 1 and discovers 4 before 3 -> [0, 1, 2, 4, 3].
    let discriminating = eval(
        "emath function disc:\n    definitions:\n        g = graph { 0, 1, 2, 3, 4; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 4 }\n        b = bfs_order(g, 0)\n",
    );
    vector_eq(
        discriminating.get("b").expect("order"),
        &[0.0, 1.0, 2.0, 3.0, 4.0],
    );
    // BFS, never DFS: a LIFO stack would discover 4 before 3.
}

/// P4: Dijkstra distances with deterministic equal-weight tie layout,
/// and the nonnegative-weight refusal surfaced from .emath source.
#[test]
fn emath_dijkstra_ties_and_nonneg_refusal() {
    let values = eval(
        "emath function forks:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        d = shortest_distances(g, 0)\n",
    );
    vector_eq(values.get("d").expect("distances"), &[0.0, 1.0, 1.0, 2.0]);

    // A negative edge refuses Dijkstra's precondition (E-GRAPH-002) as
    // a typed eval fault — never a silently greedy answer, whichever
    // carrier spelling surfaces the negative weight (sparse build here;
    // signed graph literal in `emath_dijkstra_refuses_signed_negative_weight`).
    let refused = eval_refusal(
        "emath function neg:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0])\n        d = shortest_distances(g, 0)\n",
    );
    assert!(
        refused.contains("E-GRAPH-002"),
        "negative edge must refuse Dijkstra typed, got: {refused}"
    );
}

/// P5: degree, Laplacian, and spectral composition — the Laplacian of
/// an UNDIRECTED carrier composes through the existing symmetric eigen
/// op, and a directed carrier refuses the symmetric gate.
#[test]
fn emath_degree_laplacian_spectral_composition() {
    // Path P4 as an undirected carrier: 0–1, 1–2, 2–3 (the `u - v`
    // spelling yields two directed edges). Laplacian = D − A; its
    // spectrum is {0, 2−√2, 2, 2+√2}.
    let values = eval(
        "emath function path:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 - 1, 1 - 2, 2 - 3 }\n        o = out_degrees(g)\n        l = graph_laplacian(g)\n        e = eigvals(l)\n",
    );
    vector_eq(values.get("o").expect("degrees"), &[1.0, 2.0, 2.0, 1.0]);
    matrix_eq(
        values.get("l").expect("laplacian"),
        4,
        4,
        &[
            1.0, -1.0, 0.0, 0.0, -1.0, 2.0, -1.0, 0.0, 0.0, -1.0, 2.0, -1.0, 0.0, 0.0, -1.0, 1.0,
        ],
    );
    let Value::Vector(spectrum) = values.get("e").expect("spectrum") else {
        panic!("eigvals must return a vector");
    };
    let mut sorted = spectrum.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let want = [
        0.0,
        2.0 - std::f64::consts::SQRT_2,
        2.0,
        2.0 + std::f64::consts::SQRT_2,
    ];
    for (got, want) in sorted.iter().zip(want.iter()) {
        assert!(
            (got - want).abs() < 1e-9,
            "path spectrum mismatch: got {sorted:?}, want {want:?}"
        );
    }

    // A directed carrier refuses the symmetric eigen gate — the fence
    // is explicit, not a silent diagonalization.
    let refused = eval_refusal(
        "emath function di:\n    definitions:\n        g = graph { 0, 1; 0 --> 1 }\n        l = graph_laplacian(g)\n        e = eigvals(l)\n",
    );
    assert!(
        refused.contains("E-LINALG-002"),
        "directed carrier must refuse the symmetric eigen gate, got: {refused}"
    );

    // Symmetrization is a user choice: symmetrized directed carrier
    // composes through the same path.
    let values = eval(
        "emath function sym:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 1 --> 2, 2 --> 3 }\n        s = graph_symmetrize(g)\n        l = graph_laplacian(s)\n        e = eigvals(l)\n",
    );
    assert!(
        matches!(values.get("e"), Some(Value::Vector(v)) if v.len() == 4),
        "symmetrized path must carry a 4-element spectrum"
    );
}

/// P6: Bellman-Ford admits negative edges; a reachable negative cycle
/// refuses (E-GRAPH-005); sparse COO extraction/build round-trips the
/// dense carrier.
#[test]
fn emath_bellman_ford_negative_edges_and_cycle_refusal() {
    // Negative weights enter through the sparse COO build (and — since
    // grant 156–158 — through signed graph literals, see the P6b tests).
    // Negative edge admitted: d = [0, -1] (Dijkstra would have refused).
    let values = eval(
        "emath function negbf:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0])\n        d = bellman_ford(g, 0)\n",
    );
    vector_eq(values.get("d").expect("distances"), &[0.0, -1.0]);

    // Reachable negative cycle: no answer exists — refuse typed.
    let refused = eval_refusal(
        "emath function cyc:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0, 1.0, 0.0, -1.0])\n        d = bellman_ford(g, 0)\n",
    );
    assert!(
        refused.contains("E-GRAPH-005"),
        "reachable negative cycle must refuse, got: {refused}"
    );

    // COO round-trip: sparse_triplets extracts ascending (u, v) with
    // explicit zeros skipped; sparse_from_triplets rebuilds the dense
    // carrier identically.
    let values = eval(
        "emath function coo:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3, 2 --> 0 }\n        t = sparse_triplets(g)\n        g2 = sparse_from_triplets(4.0, t)\n",
    );
    let Value::Vector(triplets) = values.get("t").expect("triplets") else {
        panic!("sparse_triplets must return a vector");
    };
    assert_eq!(
        triplets,
        &[
            0.0, 1.0, 1.0, 0.0, 2.0, 1.0, 1.0, 3.0, 1.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0
        ]
        .to_vec(),
        "triplet stream is ascending (u, v), explicit zeros skipped"
    );
    matrix_eq(
        values.get("g2").expect("rebuilt"),
        4,
        4,
        &[
            0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        ],
    );
}

/// P6b (grant 156–158): signed numeric literals in graph edges. The
/// generic signed-literal helper folds unary minus/plus over `Int`/
/// `Float` spellings, so `-[w]->` weights admit negative AND positive
/// signs in the graph literal itself — not only via
/// `sparse_from_triplets`.
#[test]
fn emath_signed_weight_literal_admits_as_edge() {
    let values = eval(
        "emath function sgn:\n    definitions:\n        g = graph { 0, 1, 2; 0 -[-1.0]-> 1, 1 -[+2.0]-> 2 }\n",
    );
    matrix_eq(
        values.get("g").expect("graph literal"),
        3,
        3,
        &[0.0, -1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0],
    );
}

/// The signed negative literal that previously died at admission
/// (E-TYPE-012) must now REACH Dijkstra and refuse there with its own
/// exact precondition code — the gap moves, it does not get silently
/// diagonalized or rounded.
#[test]
fn emath_dijkstra_refuses_signed_negative_weight() {
    let refused = eval_refusal(
        "emath function neg:\n    definitions:\n        g = graph { 0, 1; 0 -[-1.0]-> 1 }\n        d = shortest_distances(g, 0)\n",
    );
    assert!(
        refused.contains("E-GRAPH-002"),
        "Dijkstra must refuse the signed negative literal with E-GRAPH-002, got: {refused}"
    );
}

/// Bellman-Ford computes through the signed literal carrier: the
/// negative edge is a real weight, and the shortest path uses it.
#[test]
fn emath_bellman_ford_computes_signed_negative_weights() {
    let values = eval(
        "emath function negbf:\n    definitions:\n        g = graph { 0, 1, 2; 0 -[-1.0]-> 1, 0 -[2.0]-> 2, 1 -[0.5]-> 2 }\n        d = bellman_ford(g, 0)\n",
    );
    // 0 → 1 costs -1.0; 0 → 2 costs -0.5 (via 1), beating the +2.0 edge.
    vector_eq(values.get("d").expect("distances"), &[0.0, -1.0, -0.5]);
}

/// The fix must NOT widen the fence: a path, a computed expression, or
/// any non-literal spelling in a weight bracket still refuses
/// E-TYPE-012 at admission.
#[test]
fn emath_malformed_weight_still_refuses() {
    for source in [
        "emath function bad1:\n    definitions:\n        g = graph { 0, 1; 0 -[w]-> 1 }\n",
        "emath function bad2:\n    definitions:\n        g = graph { 0, 1; 0 -[1 + 2]-> 1 }\n",
    ] {
        let errors = admit_refusal(source);
        assert!(
            errors.iter().any(|e| e.contains("E-TYPE-012")),
            "malformed weight must refuse E-TYPE-012, got: {errors:#?}"
        );
    }
}

/// P7: vertex-relabel metamorphic laws. Relabeling vertices by a
/// permutation p must preserve the graph's meaning:
/// - reachability masks permute: reach′[p(u)] == reach[u];
/// - shortest distances permute: d′[p(u)] == d[u];
/// - out-degrees permute: deg′[p(u)] == deg[u];
/// - Laplacians are permutation-similar, so sorted spectra are equal.
#[test]
fn emath_vertex_relabel_metamorphic_laws() {
    // Base graph: 0→1 (1.0), 0→2 (2.0), 1→3 (3.0), 2→3 (0.5).
    let base = eval(
        "emath function base:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 -[1.0]-> 1, 0 -[2.0]-> 2, 1 -[3.0]-> 3, 2 -[0.5]-> 3 }\n        r = reachability(g, 0)\n        d = shortest_distances(g, 0)\n        o = out_degrees(g)\n        l = graph_laplacian(graph_symmetrize(g))\n        e = eigvals(l)\n",
    );
    // Relabeled graph under p = [2, 0, 3, 1]: base edges mapped
    // u→v into p[u]→p[v]. The spectral side flows through the user's
    // explicit symmetrization (a directed carrier is never diagonalized
    // silently).
    let relabeled = eval(
        "emath function relabeled:\n    definitions:\n        g = graph { 0, 1, 2, 3; 2 -[1.0]-> 0, 2 -[2.0]-> 3, 0 -[3.0]-> 1, 3 -[0.5]-> 1 }\n        r = reachability(g, 2)\n        d = shortest_distances(g, 2)\n        o = out_degrees(g)\n        l = graph_laplacian(graph_symmetrize(g))\n        e = eigvals(l)\n",
    );
    let p = [2usize, 0, 3, 1];

    /// relabeled[p[u]] for each u — the relabel law's left side.
    let relabel_view = |value: &Value| -> Vec<f64> {
        let Value::Vector(vec) = value else {
            panic!("expected vector");
        };
        p.iter().map(|&u| vec[u]).collect()
    };
    let base_as_vec = |name: &str| -> Vec<f64> {
        let Value::Vector(vec) = base.get(name).expect(name) else {
            panic!("expected vector");
        };
        vec.clone()
    };

    assert_eq!(
        relabel_view(relabeled.get("r").expect("r")),
        base_as_vec("r"),
        "reachability relabel law: relabeled[p[u]] == base[u]"
    );
    let Value::Vector(d_rel) = relabeled.get("d").expect("d") else {
        panic!("expected vector");
    };
    for old in 0..4usize {
        let Value::Vector(d_base) = base.get("d").expect("d") else {
            panic!("expected vector");
        };
        if d_base[old].is_finite() {
            assert_eq!(
                d_rel[p[old]], d_base[old],
                "distance relabel law at vertex {old}"
            );
        } else {
            assert!(
                !d_rel[p[old]].is_finite(),
                "unreachable stays unreachable under relabel at {old}"
            );
        }
    }
    assert_eq!(
        relabel_view(relabeled.get("o").expect("o")),
        base_as_vec("o"),
        "out-degree relabel law: relabeled[p[u]] == base[u]"
    );

    // Laplacian spectra are equal as sorted multisets (P L Pᵀ).
    let spectrum_of = |value: &Value| -> Vec<f64> {
        let Value::Vector(vec) = value else {
            panic!("expected spectrum vector");
        };
        let mut sorted = vec.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        sorted
    };
    let base_spectrum = spectrum_of(base.get("e").expect("e"));
    let relabeled_spectrum = spectrum_of(relabeled.get("e").expect("e"));
    assert_eq!(base_spectrum.len(), relabeled_spectrum.len());
    for (a, b) in base_spectrum.iter().zip(relabeled_spectrum.iter()) {
        assert!(
            (a - b).abs() < 1e-9,
            "relabel must preserve the spectrum: {base_spectrum:?} vs {relabeled_spectrum:?}"
        );
    }
}
