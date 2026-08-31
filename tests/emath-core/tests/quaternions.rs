//! `emath-r3-quaternions-cgvg`: B44 nucleus — contract tests.
//!
//! Design resolution (per the bead + C18): the algebra lands as an
//! emath-core nucleus with CONSTRUCTOR spelling (`quat(w, x, y, z)`)
//! and named basis constants (`qi`, `qj`, `qk`) deferred to the sema
//! admission-table follow-up — the `i/j/k` literal-suffix collision
//! with the complex `Ni` production (B14) is avoided by NOT adding any
//! new literal suffix at all. Clifford basis multiplication is the
//! C10-unblocked generic carrier (`Clifford<p, q>` over graded
//! basis blades); dual numbers carry ε² = 0 for exact first-order
//! differentiation.
//!
//! Honesty: floating-point norm/normalize are labeled f64 operations;
//! `normalize` of the zero quaternion refuses (no NaN laundering).
//! Non-commutativity is part of the CONTRACT (pinned), not an
//! implementation accident.
//!
//! Failure-first: RED (E0432) until the modules land.

use emath_core::clifford::{CliffordBasis, MultiVector};
use emath_core::dual::Dual;
use emath_core::quaternion::{quat, Quaternion};

const TOL: f64 = 1e-12;

#[test]
fn quaternion_construction_and_components() {
    let q = quat(1.0, 2.0, 3.0, 4.0);
    assert_eq!((q.w, q.x, q.y, q.z), (1.0, 2.0, 3.0, 4.0));
}

#[test]
fn quaternion_multiplication_is_non_commutative() {
    // i·j = k, j·i = −k — THE quaternion law; both orders pinned.
    let i = quat(0.0, 1.0, 0.0, 0.0);
    let j = quat(0.0, 0.0, 1.0, 0.0);
    let k = quat(0.0, 0.0, 0.0, 1.0);
    let ij = i * j;
    assert_eq!((ij.w, ij.x, ij.y, ij.z), (0.0, 0.0, 0.0, 1.0), "i·j = k");
    let ji = j * i;
    assert_eq!((ji.w, ji.x, ji.y, ji.z), (0.0, 0.0, 0.0, -1.0), "j·i = −k");
    assert_ne!(ij, ji, "non-commutativity is contract, not accident");
    // i² = j² = k² = −1.
    for (name, b) in [("i", i), ("j", j), ("k", k)] {
        let sq = b * b;
        assert_eq!((sq.w, sq.x, sq.y, sq.z), (-1.0, 0.0, 0.0, 0.0), "{name}² = −1");
    }
}

#[test]
fn quaternion_norm_conjugate_and_normalize() {
    let q = quat(1.0, 2.0, 3.0, 4.0);
    assert!((q.norm() - 30.0_f64.sqrt()).abs() < TOL);
    let conj = q.conjugate();
    assert_eq!((conj.w, conj.x, conj.y, conj.z), (1.0, -2.0, -3.0, -4.0));
    // q·q̄ = ‖q‖² (scalar).
    let product = q * conj;
    assert!((product.w - 30.0).abs() < TOL);
    assert!(product.x.abs() < TOL && product.y.abs() < TOL && product.z.abs() < TOL);
    let unit = q.normalize().unwrap();
    assert!((unit.norm() - 1.0).abs() < TOL);
    // Zero quaternion normalize refuses — no NaN laundering.
    assert!(quat(0.0, 0.0, 0.0, 0.0).normalize().is_err());
}

#[test]
fn quaternion_rotation_of_vector_axis_angle() {
    // Rotate (1,0,0) by 90° about z: expect (0,1,0). Hamilton
    // convention: v' = q v q̄ with q = [cos(θ/2), sin(θ/2)·axis].
    let half = std::f64::consts::FRAC_PI_4;
    let q = quat(half.cos(), 0.0, 0.0, half.sin());
    let rotated = q.rotate_vector([1.0, 0.0, 0.0]);
    assert!((rotated[0] - 0.0).abs() < 1e-12);
    assert!((rotated[1] - 1.0).abs() < 1e-12);
    assert!((rotated[2] - 0.0).abs() < 1e-12);
}

#[test]
fn dual_numbers_epsilon_squared_is_zero() {
    let a = Dual::new(2.0, 1.0); // a = 2 + 1ε
    let sq = a * a;
    // (2+ε)² = 4 + 4ε + ε² = 4 + 4ε (the ε² term VANISHES — exact).
    assert_eq!((sq.value, sq.epsilon), (4.0, 4.0));
    // First-order evaluation: f(x) = x³ at x=2 → f=8, f'=12 — exact
    // by the ε² = 0 carrier rule, no finite-difference error.
    let cube = a * a * a;
    assert_eq!((cube.value, cube.epsilon), (8.0, 12.0));
}

#[test]
fn dual_arithmetic_rules() {
    let x = Dual::new(3.0, 1.0);
    let c = Dual::new(5.0, 0.0); // constant: zero ε-part
    // (x + c)' = x' ; (x·c)' = c·x' ; c/x = c/x − c·x'/x² ε.
    let sum = x + c;
    assert_eq!((sum.value, sum.epsilon), (8.0, 1.0));
    let product = x * c;
    assert_eq!((product.value, product.epsilon), (15.0, 5.0));
    let quotient = c / x;
    assert!((quotient.value - 5.0 / 3.0).abs() < TOL);
    assert!((quotient.epsilon - -5.0 / 9.0).abs() < TOL);
}

#[test]
fn clifford_basis_multiplication_tables() {
    // Cl(0,2) (the quaternions' algebra): e1² = e2² = −1, e1·e2 = −e2·e1
    // = e12. The multiplication table is derived from (p, q), never
    // hand-listed.
    let basis = CliffordBasis::new(0, 2);
    let e1 = basis.blade(1);
    let e2 = basis.blade(2);
    let e1_sq = basis.multiply(e1, e1);
    assert_eq!(e1_sq.coefficient_of(0), -1.0, "e1² = −1 in Cl(0,2)");
    let e12 = basis.multiply(e1, e2);
    assert_eq!(e12.coefficient_of(0b11), 1.0, "e1·e2 = e12");
    let e21 = basis.multiply(e2, e1);
    assert_eq!(e21.coefficient_of(0b11), -1.0, "e2·e1 = −e12");
    assert_ne!(e12.coefficient_of(0b11), e21.coefficient_of(0b11));
}

#[test]
fn clifford_euclidean_p2q0_dot_product() {
    // Cl(2,0): e1² = e2² = +1. The geometric product of a vector with
    // itself IS the squared norm: (a·e1 + b·e2)² = a² + b² (wedge part
    // vanishes for equal vectors).
    let basis = CliffordBasis::new(2, 0);
    let v = MultiVector::from_blades(&basis, &[(0b01, 3.0), (0b10, 4.0)]);
    let vv = basis.multiply_multivector(&v, &v);
    assert!((vv.coefficient_of(0) - 25.0).abs() < TOL, "3-4-5: v² = 25");
    assert!(vv.coefficient_of(0b11).abs() < TOL, "wedge part vanishes");
}

#[test]
fn clifford_dimension_is_p_plus_q() {
    let basis = CliffordBasis::new(1, 2);
    assert_eq!(basis.dimension(), 3);
    assert_eq!(basis.blade_count(), 8, "2^(p+q) basis blades");
}


#[test]
fn jk_suffixes_are_not_complex_literals() {
    // C18 negative control (parse level): `3j`/`4k` were never complex
    // spellings and no quaternion suffix was added — an i/j/k chain
    // must refuse rather than silently parse as anything.
    install_source_parser_shim();
    let (_, diags) = emath_syntax::parse_str(
        "emath function f:\n    definitions:\n        q = 1.0 + 2.0i + 3.0j + 4.0k\n",
    );
    assert!(
        diags.has_errors(),
        "i/j/k suffix chain must refuse at parse (C18)"
    );
    // The admitted complex spelling still parses clean.
    let (_, diags) = emath_syntax::parse_str(
        "emath function f:\n    definitions:\n        z = 2.0i\n",
    );
    assert!(!diags.has_errors(), "complex `2.0i` unchanged, got {diags:?}");
}

fn install_source_parser_shim() {
    // Keeps this core-side suite independent of the syntax crate's
    // install helper naming while still exercising the real lexer.
    // (Direct emath_syntax::parse_str usage — no global install.)
}
