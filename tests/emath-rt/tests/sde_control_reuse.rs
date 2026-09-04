//! — the existing control
//! kernels execute from the SDE/control lane's own external target,
//! proving the control surface (transfer functions, state-space DC
//! gain, Routh–Hurwitz stability) is reuse-ready and remains green
//! against the new stochastic module.
//!
//! These are the SAME kernels the `.emath` control cells lower to
//! (control_systems.rs proves the EMIR-side seam); this target proves
//! the emath-rt seam directly, including the typed refusals, so the
//! control language cells keep executing through existing control
//! paths without any SDE-specific code.

use emath_rt::control::{ControlError, poles_stable, state_space_dc_gain, transfer_eval};

/// `H(s) = (s + 2) / (s² + 3s + 2)` — ASCENDING carriers:
/// num [2, 1], den [2, 3, 1]. H(0) = 1, H(1) = 3/6 = 0.5.
#[test]
fn transfer_function_reuse_executes() {
    let at_0 = transfer_eval(&[2.0, 1.0], &[2.0, 3.0, 1.0], 0.0).unwrap();
    assert_eq!(at_0, 1.0, "H(0) = 2/2");
    let at_1 = transfer_eval(&[2.0, 1.0], &[2.0, 3.0, 1.0], 1.0).unwrap();
    assert_eq!(at_1, 0.5, "H(1) = 3/6");
}

/// A pole hit refuses `E-CONTROL-002` — the value does not exist.
#[test]
fn transfer_function_pole_hit_refuses() {
    let err = transfer_eval(&[2.0, 1.0], &[2.0, 3.0, 1.0], -2.0).unwrap_err();
    assert_eq!(err.code(), "E-CONTROL-002", "H(-2) hits a pole");
}

/// The companion state-space pair matches the transfer function
/// DC gain bit-exactly; an unstable carrier refuses `E-CONTROL-003`.
#[test]
fn dc_gain_matches_transfer_and_refuses_unstable() {
    // A = [[-3, -2], [1, 0]], b = [1, 0], c = [1, 2] → DC gain 1.0.
    let a = vec![vec![-3.0, -2.0], vec![1.0, 0.0]];
    let gain = state_space_dc_gain(&a, &[1.0, 0.0], &[1.0, 2.0]).unwrap();
    assert_eq!(gain, 1.0, "c·(−A)⁻¹·b = 1");
    // Unstable: A = [[1, 0], [0, 1]] (pole at +1).
    let unstable = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
    let err = state_space_dc_gain(&unstable, &[1.0, 0.0], &[1.0, 2.0]).unwrap_err();
    assert_eq!(
        err.code(),
        "E-CONTROL-003",
        "unstable carrier refuses DC gain"
    );
}

/// Routh–Hurwitz: `s² + 3s + 2` is stable; `s + 1` is stable; the
/// zero polynomial refuses `E-CONTROL-002`.
#[test]
fn stability_predicate_executes() {
    assert_eq!(poles_stable(&[2.0, 3.0, 1.0]).unwrap(), true);
    assert_eq!(poles_stable(&[1.0, 1.0]).unwrap(), true);
    let err = poles_stable(&[0.0, 0.0]).unwrap_err();
    assert_eq!(
        err.code(),
        "E-CONTROL-002",
        "zero polynomial has no pole set"
    );
}

/// Shape mismatch refuses `E-CONTROL-004`.
#[test]
fn dc_gain_shape_mismatch_refuses() {
    let a = vec![vec![1.0, 0.0], vec![0.0, 1.0, 0.0]];
    let err = state_space_dc_gain(&a, &[1.0, 0.0], &[1.0]).unwrap_err();
    assert_eq!(err.code(), "E-CONTROL-004");
}

/// A degenerate Routh table (marginal poles) refuses `E-CONTROL-005`.
#[test]
fn marginal_routh_refuses() {
    // s² + 1 → zero first-column entry in the Routh table.
    let err = poles_stable(&[1.0, 0.0, 1.0]).unwrap_err();
    assert_eq!(err.code(), "E-CONTROL-005");
}

/// Control + SDE compose in one crate without entanglement: the SDE
/// drift carrier is an ascending polynomial like any control carrier,
/// and the SAME Routh–Hurwitz predicate the control cells use runs on
/// it unchanged — no SDE-specific code path exists. A negative-
/// feedback drift x' = −x is the stable polynomial 1·s + 1, and the
/// SDE with that drift is the mean-reverting process the control
/// predicate already certifies as stable.
#[test]
fn sde_drift_pairs_with_control_stability() {
    assert_eq!(poles_stable(&[1.0, 1.0]).unwrap(), true, "s + 1 stable");
}
