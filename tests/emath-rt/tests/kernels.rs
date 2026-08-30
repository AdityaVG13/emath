//! Conformance tests for the emath-rt kernel library.
//!
//! Intent: prove the kernels that generated crates and the interpreter
//! share agree with hand-computed values, preserve the documented
//! historical semantics (zip truncation, boundary mirroring), and keep
//! the embeddable `SOURCE` paste-safe.

use emath_rt::{
    EdgePolicy, EinsumError, EinsumIn, IndexError, SOURCE, SliceAxis, Tensor, cmp_i64_f64,
    complex_exp, complex_ln, complex_sqrt, einsum_as_matrix, einsum_as_scalar, einsum_checked,
    einsum_output_rank, eq_i64_f64, factorial, fold_add, fold_add_i64, fold_all, fold_any,
    fold_mul, fold_mul_i64, hamming_distance, mat_add, mat_index_checked, mat_mul_mat, mat_mul_vec,
    mat_scale, mat_sub, mat_transpose, mod_inv, poly_eval_mod, rs_encode, sample_limit, simpson,
    stencil_1d, stencil_2d, stencil_3d_checked, tensor_add, tensor_slice_as_matrix, trapezoid_sum,
    vec_add, vec_dot, vec_index_checked, vec_norm, vec_scale, vec_sub,
};

// ── vectors / matrices / tensors ──────────────────────────────────────────

#[test]
fn vector_kernels_match_hand_values() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![4.0, 5.0, 6.0];
    assert_eq!(vec_add(&a, &b), vec![5.0, 7.0, 9.0]);
    assert_eq!(vec_sub(&a, &b), vec![-3.0, -3.0, -3.0]);
    assert_eq!(vec_scale(&a, 2.0), vec![2.0, 4.0, 6.0]);
    assert_eq!(vec_dot(&a, &b), 32.0);
    assert!((vec_norm(&vec![3.0, 4.0]) - 5.0).abs() < 1e-12);
}

#[test]
fn vector_kernels_keep_zip_truncation_semantics() {
    // Generated inline code used `zip`, which drops extras past the
    // shorter operand. The kernels must preserve that exact behavior.
    assert_eq!(vec_add(&[1.0, 2.0, 3.0], &[10.0]), vec![11.0]);
    assert_eq!(vec_dot(&[1.0, 2.0, 3.0], &[10.0]), 10.0);
}

#[test]
fn matrix_kernels_match_hand_values() {
    let m1 = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let m2 = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
    assert_eq!(mat_add(&m1, &m2), vec![vec![6.0, 8.0], vec![10.0, 12.0]]);
    assert_eq!(mat_sub(&m2, &m1), vec![vec![4.0, 4.0], vec![4.0, 4.0]]);
    assert_eq!(mat_scale(&m1, 2.0), vec![vec![2.0, 4.0], vec![6.0, 8.0]]);
    assert_eq!(mat_mul_vec(&m1, &vec![1.0, 1.0]), vec![3.0, 7.0]);
    assert_eq!(
        mat_mul_mat(&m1, &m2),
        vec![vec![19.0, 22.0], vec![43.0, 50.0]]
    );
    assert_eq!(mat_transpose(&m1), vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
    // Empty matrix stays empty under transpose (historical guard).
    assert_eq!(mat_transpose(&[]), Vec::<Vec<f64>>::new());
}

#[test]
fn tensor_kernels_are_flat_zip() {
    assert_eq!(tensor_add(&[1.0, 2.0], &[3.0, 4.0]), vec![4.0, 6.0]);
    assert_eq!(tensor_add(&[1.0, 2.0, 3.0], &[1.0]), vec![2.0]);
}

#[test]
fn index_checked_refuses_oob_negative_and_fractional() {
    let v = vec![1.0, 2.0, 3.0];
    assert_eq!(vec_index_checked(&v, 1.0), Ok(2.0));
    assert_eq!(
        vec_index_checked(&v, 3.0),
        Err(IndexError::OutOfBounds { index: 3, len: 3 })
    );
    assert_eq!(
        vec_index_checked(&v, -1.0),
        Err(IndexError::OutOfBounds { index: -1, len: 3 })
    );
    assert_eq!(
        vec_index_checked(&v, 1.5),
        Err(IndexError::OutOfBounds { index: 1, len: 3 })
    );
    let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    assert_eq!(mat_index_checked(&m, 1.0, 0.0), Ok(3.0));
    assert_eq!(
        mat_index_checked(&m, 2.0, 0.0),
        Err(IndexError::OutOfBounds { index: 2, len: 2 })
    );
}

#[test]
fn tensor_slice_first_face_matches_tensor_face_example() {
    // t = [[[1,2],[3,4]], [[5,6],[7,8]]]; t[0, :, :] is the first 2×2 face.
    let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let axes = [
        SliceAxis::Point(0.0),
        SliceAxis::Range {
            start: 0.0,
            end: 2.0,
        },
        SliceAxis::Range {
            start: 0.0,
            end: 2.0,
        },
    ];
    assert_eq!(
        tensor_slice_as_matrix(&[2, 2, 2], &data, &axes),
        Ok(vec![vec![1.0, 2.0], vec![3.0, 4.0]])
    );
    let oob = [
        SliceAxis::Point(2.0),
        SliceAxis::Range {
            start: 0.0,
            end: 2.0,
        },
        SliceAxis::Range {
            start: 0.0,
            end: 2.0,
        },
    ];
    assert_eq!(
        tensor_slice_as_matrix(&[2, 2, 2], &data, &oob),
        Err(IndexError::OutOfBounds { index: 2, len: 2 })
    );
}

#[test]
fn einsum_matches_matmul_dot_and_diag() {
    let a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
    let ops = [EinsumIn::einsum_operand(&a), EinsumIn::einsum_operand(&b)];
    assert_eq!(einsum_as_matrix("ik,kj->ij", &ops), mat_mul_mat(&a, &b));
    assert_eq!(einsum_output_rank("ik,kj"), 2);
    assert_eq!(einsum_as_matrix("ik,kj", &ops), mat_mul_mat(&a, &b));

    let u = vec![1.0, 2.0, 3.0];
    let v = vec![4.0, 5.0, 6.0];
    let dots = [EinsumIn::einsum_operand(&u), EinsumIn::einsum_operand(&v)];
    assert_eq!(einsum_as_scalar("i,i->", &dots), vec_dot(&u, &v));

    let diag = [EinsumIn::einsum_operand(&u)];
    assert_eq!(
        einsum_as_matrix("i->ii", &diag),
        vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 2.0, 0.0],
            vec![0.0, 0.0, 3.0],
        ]
    );
}

#[test]
fn empty_vector_norm_and_einsum_are_total() {
    assert_eq!(
        vec_norm(&[]).to_bits(),
        0.0f64.to_bits(),
        "||[]|| must be +0.0, not -0.0 from empty f64 sum"
    );
    let empty = Vec::<f64>::new();
    let ops = [
        EinsumIn::einsum_operand(&empty),
        EinsumIn::einsum_operand(&empty),
    ];
    assert_eq!(
        einsum_checked("i,i->", &ops),
        Ok((Vec::<usize>::new(), vec![0.0]))
    );
    assert_eq!(einsum_as_scalar("i,i->", &ops), 0.0);
}

#[test]
fn einsum_checked_refuses_dimension_mismatch() {
    let a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
    let b = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let ops = [EinsumIn::einsum_operand(&a), EinsumIn::einsum_operand(&b)];
    assert_eq!(
        einsum_checked("ik,kj->ij", &ops),
        Err(EinsumError::Arithmetic("einsum dimension mismatch"))
    );
}

// ── stencils ──────────────────────────────────────────────────────────────

#[test]
fn stencil_1d_clamp_replicates_boundary() {
    // u = [1, 2, 3, 4], weights [1, 0.5] (center 0).
    // i=0: 1.0 + 0.5*2 = 2.0; i=1: 2.0 + 1.5 = 3.5; i=2: 3.0 + 2.0 = 5.0;
    // i=3: tap k=1 reads raw=4 > last=3 and clamps to cell 3: 4.0 + 2.0 = 6.0.
    let u = vec![1.0, 2.0, 3.0, 4.0];
    let w = vec![1.0, 0.5];
    assert_eq!(
        stencil_1d(&u, &w, 0, EdgePolicy::Clamp),
        vec![2.0, 3.5, 5.0, 6.0]
    );
}

#[test]
fn complex_sqrt_ln_exp_principal() {
    let (re, im) = complex_sqrt(-1.0, 0.0);
    assert!(re.abs() < 1e-12 && (im - 1.0).abs() < 1e-12);
    let (re, im) = complex_ln(-1.0, 0.0);
    assert!(re.abs() < 1e-12 && (im - std::f64::consts::PI).abs() < 1e-12);
    let (re, im) = complex_exp(0.0, std::f64::consts::PI);
    assert!((re + 1.0).abs() < 1e-12 && im.abs() < 1e-12);
}

#[test]
fn neumann_mirror_conserves_trapezoid_heat() {
    // Cell-center even reflection does not conserve the unweighted sum when
    // heat sits on the boundary; it conserves the trapezoidal (half-cell)
    // inner product. Clamp conserves the unweighted sum.
    let u = vec![1.0, 0.0, 0.0, 0.0, 0.0];
    let w = vec![1.0, -2.0, 1.0];
    let l_n = stencil_1d(&u, &w, 1, EdgePolicy::Neumann);
    let l_c = stencil_1d(&u, &w, 1, EdgePolicy::Clamp);
    let sum_n: f64 = l_n.iter().sum();
    let sum_c: f64 = l_c.iter().sum();
    assert!(
        (sum_c).abs() < 1e-12,
        "Clamp laplacian must conserve Σu, got {sum_c}"
    );
    assert!(
        (sum_n + 1.0).abs() < 1e-12,
        "Neumann unweighted sum on a boundary hotspot is -1, got {sum_n}"
    );
    assert!(
        trapezoid_sum(&l_n).abs() < 1e-12,
        "Neumann must conserve trapezoidal heat, got {}",
        trapezoid_sum(&l_n)
    );
}

#[test]
fn stencil_1d_neumann_mirrors_boundary() {
    // Weights [-1, 2, -1] (center 1): at i=0 taps raw = -1, 0, 1; the
    // raw=-1 tap mirrors to cell 1: -u[1] + 2u[0] - u[1].
    let u = vec![1.0, 2.0, 3.0];
    let w = vec![-1.0, 2.0, -1.0];
    assert_eq!(
        stencil_1d(&u, &w, 1, EdgePolicy::Neumann),
        vec![-2.0, 0.0, 2.0]
    );
}

#[test]
fn stencil_1d_onesided_is_exact_on_a_linear_field() {
    // Central first-difference weights [-1/2, 0, 1/2] on u = [0,1,2,3,4].
    // OneSided ghost u[-1] = -1, u[5] = 5, so the derivative is 1
    // everywhere. Clamp on the same stencil returns 0.5 at the edges.
    let u = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let w = vec![-0.5, 0.0, 0.5];
    assert_eq!(
        stencil_1d(&u, &w, 1, EdgePolicy::OneSided),
        vec![1.0, 1.0, 1.0, 1.0, 1.0]
    );
    assert_eq!(
        stencil_1d(&u, &w, 1, EdgePolicy::Clamp),
        vec![0.5, 1.0, 1.0, 1.0, 0.5]
    );
}

#[test]
fn stencil_1d_dirichlet_holds_boundary() {
    let u = vec![1.0, 2.0, 3.0];
    let w = vec![-1.0, 2.0, -1.0];
    let edge = EdgePolicy::Dirichlet {
        left: 0.0,
        right: 0.0,
    };
    // i=0: -left + 2u[0] - u[1] = 0 + 2 - 2 = 0; i=2: -u[1] + 2u[2] - right = 4.
    assert_eq!(stencil_1d(&u, &w, 1, edge), vec![0.0, 0.0, 4.0]);
}

#[test]
fn stencil_2d_clamp_matches_manual_convolution() {
    let u = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    // Laplace-ish: center weight 4, orthogonal -1, corners 0.
    let w = [0.0, -1.0, 0.0, -1.0, 4.0, -1.0, 0.0, -1.0, 0.0];
    let out = stencil_2d(&u, &w, (1, 1), EdgePolicy::Clamp);
    // Clamp duplicates the boundary cell per tap, so edge taps accumulate
    // multiple contributions of the same value:
    // cell (0,0): -u01 - u10 + 4*u00 - u01 - u10 = -2 - 3 + 4 - 2 - 3 = -3
    // cell (0,1): -u00 - u01 - u01 - u11 + 4*u01 = -1 - 2 - 2 - 4 + 8 = -1
    // cell (1,0): -u00 - u10 - u11 - u10 + 4*u10 = -1 - 3 - 4 - 3 + 12 = 1
    // cell (1,1): -u10 - u01 - u11 - u11 + 4*u11 = -3 - 2 - 4 - 4 + 16 = 3
    let expected = vec![vec![-3.0, -1.0], vec![1.0, 3.0]];
    assert_eq!(out, expected);
}

#[test]
fn stencil_2d_onesided_column_ramp_is_one() {
    // u[r][c] = c; central du/dc weights in the middle row.
    let u = vec![
        vec![0.0, 1.0, 2.0],
        vec![0.0, 1.0, 2.0],
        vec![0.0, 1.0, 2.0],
    ];
    let w = [0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.0, 0.0];
    let out = stencil_2d(&u, &w, (1, 1), EdgePolicy::OneSided);
    assert_eq!(
        out,
        vec![
            vec![1.0, 1.0, 1.0],
            vec![1.0, 1.0, 1.0],
            vec![1.0, 1.0, 1.0]
        ]
    );
}

#[test]
fn stencil_3d_anisotropic_laplacian_matches_quadratic_center() {
    // u(x,y,z) = x² + y² + z² on a 3³ grid. Each axis contributes 2
    // at the sole interior cell, even with unequal physical spacings.
    let spacing = [0.5f64, 1.0, 2.0];
    let shape = vec![3, 3, 3];
    let data = (0..3)
        .flat_map(|x| {
            (0..3).flat_map(move |y| {
                (0..3).map(move |z| {
                    let px = x as f64 * spacing[0];
                    let py = y as f64 * spacing[1];
                    let pz = z as f64 * spacing[2];
                    px * px + py * py + pz * pz
                })
            })
        })
        .collect();
    let mut weights = [0.0; 27];
    for (negative, center, positive, h) in [
        (4, 13, 22, spacing[0]),
        (10, 13, 16, spacing[1]),
        (12, 13, 14, spacing[2]),
    ] {
        let inv = 1.0 / (h * h);
        weights[negative] = inv;
        weights[center] -= 2.0 * inv;
        weights[positive] = inv;
    }
    let out = stencil_3d_checked(
        &Tensor { shape, data },
        &weights,
        (1, 1, 1),
        EdgePolicy::Clamp,
    )
    .unwrap();
    assert!((out.data[13] - 6.0).abs() < 1e-12);
}

// ── number theory ─────────────────────────────────────────────────────────

#[test]
fn mod_inv_is_a_true_inverse() {
    for m in [7i64, 11, 13, 101, 1009] {
        for a in 1..m.min(20) {
            let inv = mod_inv(a, m);
            assert_eq!((a * inv).rem_euclid(m), 1, "a={a} m={m}");
            assert_eq!(mod_inv(inv, m), a, "mod_inv²({a},{m})");
        }
    }
}

#[test]
fn poly_eval_mod_matches_direct_substitution() {
    // p(x) = 2 + 3x + x^2 over GF(7); at x=5: 2 + 15 + 25 = 42 ≡ 0.
    let coeffs = vec![2.0, 3.0, 1.0];
    assert_eq!(poly_eval_mod(&coeffs, 5, 7), 0);
    // x=1: 2 + 3 + 1 = 6.
    assert_eq!(poly_eval_mod(&coeffs, 1, 7), 6);
}

#[test]
fn poly_eval_mod_refuses_nan_inf_subnormal_coeffs() {
    use emath_rt::poly_eval_mod_checked;
    assert!(poly_eval_mod_checked(&[f64::NAN], 1, 7).is_err());
    assert!(poly_eval_mod_checked(&[f64::INFINITY], 1, 7).is_err());
    assert!(poly_eval_mod_checked(&[f64::from_bits(1)], 1, 7).is_err());
    assert_eq!(poly_eval_mod_checked(&[2.0], 1, 7).unwrap(), 2);
}

#[test]
fn rs_encode_produces_expected_codeword() {
    // RS over GF(7), message [1, 2] (constants 1, 2): evaluations at
    // x=0..3 give 1, 3, 5, 0.
    let codeword = rs_encode(&vec![1.0, 2.0], 4, 7);
    assert_eq!(codeword, vec![1.0, 3.0, 5.0, 0.0]);
}

#[test]
fn hamming_distance_counts_bit_exact_differences() {
    assert_eq!(hamming_distance(&[1.0, 2.0, 3.0], &[1.0, 5.0, 3.0]), 1);
    assert_eq!(hamming_distance(&[1.0, 2.0], &[1.0, 2.0]), 0);
    // -0.0 and 0.0 differ at the bit level.
    assert_eq!(hamming_distance(&[0.0], &[-0.0]), 1);
}

#[test]
fn mixed_i64_f64_compare_is_exact() {
    use core::cmp::Ordering;
    let two53 = 1i64 << 53;
    let past = two53 + 1;
    // Widening `as f64` would claim these equal.
    assert!(!eq_i64_f64(past, two53 as f64));
    assert_eq!(cmp_i64_f64(past, two53 as f64), Some(Ordering::Greater));
    assert!(eq_i64_f64(two53, two53 as f64));
    assert!(eq_i64_f64(0, -0.0));
    assert!(eq_i64_f64(0, 0.0));
    assert!(!eq_i64_f64(1, f64::NAN));
    assert_eq!(cmp_i64_f64(1, f64::NAN), None);
    assert_eq!(cmp_i64_f64(i64::MAX, f64::INFINITY), Some(Ordering::Less));
    // 2^63 is i64::MAX as f64, but is not an i64 value.
    assert!(!eq_i64_f64(i64::MAX, 9_223_372_036_854_775_808.0));
    assert_eq!(
        cmp_i64_f64(i64::MAX, 9_223_372_036_854_775_808.0),
        Some(Ordering::Less)
    );
    assert!(eq_i64_f64(i64::MIN, i64::MIN as f64));
}

#[test]
fn factorial_edges() {
    assert_eq!(factorial(0), 1);
    assert_eq!(factorial(1), 1);
    assert_eq!(factorial(5), 120);
    assert_eq!(factorial(20), 2_432_902_008_176_640_000);
}

// ── higher-order drivers ──────────────────────────────────────────────────

#[test]
fn folds_accumulate_in_documented_order() {
    // Sum of squares over [1, 4): 1 + 4 + 9 = 14.
    assert_eq!(fold_add(&|x| x * x, 1, 4, 0.0), 14.0);
    // Product over [1, 4): 1 * 2 * 3 = 6.
    assert_eq!(fold_mul(&|x| x, 1, 4, 1.0), 6.0);
    // forall x in [1, 5): x < 5 (true), x < 4 (false starting at 4).
    assert!(fold_all(&|x| x < 5.0, 1, 5, true));
    assert!(!fold_all(&|x| x < 4.0, 1, 5, true));
    assert!(fold_any(&|x| x == 4.0, 1, 5, false));
    // Empty range: identity accumulator.
    assert_eq!(fold_add(&|x| x, 5, 5, 7.0), 7.0);
    // Exact 20! — the f64 fold cannot be the source of truth past 2^53.
    assert_eq!(fold_mul_i64(&|i| i, 1, 21, 1), 2_432_902_008_176_640_000);
    assert_eq!(fold_add_i64(&|i| i, 1, 5, 0), 10);
    assert_eq!(fold_mul_i64(&|i| i, 5, 5, 7), 7);
}

#[test]
fn simpson_exact_on_quadratics() {
    // Simpson is exact for degree <= 3; int_0^1 x^2 dx = 1/3.
    let v = simpson(&|x| x * x, 0.0, 1.0, 64);
    assert!((v - 1.0 / 3.0).abs() < 1e-12);
    // int_0^pi sin x dx = 2.
    let v = simpson(&|x| x.sin(), 0.0, std::f64::consts::PI, 64);
    assert!((v - 2.0).abs() < 1e-6);
}

#[test]
fn sample_limit_recovers_sinc_at_zero() {
    // sin(x)/x -> 1 as x -> 0. One-sided sampling (from above) walks the
    // geometric progression 1e-1..1e-12 and returns the first sample
    // agreeing with its predecessor to 1%; at h=0.01 the value is
    // sin(0.01)/0.01 ≈ 0.99983 (within 2e-4 of 1).
    let l = sample_limit(&|x| x.sin() / x, 0.0, 1.0);
    assert!((l - 1.0).abs() < 1e-3);
}

#[test]
fn sample_limit_respects_direction() {
    // f(x) = |x| at 0 from above: limit x -> 1-ish samples, all positive.
    let l = sample_limit(&|x| x.abs(), 0.0, 1.0);
    assert!(l >= 0.0);
    assert!((l - 0.0).abs() < 1e-3);
}

// ── embedding safety ──────────────────────────────────────────────────────

#[test]
fn embedded_source_bans_imports_and_unsafe() {
    // No inner attributes may be embedded (`#![forbid]`, `#![no_std]`,
    // ... would break module embedding; hosts that strip `#![...]` lines
    // would also strip them mid-module).
    assert!(
        !SOURCE.contains("#!["),
        "inner attributes must not be embedded"
    );
    assert!(SOURCE.contains("pub fn vec_add"), "kernels must be public");
    assert!(SOURCE.contains("pub enum EdgePolicy"));
    assert!(SOURCE.contains("pub fn einsum_checked"));
    assert!(SOURCE.contains("pub fn vec_index_checked"));
    assert!(SOURCE.contains("pub fn tensor_slice_checked"));
    assert!(SOURCE.contains("pub struct Tensor"));
    // The embedded module must stay std-only: no external crate paths
    // and no unsafe code. Doc comments may mention "unsafe" (e.g. a
    // crate's `forbid(unsafe_code)` policy), so comment lines are
    // skipped and only real unsafe constructs fail the guard.
    for line in SOURCE.lines() {
        let trimmed = line.trim_start();
        assert!(
            !trimmed.starts_with("use ") && !trimmed.starts_with("extern "),
            "embedded source must not import: {line}"
        );
        if trimmed.starts_with("//") {
            continue;
        }
        let is_unsafe_code = ["unsafe fn", "unsafe impl", "unsafe trait", "unsafe mod", "unsafe {"]
            .iter()
            .any(|pat| trimmed.contains(pat));
        assert!(
            !is_unsafe_code,
            "embedded source must not contain unsafe code: {line}"
        );
    }
}

// ── exact integer nullspace (rymw generic primitive) ──────────────────────

#[test]
fn int_nullvector_combustion_thermite_hydrogenation() {
    use emath_rt::primitive_int_nullvector;
    // 2 H2 + O2 -> 2 H2O (rows H, O; cols H2, O2, H2O).
    let combustion = vec![vec![2, 0, 2], vec![0, 2, 1]];
    assert_eq!(
        primitive_int_nullvector(&combustion).expect("combustion computes"),
        Some(vec![2, 1, -2])
    );
    // Fe2O3 + 2 Al -> Al2O3 + 2 Fe (rows Fe, O, Al; cols Fe2O3, Al, Al2O3, Fe).
    let thermite = vec![vec![2, 0, 0, 1], vec![3, 0, 3, 0], vec![0, 1, 2, 0]];
    assert_eq!(
        primitive_int_nullvector(&thermite).expect("thermite computes"),
        Some(vec![1, 2, -1, -2])
    );
    // C2H4 + H2 -> C2H6 (rows C, H; cols C2H4, H2, C2H6).
    let hydrogenation = vec![vec![2, 0, 2], vec![4, 2, 6]];
    assert_eq!(
        primitive_int_nullvector(&hydrogenation).expect("hydrogenation computes"),
        Some(vec![1, 1, -1])
    );
}

#[test]
fn int_nullvector_canonical_sign_and_primitivity() {
    use emath_rt::primitive_int_nullvector;
    // Column order reversed: species H2O, H2, O2 give the raw null
    // vector [2, -1, -2], already first-nonzero-positive — the
    // canonical form of 2 H2O -> 2 H2 + O2.
    let reversed = vec![vec![2, 0, 2], vec![1, 2, 0]];
    assert_eq!(
        primitive_int_nullvector(&reversed).expect("computes"),
        Some(vec![2, -1, -2])
    );
    // H2O2 decomposition (2 H2O2 -> 2 H2O + O2): rows H, O; cols
    // H2O2, H2O, O2. Null vector [2, -2, -1] is already primitive.
    let scaled = vec![vec![2, 2, 0], vec![2, 1, 2]];
    assert_eq!(
        primitive_int_nullvector(&scaled).expect("computes"),
        Some(vec![2, -2, -1])
    );
    // Doubling the composition yields the same primitive null vector:
    // the free-variable basis normalization is scale-invariant.
    let doubled = vec![vec![4, 4, 0], vec![4, 2, 4]];
    assert_eq!(
        primitive_int_nullvector(&doubled).expect("computes"),
        Some(vec![2, -2, -1])
    );
}

#[test]
fn int_nullvector_refuses_non_one_dimensional() {
    use emath_rt::primitive_int_nullvector;
    // H2 + He: each element appears in exactly one species — zero
    // nullspace.
    let impossible = vec![vec![2, 0], vec![0, 1]];
    assert_eq!(
        primitive_int_nullvector(&impossible).expect("computes"),
        None
    );
    // Combustion plus H2O2: two independent equations in four species —
    // two-dimensional nullspace.
    let underdetermined = vec![vec![2, 0, 2, 0], vec![0, 2, 1, 2]];
    assert_eq!(
        primitive_int_nullvector(&underdetermined).expect("computes"),
        None
    );
    // A species with zero atoms in every element contributes a free
    // direction — dimension at least two.
    let zero_column = vec![vec![2, 0, 2, 0], vec![0, 1, 1, 0]];
    assert_eq!(
        primitive_int_nullvector(&zero_column).expect("computes"),
        None
    );
}

#[test]
fn int_nullvector_row_scaling_and_permutation_invariant() {
    use emath_rt::primitive_int_nullvector;
    // Row scaling preserves the nullspace.
    let combustion = vec![vec![2, 0, 2], vec![0, 2, 1]];
    let scaled_rows = vec![vec![4, 0, 4], vec![0, 6, 3]];
    assert_eq!(
        primitive_int_nullvector(&scaled_rows).expect("computes"),
        primitive_int_nullvector(&combustion).expect("computes")
    );
    // Column permutation permutes the null vector identically:
    // species O2, H2, H2O (rows H, O) give [2, 1, -1] for
    // 2 H2 + O2 -> 2 H2O read back through the permuted columns.
    let permuted = vec![vec![0, 2, 2], vec![1, 0, 2]]; // O2, H2, H2O
    assert_eq!(
        primitive_int_nullvector(&permuted).expect("computes"),
        Some(vec![2, 1, -1])
    );
}

#[test]
fn int_nullvector_rejects_ragged_and_overflow() {
    use emath_rt::primitive_int_nullvector;
    assert!(primitive_int_nullvector(&[vec![1, 2], vec![3]]).is_err());
    assert!(primitive_int_nullvector(&[]).is_err());
    assert!(primitive_int_nullvector(&[vec![]]).is_err());
    // i128 overflow: entries near i64::MAX blow past 2^127 during
    // elimination.
    let huge = vec![
        vec![9_223_372_036_854_775_000_i64, 1, 1],
        vec![1, 9_223_372_036_854_775_000, 1],
    ];
    assert!(primitive_int_nullvector(&huge).is_err());
}
