//! Bead emath-sci-physics-lane-3f7v (thin dim-group slice).
//!
//! The carrier of dimensional analysis is the free abelian group Z^7 over
//! the SI base dimensions. This suite pins:
//! 1. the group laws on that carrier (identity, inverse, associativity,
//!    commutativity) — a dim-group, not an ad-hoc exponent bag;
//! 2. law-grade homogeneity receipts (⟦lhs⟧ =symp ⟦rhs⟧ with the shared
//!    dimension as witness, notation canonical);
//! 3. the Buckingham π-theorem as an integer null-space computation with
//!    primitive (witness-minimized) groups — classic drag-law shape gives
//!    exactly 2 dimensionless groups over 5 variables of rank 3;
//! 4. the affine negative check: affine units are a torsor, NOT a
//!    multiplicative group — `20 degC * 2` refuses (E-UNIT-AFFINE-2),
//!    while the difference unit `ΔdegC * 2` is admitted (40 ΔdegC).
//!
//! RED first: none of dim_add/dim_neg/dim_pow/dim_notation/
//! check_homogeneity/dimensionless_groups/dim_rank existed before this
//! bead. tensor-geometry / Noether / variational-action are fenced to
//! follow-up slices (no fake conservation claims here).

#![forbid(unsafe_code)]

use emath_core::units::{
    dim_add, dim_group_is_primitive, dim_identity, dim_is_identity, dim_neg, dim_notation,
    dim_pow, dim_rank, check_homogeneity, difference_unit, mul, dimensionless_groups, Quantity,
    QuantityKind, UnitSpec, E_UNIT_AFFINE_MUL, E_UNIT_DIM,
};

const L: [i64; 7] = [1, 0, 0, 0, 0, 0, 0];
const M: [i64; 7] = [0, 1, 0, 0, 0, 0, 0];
const T: [i64; 7] = [0, 0, 1, 0, 0, 0, 0];
/// Force: kg·m·s^-2.
const FORCE: [i64; 7] = [1, 1, -2, 0, 0, 0, 0];
/// Energy: kg·m^2·s^-2.
const ENERGY: [i64; 7] = [2, 1, -2, 0, 0, 0, 0];

fn q(value: f64, name: &str, dims: [i64; 7], scale: f64, offset: f64, kind: QuantityKind) -> Quantity {
    Quantity { value, unit: UnitSpec::new(name, dims, scale, offset), kind }
}

#[test]
fn group_laws_hold_over_si_bases() {
    // Identity: FORCE * 1 = FORCE, and identity notation is "1".
    assert_eq!(dim_add(FORCE, dim_identity()), FORCE);
    assert_eq!(dim_add(dim_identity(), FORCE), FORCE);
    assert!(dim_is_identity(dim_identity()));
    assert!(!dim_is_identity(FORCE));

    // Inverse: FORCE * FORCE^-1 = 1; notation of the inverse is s^2/(m*kg)-style canonical.
    assert!(dim_is_identity(dim_add(FORCE, dim_neg(FORCE))));

    // Commutativity + associativity on a triple with mixed signs.
    let (a, b, c) = (FORCE, T, ENERGY);
    assert_eq!(dim_add(a, b), dim_add(b, a));
    assert_eq!(dim_add(dim_add(a, b), c), dim_add(a, dim_add(b, c)));

    // Power is repeated group composition: ENERGY^1 = ENERGY, ENERGY^-1 is the inverse.
    assert_eq!(dim_pow(ENERGY, 1), ENERGY);
    assert!(dim_is_identity(dim_add(ENERGY, dim_pow(ENERGY, -1))));
    // Energy = force * length: the group composes compound units exactly.
    assert_eq!(dim_add(FORCE, L), ENERGY);
}

#[test]
fn homogeneity_receipt_names_shared_witness() {
    // Kinetic energy form check: ⟦(1/2) m v^2⟧ vs ⟦E⟧ — v^2 has dims L^2 T^-2,
    // times M gives energy. The receipt carries the shared witness.
    let v_squared = dim_add(dim_pow(L, 2), dim_pow(T, -2));
    let left = dim_add(M, v_squared);
    let receipt = check_homogeneity(left, ENERGY).expect("m*v^2 is homogeneous with E");
    assert_eq!(receipt.witness, ENERGY);
    assert_eq!(receipt.notation, dim_notation(ENERGY));
    assert_eq!(receipt.notation, "m^2*kg*s^-2");

    // Refusal names BOTH sides' notations (law-grade diagnostic, not a bare code).
    let err = check_homogeneity(FORCE, ENERGY).expect_err("force != energy");
    assert_eq!(err.code, E_UNIT_DIM);
    assert!(err.message.contains("m*kg*s^-2"), "lhs notation named: {}", err.message);
    assert!(err.message.contains("m^2*kg*s^-2"), "rhs notation named: {}", err.message);

    // The identity is its own homogeneous partner (dimensionless law, e.g. ratios).
    assert!(check_homogeneity(dim_identity(), dim_identity()).is_ok());
}

#[test]
fn pi_theorem_drag_law_gives_two_dimensionless_groups() {
    // Classic external-flow variables: v, rho, r, mu, F.
    // Rank of the dimension matrix is 3 (M, L, T), so Buckingham gives
    // 5 - 3 = 2 independent dimensionless groups (Reynolds number and the
    // drag coefficient group).
    let vars = [
        dim_add(L, dim_neg(T)),                    // v:    L·T^-1
        dim_add(M, dim_pow(L, -3)),                // rho:  M·L^-3
        L,                                         // r:    L
        dim_add(dim_add(M, dim_neg(L)), dim_neg(T)), // mu:  M·L^-1·T^-1
        FORCE,                                     // F:    M·L·T^-2
    ];
    assert_eq!(dim_rank(&vars), 3);
    let groups = dimensionless_groups(&vars);
    assert_eq!(groups.len(), 2, "pi count = n - rank = 2, got {groups:?}");

    // Each group is a witness-minimized integer vector: primitive and
    // sign-canonical (first nonzero exponent positive).
    for g in &groups {
        assert!(dim_group_is_primitive(g), "group must be primitive: {g:?}");
        let first_nonzero = g.iter().copied().find(|e| *e != 0);
        assert!(first_nonzero.is_none_or(|e| e > 0), "sign-canonical: {g:?}");
    }

    // THE theorem: combining the variables with each group's exponents
    // yields the identity — the group is genuinely dimensionless.
    for g in &groups {
        let mut acc = dim_identity();
        for (var, exp) in vars.iter().zip(g.iter()) {
            if *exp != 0 {
                acc = dim_add(acc, dim_pow(*var, *exp));
            }
        }
        assert!(dim_is_identity(acc), "group {g:?} must be dimensionless, got {acc:?}");
    }

    // All-dimensionless variables: rank 0, every variable is its own group
    // (each basis vector is a standard basis direction, primitive).
    let ratios = [dim_identity(), dim_identity(), dim_identity()];
    assert_eq!(dim_rank(&ratios), 0);
    assert_eq!(dimensionless_groups(&ratios).len(), 3);

    // Witness-minimization is load-bearing: scaling every variable dim 2×
    // scales the raw elimination combos by a common factor at every stage,
    // but the same law family must yield the SAME normalized π groups.
    // (Kills a dropped primitive-renormalization gate — verified by mutant.)
    let doubled: Vec<[i64; 7]> = vars.iter().map(|v| dim_pow(*v, 2)).collect();
    assert_eq!(dimensionless_groups(&doubled), groups, "π basis is scale-invariant");
}

#[test]
fn affine_units_are_not_a_multiplicative_group() {
    // NC from the bead: 20 degC * 2 != 40 degC — it REFUSES. Affine units
    // are a torsor over the ratio-group, never a group under composition.
    let celsius = q(20.0, "degC", [0, 0, 0, 0, 1, 0, 0], 1.0, 273.15, QuantityKind::Absolute);
    let two = q(2.0, "K", [0, 0, 0, 0, 1, 0, 0], 1.0, 0.0, QuantityKind::Difference);
    let err = mul(&celsius, &two).expect_err("affine * anything refuses");
    assert_eq!(err.code, E_UNIT_AFFINE_MUL);
    assert!(err.message.contains("degC"));

    // The difference unit IS multiplicative: ΔdegC * 2 = 40 ΔdegC.
    let delta = Quantity {
        value: 20.0,
        unit: difference_unit(&celsius.unit),
        kind: QuantityKind::Difference,
    };
    let doubled = mul(&delta, &two).expect("difference * scalar admits");
    assert!((doubled.value - 40.0).abs() < 1e-12);
    assert_eq!(doubled.unit.name, "ΔdegC");
}
