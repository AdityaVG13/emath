//! Bead `emath-r3-sde-control-zxkl` — pass 5: the immutable static
//! native-kernel registry (approved generic ABI).
//!
//! These tests prove the registry shape WITHOUT the interpreter
//! wiring (which waits on CalmPine's interp.rs release):
//! - the table is keyed by capability name, resolves Pure cells, and
//!   is immutable (no register API, no runtime mutation, no ambient
//!   state);
//! - a TOY entry (`std.stochastic.toy_double`) proves genericity — the
//!   mechanism is domain-neutral, not an SDE branch;
//! - the SDE entries (`euler_maruyama`, `stratonovich`) expose the
//!   kernel through the existing exec-ir `Value` type with exact arity
//!   and strict-f64 refusal discipline;
//! - unknown names keep `None` (the caller's refusal path is
//!   unchanged); wrong arity refuses with the same message the
//!   compiled-cell path uses.

use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::native_kernel::native_kernel;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};

fn val_f64(x: f64) -> Value {
    Value::F64(x)
}

fn val_vec(xs: &[f64]) -> Value {
    Value::Vector(xs.to_vec())
}

/// The toy entry: pure doubling is generic kernel data.
#[test]
fn toy_kernel_doubles_scalar() {
    let kernel = native_kernel("std.stochastic.toy_double").expect("toy entry exists");
    assert_eq!(kernel.arity, 1);
    let out = (kernel.handler)(&[val_f64(21.0)]).expect("toy runs");
    assert_eq!(out, val_f64(42.0));
    // Non-finite input refuses under the pure-cell strict-f64 guard.
    let err = (kernel.handler)(&[val_f64(f64::NAN)]).expect_err("non-finite refuses");
    assert!(err.contains("E-CELL-006"), "guard code present: {err}");
    // A type-mismatched input refuses typed (not silently coerced).
    let err = (kernel.handler)(&[val_vec(&[1.0])]).expect_err("non-scalar refuses");
    assert!(err.contains("E-TYPE-012"), "type refusal present: {err}");
}

/// Wrong arity is refused identically to the compiled-cell path.
#[test]
fn kernel_enforces_arity() {
    let kernel = native_kernel("std.stochastic.euler_maruyama").expect("sde entry exists");
    // The registry stores the exact arity; the caller checks it BEFORE
    // the handler (same discipline as apply_capability_cell).
    assert_eq!(kernel.arity, 7, "sde cell contract is 7 args");
    // A direct handler call with the wrong count still refuses typed
    // (not a partial execution).
    let err = (kernel.handler)(&[val_f64(1.0), val_f64(2.0)]).unwrap_err();
    assert_eq!(
        err, "capability argument count does not match the cell contract",
        "arity refusal message matches the compiled-cell path"
    );
}

/// Unknown names keep `None` — the registry never fabricates a handler.
#[test]
fn unknown_name_keeps_none() {
    assert!(native_kernel("std.stochastic.does_not_exist").is_none());
    assert!(native_kernel("").is_none());
}

/// Genericity: a name that is NOT SDE resolves through the SAME table
/// mechanism (toy entry), so this is a generic capability ABI, not a
/// domain match arm.
#[test]
fn registry_is_generic_not_domain_switch() {
    let toy = native_kernel("std.stochastic.toy_double").expect("toy");
    let sde = native_kernel("std.stochastic.euler_maruyama").expect("sde");
    assert_ne!(toy.handler, sde.handler, "distinct handlers");
}

/// The SDE Itô entry: deterministic, matches the owned kernel oracle
/// bit-for-bit under a fixed seed (the Z stream is the SAME Normal(0,1)
/// draws the ProbSample cell yields).
#[test]
fn sde_ito_registry_matches_owned_kernel() {
    let kernel = native_kernel("std.stochastic.euler_maruyama").expect("ito entry");
    let drift = val_vec(&[0.0, 0.25]); // μ(x) = 0.25·x
    let diffusion = val_vec(&[0.0, 0.35]); // σ(x) = 0.35·x
    let seed = 7.0;
    let args = &[
        drift,
        diffusion,
        val_f64(1.0), // x0
        val_f64(0.01), // h
        val_f64(64.0), // steps
        val_f64(seed), // seed
        val_vec(&[]),   // stream (root)
    ];
    let got = (kernel.handler)(args).expect("ito runs");
    let Value::Vector(trajectory) = got else {
        panic!("ito must return a vector");
    };
    assert_eq!(trajectory.len(), 65, "x0 + 64 steps");
    // Cross-check against the owned emath-rt kernel directly: the
    // adapter must be a pure mirror with zero drift in the mapping.
    let want = emath_rt::stochastic::sde_euler_maruyama(
        emath_rt::stochastic::SdeRule::Ito,
        &[0.0, 0.25],
        &[0.0, 0.35],
        1.0,
        0.01,
        64,
        Some(seed),
    )
    .expect("owned kernel runs");
    assert_eq!(trajectory, want, "registry adapter must mirror the owned kernel");
}

/// The SDE Stratonovich entry: distinct rule, same kernel, same
/// oracle — the correction term is present, not merged.
#[test]
fn sde_stratonovich_registry_matches_owned_kernel() {
    let kernel = native_kernel("std.stochastic.stratonovich").expect("strat entry");
    let args = &[
        val_vec(&[0.0, 0.25]),
        val_vec(&[0.0, 0.35]),
        val_f64(1.0), // x0
        val_f64(0.01), // h
        val_f64(64.0), // steps
        val_f64(7.0),  // seed
        val_vec(&[]),  // stream (root)
    ];
    let got = (kernel.handler)(args).expect("strat runs");
    let Value::Vector(trajectory) = got else {
        panic!("strat must return a vector");
    };
    let want = emath_rt::stochastic::sde_euler_maruyama(
        emath_rt::stochastic::SdeRule::Stratonovich,
        &[0.0, 0.25],
        &[0.0, 0.35],
        1.0,
        0.01,
        64,
        Some(7.0),
    )
    .expect("owned kernel runs");
    assert_eq!(trajectory, want, "registration of the distinct rule must mirror the kernel");
    // The two registry entries are distinct cells, never aliased.
    assert_ne!(
        trajectory,
        {
            let ito = native_kernel("std.stochastic.euler_maruyama").unwrap();
            let Value::Vector(ito_traj) = (ito.handler)(args).unwrap() else {
                panic!("ito returns vector");
            };
            ito_traj
        },
        "ito and strat trajectories must differ for state-dependent noise"
    );
}

/// --- The interpreter seam (ApplyCapability → native registry) ---
///
/// The capability-application path consults compiled-cell data FIRST;
/// a miss falls to the immutable native-kernel registry with the SAME
/// arity/refusal discipline and NO new EmirOp or domain switch.
/// Unknown names keep the exact pre-existing refusal.

/// The fjxh.6 seam shape: load inputs, then one ApplyCapability.
fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, emath_core::Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Default::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Default::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

/// The seam executes the TOY cell through the real interpreter: the
/// native registry is reachable from ApplyCapability, not just from
/// direct handler calls.
#[test]
fn interp_seam_runs_toy_cell() {
    let out = seam_eval("std.stochastic.toy_double", &[val_f64(21.0)])
        .expect("toy cell resolves through the seam");
    assert_eq!(out, val_f64(42.0));
}

/// The seam executes the SDE cell through the real interpreter and the
/// result is the owned kernel's trajectory bit-for-bit.
#[test]
fn interp_seam_runs_sde_cell() {
    let inputs = vec![
        val_vec(&[0.0, 0.25]),
        val_vec(&[0.0, 0.35]),
        val_f64(1.0),
        val_f64(0.01),
        val_f64(64.0),
        val_f64(7.0),
        val_vec(&[]),
    ];
    let got = seam_eval("std.stochastic.euler_maruyama", &inputs)
        .expect("sde cell resolves through the seam");
    let want = emath_rt::stochastic::sde_euler_maruyama(
        emath_rt::stochastic::SdeRule::Ito,
        &[0.0, 0.25],
        &[0.0, 0.35],
        1.0,
        0.01,
        64,
        Some(7.0),
    )
    .expect("owned kernel runs");
    assert_eq!(got, Value::Vector(want), "seam result mirrors the owned kernel");
}

/// A wrong argument count through the seam refuses with the SAME
/// message the compiled-cell path uses (one discipline, two backends).
#[test]
fn interp_seam_arity_refusal_matches_compiled_path() {
    let err = seam_eval("std.stochastic.toy_double", &[val_f64(1.0), val_f64(2.0)])
        .expect_err("arity mismatch refuses");
    match err {
        EvalFault::Arithmetic { detail, .. } => {
            assert_eq!(
                detail, "capability argument count does not match the cell contract",
                "arity refusal matches the compiled-cell path"
            );
        }
        other => panic!("expected Arithmetic fault, got {other:?}"),
    }
}

/// An unknown capability name through the seam keeps the EXACT
/// pre-existing refusal — the registry never fabricates a handler and
/// the miss path is unchanged.
#[test]
fn interp_seam_unknown_name_unchanged() {
    let err = seam_eval("std.stochastic.does_not_exist", &[val_f64(1.0)])
        .expect_err("unknown name refuses");
    match err {
        EvalFault::Arithmetic { detail, .. } => {
            assert_eq!(
                detail, "no local reference semantics for this pure cell",
                "unknown-name refusal is unchanged"
            );
        }
        other => panic!("expected Arithmetic fault, got {other:?}"),
    }
}

/// A kernel-side typed refusal (domain error) flows through the seam
/// verbatim as a CapabilityRefused naming the capability and the
/// stable code — typed, never a silent value.
#[test]
fn interp_seam_kernel_refusal_flows_through() {
    let inputs = vec![
        val_vec(&[0.0, 0.25]),
        val_vec(&[0.0, 0.35]),
        val_f64(1.0),
        val_f64(0.0), // h ≤ 0 → E-SIM-002
        val_f64(64.0),
        val_f64(7.0),
        val_vec(&[]),
    ];
    let err = seam_eval("std.stochastic.euler_maruyama", &inputs)
        .expect_err("domain error refuses");
    match err {
        EvalFault::CapabilityRefused { capability, code } => {
            assert_eq!(capability, "std.stochastic.euler_maruyama");
            assert!(code.starts_with("E-SIM-002"), "stable code surfaced: {code}");
        }
        other => panic!("expected CapabilityRefused, got {other:?}"),
    }
}
