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

use emath_test_harness::{Probe, Source, boot};

const GRAPH: &str = "\
emath function negative_edge_shortest_paths:
    inputs:
        source: Float64
    outputs:
        distances: Vector<Float64>
    definitions:
        distances = bellman_ford([[0, 4, 1, 0], [0, 0, 0, 1], [0, -2, 0, 5], [0, 0, 0, 0]], source)
";

const RECORD_DATA: &str = "\
emath function Wrap:
    inputs:
        x: Float64
    outputs:
        c: Float64
    definitions:
        p = Point:{ x: 1.0, y: 2.0 }
        c = 3.0
";

const RECORD_ACCESS: &str = "\
emath function Wrap:
    inputs:
        x: Float64
    outputs:
        c: Float64
    definitions:
        p = Point:{ x: 1.0, y: 2.0 }
        c = p.x + 1.0
";

#[test]
fn doctrine() {
    boot();
    let mut p = Probe::new(
        "shipped compute paths admit; open gaps fail closed with pinned codes",
    );
    p.case("shipped", |p| {
        for path in [
            "language/examples/intro/autodiff.emath",
            "tests/fixtures/language/intro/solve.emath",
            "language/examples/intro/optimize.emath",
            "tests/fixtures/language/intro/jacobian.emath",
            "language/examples/numerical/dae-rc-circuit.emath",
        ] {
            Source::from_workspace(path).must_admit(&mut *p);
        }
    });
    p.case("landed", |p| {
        Source::from_str("graph", GRAPH).must_admit(&mut *p);
        Source::from_str("imports", "use physics::classical::{NewtonSecond}\n")
            .must_admit(&mut *p);
    });
    p.case("gaps", |p| {
        Source::from_str("kinds", "emath widget Cool:\n    inputs:\n        x: Float64\n")
            .must_refuse(&mut *p, &["E-KIND-100"]);
        Source::from_str("records-data", RECORD_DATA).must_admit(&mut *p);
        Source::from_str("records-member-access", RECORD_ACCESS)
            .must_refuse(&mut *p, &["E-TYPE-002"]);
    });
    p.finish();
}
