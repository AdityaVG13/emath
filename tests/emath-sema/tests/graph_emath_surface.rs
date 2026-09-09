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

use std::collections::BTreeMap;

use emath_exec_ir::interp::Value;
use emath_test_harness::{Probe, Source, boot};

const ROUTER: &str = "emath function router:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n        b = bfs_order(g, 0)\n        d = shortest_distances(g, 0)\n        o = out_degrees(g)\n";

fn eval_values(p: &mut Probe, name: &str, source: &str) -> Option<BTreeMap<String, Value>> {
    let checked = Source::from_str(name, source).must_admit(p);
    if checked.diagnostics.has_errors() {
        return None;
    }
    match emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    ) {
        Ok(values) => Some(values),
        Err(fault) => {
            p.fail(format!("{name}/eval"), format!("graph source must evaluate: {fault}"));
            None
        }
    }
}

fn eval_refuses(p: &mut Probe, name: &str, source: &str, code: &str) {
    let checked = Source::from_str(name, source).check();
    p.demand(format!("{name}/admits"), !checked.diagnostics.has_errors(), "source must admit and refuse at eval");
    if checked.diagnostics.has_errors() {
        return;
    }
    match emath_exec_ir::runner::eval_definitions_values(
        &checked.package,
        &checked.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    ) {
        Err(fault) => {
            p.contains(format!("{name}/code"), &fault.to_string(), code);
        }
        Ok(values) => {
            p.fail(format!("{name}/must-refuse"), format!("evaluation must refuse {code}, got {values:?}"));
        }
    }
}

#[test]
fn graph_emath_surface() {
    boot();
    let mut p = Probe::new("executable .emath graph surface: carriers, calls, refusals, relabel laws");
    p.case("dense-carrier", |p| {
        let Some(values) = eval_values(p, "router", ROUTER) else { return };
        p.eq("g", values.get("g"), Some(&Value::Matrix { rows: 4, cols: 4, data: vec![0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0] }));
        p.eq("r", values.get("r"), Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0])));
        p.eq("b", values.get("b"), Some(&Value::Vector(vec![0.0, 1.0, 2.0, 3.0])));
        p.eq("d", values.get("d"), Some(&Value::Vector(vec![0.0, 1.0, 1.0, 2.0])));
        p.eq("o", values.get("o"), Some(&Value::Vector(vec![2.0, 1.0, 1.0, 0.0])));
    });
    p.case("bfs-order", |p| {
        let Some(first) = eval_values(p, "router", ROUTER) else { return };
        let Some(second) = eval_values(p, "router-again", ROUTER) else { return };
        p.eq("deterministic", first.clone(), second);
        p.eq("order", first.get("b"), Some(&Value::Vector(vec![0.0, 1.0, 2.0, 3.0])));
        let Some(isolated) = eval_values(p, "iso", "emath function iso:\n    definitions:\n        g = graph { 0, 1, 2, 3, 4; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n        b = bfs_order(g, 0)\n") else { return };
        p.eq("iso-mask", isolated.get("r"), Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0, 0.0])));
        p.eq("iso-order", isolated.get("b"), Some(&Value::Vector(vec![0.0, 1.0, 2.0, 3.0])));
        let Some(discriminating) = eval_values(p, "disc", "emath function disc:\n    definitions:\n        g = graph { 0, 1, 2, 3, 4; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 4 }\n        b = bfs_order(g, 0)\n") else { return };
        p.eq("bfs-not-dfs", discriminating.get("b"), Some(&Value::Vector(vec![0.0, 1.0, 2.0, 3.0, 4.0])));
    });
    p.case("dijkstra", |p| {
        let Some(values) = eval_values(p, "forks", "emath function forks:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        d = shortest_distances(g, 0)\n") else { return };
        p.eq("distances", values.get("d"), Some(&Value::Vector(vec![0.0, 1.0, 1.0, 2.0])));
        eval_refuses(p, "neg-dijkstra", "emath function neg:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0])\n        d = shortest_distances(g, 0)\n", "E-GRAPH-002");
    });
    p.case("laplacian-spectral", |p| {
        let Some(values) = eval_values(p, "path", "emath function path:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 - 1, 1 - 2, 2 - 3 }\n        o = out_degrees(g)\n        l = graph_laplacian(g)\n        e = eigvals(l)\n") else { return };
        p.eq("degrees", values.get("o"), Some(&Value::Vector(vec![1.0, 2.0, 2.0, 1.0])));
        p.eq("laplacian", values.get("l"), Some(&Value::Matrix { rows: 4, cols: 4, data: vec![1.0, -1.0, 0.0, 0.0, -1.0, 2.0, -1.0, 0.0, 0.0, -1.0, 2.0, -1.0, 0.0, 0.0, -1.0, 1.0] }));
        match values.get("e") {
            Some(Value::Vector(spectrum)) => {
                let mut sorted = spectrum.clone();
                sorted.sort_by(|a, b| a.total_cmp(b));
                let want = [0.0, 2.0 - std::f64::consts::SQRT_2, 2.0, 2.0 + std::f64::consts::SQRT_2];
                p.eq("spectrum-len", sorted.len(), 4);
                for (index, (got, want)) in sorted.iter().zip(want.iter()).enumerate() {
                    p.close(format!("spectrum/{index}"), *got, *want, 1e-9);
                }
            }
            other => {
                p.fail("spectrum", format!("eigvals must return a vector, got {other:?}"));
            }
        }
        eval_refuses(p, "directed-eig", "emath function di:\n    definitions:\n        g = graph { 0, 1; 0 --> 1 }\n        l = graph_laplacian(g)\n        e = eigvals(l)\n", "E-LINALG-002");
        let Some(sym) = eval_values(p, "sym", "emath function sym:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 1 --> 2, 2 --> 3 }\n        s = graph_symmetrize(g)\n        l = graph_laplacian(s)\n        e = eigvals(l)\n") else { return };
        p.demand("symmetrized-spectrum", matches!(sym.get("e"), Some(Value::Vector(v)) if v.len() == 4), "symmetrized path must carry a 4-element spectrum");
    });
    p.case("bellman-ford", |p| {
        let Some(values) = eval_values(p, "negbf", "emath function negbf:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0])\n        d = bellman_ford(g, 0)\n") else { return };
        p.eq("neg-distances", values.get("d"), Some(&Value::Vector(vec![0.0, -1.0])));
        eval_refuses(p, "neg-cycle", "emath function cyc:\n    definitions:\n        g = sparse_from_triplets(2.0, [0.0, 1.0, -1.0, 1.0, 0.0, -1.0])\n        d = bellman_ford(g, 0)\n", "E-GRAPH-005");
        let Some(coo) = eval_values(p, "coo", "emath function coo:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3, 2 --> 0 }\n        t = sparse_triplets(g)\n        g2 = sparse_from_triplets(4.0, t)\n") else { return };
        p.eq("triplets", coo.get("t"), Some(&Value::Vector(vec![0.0, 1.0, 1.0, 0.0, 2.0, 1.0, 1.0, 3.0, 1.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0])));
        p.eq("rebuilt", coo.get("g2"), Some(&Value::Matrix { rows: 4, cols: 4, data: vec![0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0] }));
    });
    p.case("signed-literals", |p| {
        let Some(values) = eval_values(p, "sgn", "emath function sgn:\n    definitions:\n        g = graph { 0, 1, 2; 0 -[-1.0]-> 1, 1 -[+2.0]-> 2 }\n") else { return };
        p.eq("signed-weights", values.get("g"), Some(&Value::Matrix { rows: 3, cols: 3, data: vec![0.0, -1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0] }));
        eval_refuses(p, "signed-dijkstra", "emath function neg:\n    definitions:\n        g = graph { 0, 1; 0 -[-1.0]-> 1 }\n        d = shortest_distances(g, 0)\n", "E-GRAPH-002");
        let Some(negbf) = eval_values(p, "negbf-signed", "emath function negbf:\n    definitions:\n        g = graph { 0, 1, 2; 0 -[-1.0]-> 1, 0 -[2.0]-> 2, 1 -[0.5]-> 2 }\n        d = bellman_ford(g, 0)\n") else { return };
        p.eq("signed-bellman-ford", negbf.get("d"), Some(&Value::Vector(vec![0.0, -1.0, -0.5])));
    });
    p.case("malformed-weight", |p| {
        for (index, source) in [
            "emath function bad1:\n    definitions:\n        g = graph { 0, 1; 0 -[w]-> 1 }\n",
            "emath function bad2:\n    definitions:\n        g = graph { 0, 1; 0 -[1 + 2]-> 1 }\n",
        ]
        .iter()
        .enumerate()
        {
            Source::from_str(format!("malformed-{index}"), *source).must_refuse(p, &["E-TYPE-012"]);
        }
    });
    p.case("relabel-laws", |p| {
        let Some(base) = eval_values(p, "base", "emath function base:\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 -[1.0]-> 1, 0 -[2.0]-> 2, 1 -[3.0]-> 3, 2 -[0.5]-> 3 }\n        r = reachability(g, 0)\n        d = shortest_distances(g, 0)\n        o = out_degrees(g)\n        l = graph_laplacian(graph_symmetrize(g))\n        e = eigvals(l)\n") else { return };
        let Some(relabeled) = eval_values(p, "relabeled", "emath function relabeled:\n    definitions:\n        g = graph { 0, 1, 2, 3; 2 -[1.0]-> 0, 2 -[2.0]-> 3, 0 -[3.0]-> 1, 3 -[0.5]-> 1 }\n        r = reachability(g, 2)\n        d = shortest_distances(g, 2)\n        o = out_degrees(g)\n        l = graph_laplacian(graph_symmetrize(g))\n        e = eigvals(l)\n") else { return };
        let perm = [2usize, 0, 3, 1];
        let relabel_view = |value: &Value| -> Option<Vec<f64>> {
            match value {
                Value::Vector(vec) => Some(perm.iter().map(|u| vec[*u]).collect()),
                _ => None,
            }
        };
        let base_vec = |values: &BTreeMap<String, Value>, name: &str| -> Option<Vec<f64>> {
            match values.get(name) {
                Some(Value::Vector(vec)) => Some(vec.clone()),
                _ => None,
            }
        };
        match (relabeled.get("r"), base.get("r")) {
            (Some(relabeled_r), Some(_)) => match (relabel_view(relabeled_r), base_vec(&base, "r")) {
                (Some(got), Some(want)) => {
                    p.eq("relabel-reach", got, want);
                }
                _ => {
                    p.fail("relabel-reach", "reachability vectors must be vectors");
                }
            },
            other => {
                p.fail("relabel-reach", format!("reachability vectors missing: {other:?}"));
            }
        }
        match (relabeled.get("d"), base.get("d")) {
            (Some(Value::Vector(d_rel)), Some(Value::Vector(d_base))) => {
                for old in 0..4usize {
                    if d_base[old].is_finite() {
                        p.eq(format!("relabel-dist/{old}"), d_rel[perm[old]], d_base[old]);
                    } else {
                        p.demand(format!("relabel-unreachable/{old}"), !d_rel[perm[old]].is_finite(), "unreachable stays unreachable under relabel");
                    }
                }
            }
            other => {
                p.fail("relabel-dist", format!("distance vectors missing: {other:?}"));
            }
        }
        match (relabeled.get("o"), base.get("o")) {
            (Some(relabeled_o), Some(_)) => match (relabel_view(relabeled_o), base_vec(&base, "o")) {
                (Some(got), Some(want)) => {
                    p.eq("relabel-degree", got, want);
                }
                _ => {
                    p.fail("relabel-degree", "degree vectors must be vectors");
                }
            },
            other => {
                p.fail("relabel-degree", format!("degree vectors missing: {other:?}"));
            }
        }
        let spectrum_of = |value: &Value| -> Option<Vec<f64>> {
            match value {
                Value::Vector(vec) => {
                    let mut sorted = vec.clone();
                    sorted.sort_by(|a, b| a.total_cmp(b));
                    Some(sorted)
                }
                _ => None,
            }
        };
        match (base.get("e"), relabeled.get("e")) {
            (Some(base_e), Some(relabeled_e)) => match (spectrum_of(base_e), spectrum_of(relabeled_e)) {
                (Some(base_spectrum), Some(relabeled_spectrum)) => {
                    p.eq("spectrum-len", base_spectrum.len(), relabeled_spectrum.len());
                    for (index, (a, b)) in base_spectrum.iter().zip(relabeled_spectrum.iter()).enumerate() {
                        p.close(format!("relabel-spectrum/{index}"), *a, *b, 1e-9);
                    }
                }
                _ => {
                    p.fail("relabel-spectrum", "spectra must be vectors");
                }
            }
            other => {
                p.fail("relabel-spectrum", format!("spectra missing: {other:?}"));
            }
        }
    });
    p.finish();
}
