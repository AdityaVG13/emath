//! Interval-containment property grid.
//!
//! CONTRACT.md (emath-ir): `Domain` / `Interval` provide deterministic
//! membership (`Interval::contains`, `Domain::require_contains` with
//! `E-DOM-001`). TEST_STRATEGY.md names interval containment as a
//! property law; this is the seam that actually claims it.

use emath_ir::{Domain, Interval};

/// Seeded probe grid: interiors, endpoints, and exteriors of [0, 1].
const PROBES: [f64; 11] = [-2.0, -0.5, -0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 2.0, 10.0];

#[test]
fn interval_containment_holds_on_seeded_grid() {
    let inner = Domain::Interval(Interval::closed(0.0, 1.0));
    let outer = Domain::Interval(Interval::closed(-1.0, 2.0));

    for value in PROBES {
        let in_inner = inner.contains(value);
        let in_outer = outer.contains(value);
        if in_inner {
            assert!(
                in_outer,
                "{value} in the inner interval must stay in the outer interval"
            );
            inner
                .require_contains(value, "inner")
                .expect("a contained probe must not raise E-DOM-001");
        }
        if !in_outer {
            assert!(
                !in_inner,
                "{value} outside the outer interval cannot be inside the inner"
            );
            let error = outer
                .require_contains(value, "outer")
                .expect_err("an exterior probe must raise E-DOM-001");
            assert_eq!(error.code, "E-DOM-001");
        }
    }

    assert!(inner.contains(0.0) && inner.contains(1.0) && inner.contains(0.5));
    assert!(!inner.contains(-0.5) && !inner.contains(1.25));
}
