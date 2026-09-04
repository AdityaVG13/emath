//! Compute-first doctrine gate (emath-49o8): shipped compute paths keep
//! admitting, the previously-refused slices that landed keep admitting
//! end to end, and the remaining feature-gap sketches fail closed with
//! their pinned codes — never an opaque unimplemented catch-all.
//!
//! Landed on HEAD (must keep admitting): variable-bound sums and
//! quantifiers, integral, autodiff, solve/optimize, DAEs, exact integer
//! arithmetic, constraints, elementary functions, signed graph literals
//! (bellman_ford), embedded package imports, spatial field builtins.
//!
//! Remaining feature gaps, each owned by an open bead (never a
//! catch-all): record member access (`p.x`) — emath-r5-records-6hcu;
//! custom declaration kinds — emath-r6-kinds-45l8; PDE methods beyond
//! Laplacians — emath-xx0x.4.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
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
    session.check_owned(name, source)
}

fn error_codes(result: &emath_sema::admit::CheckResult) -> Vec<&'static str> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn shipped_compute_examples_still_admit() {
    // Mutating any of these examples to expect a refusal must fail this
    // gate (and be restored) — they are the compute-first proof.
    let shipped: &[(&str, &str)] = &[
        (
            "autodiff",
            include_str!("../../../language/examples/intro/autodiff.emath"),
        ),
        (
            "solve",
            include_str!("../../../language/examples/intro/solve.emath"),
        ),
        (
            "optimize",
            include_str!("../../../language/examples/intro/optimize.emath"),
        ),
        (
            "jacobian",
            include_str!("../../../language/examples/intro/jacobian.emath"),
        ),
        (
            "dae",
            include_str!("../../../language/examples/numerical/dae-rc-circuit.emath"),
        ),
    ];
    for (name, source) in shipped {
        let result = check(name, source);
        assert!(
            !result.diagnostics.has_errors(),
            "{name} must keep admitting: {:?}",
            result.diagnostics.errors().collect::<Vec<_>>()
        );
    }
}

#[test]
fn landed_remaining_slice_capabilities_admit() {
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
    // Graphs landed (emath-r2-graphs-masa closed): the shipped
    // negative-edge carrier admits.
    let graph = session
        .check_owned(
            "graph",
            "emath function negative_edge_shortest_paths:\n    inputs:\n        source: Float64\n    outputs:\n        distances: Vector<Float64>\n    definitions:\n        distances = bellman_ford([[0, 4, 1, 0], [0, 0, 0, 1], [0, -2, 0, 5], [0, 0, 0, 0]], source)\n",
        );
    assert!(!graph.diagnostics.has_errors());

    // Multi-file imports (emath-r3-imports-utzd closed): embedded
    // package imports resolve.
    let imports = session.check_owned("imports", "use physics::classical::{NewtonSecond}\n");
    assert!(!imports.diagnostics.has_errors());
}

#[test]
fn remaining_slice_sketches_refuse_with_pinned_codes() {
    // Custom declaration kinds: schema-driven sections are owned by
    // emath-r6-kinds-45l8. The fence is the Phase 1 subset, not a
    // catch-all.
    let kinds = check(
        "kinds",
        "emath widget Cool:\n    inputs:\n        x: Float64\n",
    );
    assert!(
        error_codes(&kinds).iter().any(|code| *code == "E-KIND-100"),
        "custom-kind sketches must refuse with the pinned subset fence"
    );

    // Records as first-class values (member access) are owned by
    // emath-r5-records-6hcu; today the fence is the name resolver
    // (E-TYPE-002), never a silent admit. Record DATA (a prefixed
    // record literal bound to a name) admits — the value lane is
    // landed; member access is the gap.
    let records = check(
        "records-data",
        "emath function Wrap:\n    inputs:\n        x: Float64\n    outputs:\n        c: Float64\n    definitions:\n        p = Point:{ x: 1.0, y: 2.0 }\n        c = 3.0\n",
    );
    assert!(
        !records.diagnostics.has_errors(),
        "record DATA admits (the value lane is landed)"
    );
    let mut session = CompilerSession::new(Limits::default());
    let access = session.check_owned(
        "records-member-access",
        "emath function Wrap:\n    inputs:\n        x: Float64\n    outputs:\n        c: Float64\n    definitions:\n        p = Point:{ x: 1.0, y: 2.0 }\n        c = p.x + 1.0\n",
    );
    assert!(
        access
            .diagnostics
            .errors()
            .any(|diagnostic| diagnostic.code == "E-TYPE-002"),
        "record member access must fail closed until emath-r5-records-6hcu lands"
    );
}
