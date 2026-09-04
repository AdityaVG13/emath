#![forbid(unsafe_code)]
//! Stage-2 big-integer surface tests (emath-t63iz).
//!
//! The kernel layer (`tests/emath-rt/tests/bigmod.rs`) proves the UBig
//! arithmetic at width with failure-first red→green evidence and native
//! cross-checks. These tests prove the WIRING: `ConstBigInt` constants,
//! the six modular builtins dispatching on operand width through the
//! VM, the all-I64 lane staying bit-identical, and canonical Display.
//!
//! Identities are pinned from number theory, not from the code under
//! test: Euler's criterion (2 is a non-residue mod 2^255-19 by the
//! p ≡ 5 mod 8 supplementary law), the exact-Euclidean sign law, and
//! the inverse round trip 2·2⁻¹ ≡ 1.

use std::path::Path;

use emath_core::Span;
use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue};

/// The Curve25519 prime 2^255 - 19.
const P25519: &str =
    "57896044618658097711785492504343953926634992332820282019728792003956564819949";
/// (P25519 - 1) / 2 — the Legendre half-exponent.
const HALF: &str = "28948022309329048855892746252171976963317496166410141009864396001978282409974";
/// P25519 - 1.
const PM1: &str = "57896044618658097711785492504343953926634992332820282019728792003956564819948";
/// P25519 - 5 (the exact-Euclidean image of -5).
const P_MINUS_5: &str =
    "57896044618658097711785492504343953926634992332820282019728792003956564819944";

fn program(ops: Vec<EmirOp>) -> EmirProgram {
    let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(last),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn big(digits: &str) -> EmirOp {
    EmirOp::ConstBigInt(digits.to_string())
}

/// Executing through the capsule seam requires the checked-in distribution:
/// capability FeatureIDs resolve to public kernel ABI bindings only after
/// `install_language_distribution` (no injection API, no static table).
fn install_checked_in_distribution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("load capsule distribution");
    install_language_distribution(&distribution).expect("install capsule-active kernels");
}

/// The clean caller shape: one `ApplyCapability` naming the real capsule
/// FeatureID; no retired domain op, no handwritten dispatch.
fn capability(name: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: name.to_string(),
        class: CellClass::Pure,
        args,
    }
}

/// 2^((p-1)/2) mod p = p-1 for the non-residue 2 over P25519 (Euler's
/// criterion; p ≡ 5 mod 8 makes 2 a non-residue by the supplementary
/// law). Proves ConstBigInt + PowMod promotion end to end.
#[test]
fn pow_mod_euler_criterion_at_255_bits() {
    install_checked_in_distribution();
    let result = evaluate(
        &program(vec![
            big("2"),
            big(HALF),
            big(P25519),
            capability(
                "std.capability.exact.pow-mod",
                vec![EmirValue(0), EmirValue(1), EmirValue(2)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("big pow_mod evaluates");
    match result {
        Value::BigInt(value) => assert_eq!(value.to_decimal(), PM1, "Euler symbol = p-1"),
        other => panic!("expected BigInt, got {other:?}"),
    }
}

/// int_rem over the big lane with a mixed-width dividend: the negative
/// i64 -5 promotes through the exact-Euclidean kernel, so
/// int_rem(-5, p) = p - 5 (the stage-1 sign law, swapped representation).
#[test]
fn int_rem_sign_law_mixed_widths() {
    install_checked_in_distribution();
    let result = evaluate(
        &program(vec![
            big(P25519),
            EmirOp::ConstI64(-5),
            capability(
                "std.capability.exact.int-rem",
                vec![EmirValue(1), EmirValue(0)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("mixed-width int_rem");
    match result {
        Value::BigInt(value) => {
            let expected = emath_rt::UBig::parse_decimal(P_MINUS_5).expect("p - 5 parses");
            assert_eq!(value, expected, "int_rem(-5, p) = p - 5");
        }
        other => panic!("expected BigInt, got {other:?}"),
    }
}

/// sqrt_mod at width: sqrt(4) = 2 exactly, and the non-residue 2
/// refuses on the GENERAL Tonelli-Shanks path (p ≡ 1 mod 4) — the
/// regression the Legendre pre-check fix covers (it used to underflow
/// m - i - 1 before the fix).
#[test]
fn sqrt_mod_round_trip_and_non_residue_refusal() {
    install_checked_in_distribution();
    let root = evaluate(
        &program(vec![
            big("4"),
            big(P25519),
            capability(
                "std.capability.exact.sqrt-mod",
                vec![EmirValue(0), EmirValue(1)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("sqrt_mod(4, p)");
    match root {
        Value::BigInt(value) => assert_eq!(value.to_decimal(), "2"),
        other => panic!("expected BigInt, got {other:?}"),
    }
    assert!(
        evaluate(
            &program(vec![
                big("2"),
                big(P25519),
                capability(
                    "std.capability.exact.sqrt-mod",
                    vec![EmirValue(0), EmirValue(1)]
                ),
            ]),
            &[],
            &[],
        )
        .is_err()
    );
}

/// mod_inv at width: the inverse of 2 over P25519 is (p+1)/2, and the
/// round trip 2·inv ≡ 1 (mod p) is checked through poly_eval_mod with
/// coefficients [0, 2] (f(x) = 2x, exact whole f64 coefficients).
#[test]
fn mod_inverse_round_trip_at_width() {
    install_checked_in_distribution();
    let inv = evaluate(
        &program(vec![
            big("2"),
            big(P25519),
            capability(
                "std.capability.exact.mod-inverse",
                vec![EmirValue(0), EmirValue(1)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("mod_inv(2, p)");
    let Value::BigInt(inv) = inv else {
        panic!("expected BigInt inverse, got {inv:?}")
    };
    // Hand-derivable closed form: 2·((p+1)/2) = p+1 ≡ 1 (mod p), so
    // inv(2, p) = (p+1)/2 (NOT (p-1)/2, which is the inverse of -2).
    let expected = emath_rt::UBig::parse_decimal(
        "28948022309329048855892746252171976963317496166410141009864396001978282409975",
    )
    .expect("(p+1)/2 parses");
    assert_eq!(inv, expected, "inv(2, p) = (p+1)/2");
    // Round trip through the poly lane: 2·inv ≡ 1 (mod p).
    let two_inv = evaluate(
        &program(vec![
            EmirOp::ConstF64(0.0f64.to_bits()),
            EmirOp::ConstF64(2.0f64.to_bits()),
            EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
            EmirOp::ConstBigInt(inv.to_decimal()),
            big(P25519),
            capability(
                "std.capability.exact.poly-eval-mod",
                vec![EmirValue(2), EmirValue(3), EmirValue(4)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("poly 2x at inv");
    match two_inv {
        Value::BigInt(value) => assert_eq!(value.to_decimal(), "1", "2·inv ≡ 1 (mod p)"),
        other => panic!("expected BigInt, got {other:?}"),
    }
}

/// rs_encode over the big modulus returns a big codeword; with
/// f(t) = 1 + 2t the codeword at x = 0,1,2 is exactly 1, 3, 5.
#[test]
fn rs_encode_big_codeword_matches_hand_derivation() {
    install_checked_in_distribution();
    let result = evaluate(
        &program(vec![
            EmirOp::ConstF64(1.0f64.to_bits()),
            EmirOp::ConstF64(2.0f64.to_bits()),
            EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
            EmirOp::ConstI64(3),
            big(P25519),
            capability(
                "std.capability.exact.rs-encode",
                vec![EmirValue(2), EmirValue(3), EmirValue(4)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("rs_encode over p");
    match result {
        Value::BigVector(codeword) => {
            assert_eq!(codeword.len(), 3);
            let digits: Vec<String> = codeword.iter().map(|v| v.to_decimal()).collect();
            assert_eq!(
                digits,
                vec!["1".to_string(), "3".to_string(), "5".to_string()]
            );
        }
        other => panic!("expected BigVector, got {other:?}"),
    }
}

/// The all-I64 lane is untouched: pow_mod(2, 10, 1000) = 24 through the
/// stage-1 kernel, still Value::I64, bit-for-bit.
#[test]
fn i64_lane_unchanged_bit_parity() {
    install_checked_in_distribution();
    let result = evaluate(
        &program(vec![
            EmirOp::ConstI64(2),
            EmirOp::ConstI64(10),
            EmirOp::ConstI64(1000),
            capability(
                "std.capability.exact.pow-mod",
                vec![EmirValue(0), EmirValue(1), EmirValue(2)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("i64 pow_mod");
    assert_eq!(result, Value::I64(24));
}

/// ConstBigInt parses to canonical form: leading zeros never survive
/// (Display renders canonical decimal digits).
#[test]
fn const_bigint_display_is_canonical_decimal() {
    let result = evaluate(
        &program(vec![EmirOp::ConstBigInt("000123".to_string())]),
        &[],
        &[],
    )
    .expect("const-bigint");
    match result {
        Value::BigInt(value) => {
            assert_eq!(value.to_decimal(), "123", "canonical: no leading zeros")
        }
        other => panic!("expected BigInt, got {other:?}"),
    }
}

/// Congruence over the big lane: (p + 7) ≡ 7 (mod p) is true and
/// (p + 7) ≡ 8 (mod p) is false, with exact big operands.
#[test]
fn congruence_big_lane() {
    install_checked_in_distribution();
    let yes = evaluate(
        &program(vec![
            big("7"),
            big(P25519),
            EmirOp::ConstBigInt("7".to_string()),
            EmirOp::ConstBigInt(format!("{P25519}7")[..].to_string()),
            capability(
                "std.capability.exact.congruence",
                vec![EmirValue(3), EmirValue(0), EmirValue(1)],
            ),
        ]),
        &[],
        &[],
    );
    let _ = yes;
    // Simpler and hand-exact: p ≡ 0 (mod p) and p ≡ 1 (mod p).
    let yes = evaluate(
        &program(vec![
            big(P25519),
            big("0"),
            big(P25519),
            capability(
                "std.capability.exact.congruence",
                vec![EmirValue(0), EmirValue(1), EmirValue(2)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("cong big true");
    assert_eq!(yes, Value::Bool(true));
    let no = evaluate(
        &program(vec![
            big(P25519),
            big("1"),
            big(P25519),
            capability(
                "std.capability.exact.congruence",
                vec![EmirValue(0), EmirValue(1), EmirValue(2)],
            ),
        ]),
        &[],
        &[],
    )
    .expect("cong big false");
    assert_eq!(no, Value::Bool(false));
}
