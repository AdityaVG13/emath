//! Conformance tests for the emath-rt kernel library: hand-computed values,
//! historical semantics (zip truncation, boundary mirroring), paste-safe `SOURCE`.

use emath_rt::{
    EinsumError, EinsumIn, IndexError, SOURCE, SliceAxis, cmp_i64_f64,
    complex_exp, complex_ln, complex_sqrt, einsum_as_matrix, einsum_as_scalar, einsum_checked,
    einsum_output_rank, eq_i64_f64, factorial, fold_add, fold_add_i64, fold_all, fold_any,
    fold_mul, fold_mul_i64, hamming_distance, mat_add, mat_index_checked, mat_mul_mat, mat_mul_vec,
    mat_scale, mat_sub, mat_transpose, mod_inv, poly_eval_mod, rs_encode, sample_limit, simpson,
    tensor_add, tensor_slice_as_matrix,
    vec_add, vec_dot, vec_index_checked, vec_norm, vec_scale, vec_sub,
};
use emath_test_harness::Probe;

#[test]
fn kernels() {
    let mut p = Probe::new("rt kernels match hand values and keep historical semantics");
    p.case("vectors", |p| {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        p.eq("add", vec_add(&a, &b), vec![5.0, 7.0, 9.0]);
        p.eq("sub", vec_sub(&a, &b), vec![-3.0, -3.0, -3.0]);
        p.eq("scale", vec_scale(&a, 2.0), vec![2.0, 4.0, 6.0]);
        p.eq("dot", vec_dot(&a, &b), 32.0);
        p.close("norm", vec_norm(&vec![3.0, 4.0]), 5.0, 1e-12);
        p.eq("zip-add", vec_add(&[1.0, 2.0, 3.0], &[10.0]), vec![11.0]);
        p.eq("zip-dot", vec_dot(&[1.0, 2.0, 3.0], &[10.0]), 10.0);
        p.eq("flat-add", tensor_add(&[1.0, 2.0], &[3.0, 4.0]), vec![4.0, 6.0]);
        p.eq("flat-zip", tensor_add(&[1.0, 2.0, 3.0], &[1.0]), vec![2.0]);
    });
    p.case("matrices", |p| {
        let m1 = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let m2 = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        p.eq("add", mat_add(&m1, &m2), vec![vec![6.0, 8.0], vec![10.0, 12.0]]);
        p.eq("sub", mat_sub(&m2, &m1), vec![vec![4.0, 4.0], vec![4.0, 4.0]]);
        p.eq("scale", mat_scale(&m1, 2.0), vec![vec![2.0, 4.0], vec![6.0, 8.0]]);
        p.eq("mul-vec", mat_mul_vec(&m1, &vec![1.0, 1.0]), vec![3.0, 7.0]);
        p.eq("mul-mat", mat_mul_mat(&m1, &m2), vec![vec![19.0, 22.0], vec![43.0, 50.0]]);
        p.eq("transpose", mat_transpose(&m1), vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
        p.eq("empty-t", mat_transpose(&[]), Vec::<Vec<f64>>::new());
    });
    p.case("index", |p| {
        let v = vec![1.0, 2.0, 3.0];
        p.eq("hit", vec_index_checked(&v, 1.0), Ok(2.0));
        p.eq("oob", vec_index_checked(&v, 3.0), Err(IndexError::OutOfBounds { index: 3, len: 3 }));
        p.eq("neg", vec_index_checked(&v, -1.0), Err(IndexError::OutOfBounds { index: -1, len: 3 }));
        p.eq("frac", vec_index_checked(&v, 1.5), Err(IndexError::OutOfBounds { index: 1, len: 3 }));
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        p.eq("mat-hit", mat_index_checked(&m, 1.0, 0.0), Ok(3.0));
        p.eq("mat-oob", mat_index_checked(&m, 2.0, 0.0), Err(IndexError::OutOfBounds { index: 2, len: 2 }));
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let axes = [SliceAxis::Point(0.0), SliceAxis::Range { start: 0.0, end: 2.0 }, SliceAxis::Range { start: 0.0, end: 2.0 }];
        p.eq("face", tensor_slice_as_matrix(&[2, 2, 2], &data, &axes), Ok(vec![vec![1.0, 2.0], vec![3.0, 4.0]]));
        let oob = [SliceAxis::Point(2.0), SliceAxis::Range { start: 0.0, end: 2.0 }, SliceAxis::Range { start: 0.0, end: 2.0 }];
        p.eq("face-oob", tensor_slice_as_matrix(&[2, 2, 2], &data, &oob), Err(IndexError::OutOfBounds { index: 2, len: 2 }));
    });
    p.case("einsum", |p| {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];
        let ops = [EinsumIn::einsum_operand(&a), EinsumIn::einsum_operand(&b)];
        p.eq("matmul", einsum_as_matrix("ik,kj->ij", &ops), mat_mul_mat(&a, &b));
        p.eq("rank", einsum_output_rank("ik,kj"), 2);
        let u = vec![1.0, 2.0, 3.0];
        let v = vec![4.0, 5.0, 6.0];
        let dots = [EinsumIn::einsum_operand(&u), EinsumIn::einsum_operand(&v)];
        p.eq("dot", einsum_as_scalar("i,i->", &dots), vec_dot(&u, &v));
        p.eq(
            "diag",
            einsum_as_matrix("i->ii", &[EinsumIn::einsum_operand(&u)]),
            vec![vec![1.0, 0.0, 0.0], vec![0.0, 2.0, 0.0], vec![0.0, 0.0, 3.0]],
        );
        p.eq("empty-norm-bits", vec_norm(&[]).to_bits(), 0.0f64.to_bits());
        let empty = Vec::<f64>::new();
        let eops = [EinsumIn::einsum_operand(&empty), EinsumIn::einsum_operand(&empty)];
        p.eq("empty-einsum", einsum_checked("i,i->", &eops), Ok((Vec::<usize>::new(), vec![0.0])));
        let c = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let d = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let bad = [EinsumIn::einsum_operand(&c), EinsumIn::einsum_operand(&d)];
        p.eq("mismatch", einsum_checked("ik,kj->ij", &bad), Err(EinsumError::Arithmetic("einsum dimension mismatch")));
    });
    p.case("complex", |p| {
        let (re, im) = complex_sqrt(-1.0, 0.0);
        p.close("sqrt-re", re, 0.0, 1e-12);
        p.close("sqrt-im", im, 1.0, 1e-12);
        let (re, im) = complex_ln(-1.0, 0.0);
        p.close("ln-re", re, 0.0, 1e-12);
        p.close("ln-im", im, std::f64::consts::PI, 1e-12);
        let (re, im) = complex_exp(0.0, std::f64::consts::PI);
        p.close("exp-re", re, -1.0, 1e-12);
        p.close("exp-im", im, 0.0, 1e-12);
    });
    p.case("numtheory", |p| {
        for m in [7i64, 11, 13, 101, 1009] {
            for a in 1..m.min(20) {
                let inv = mod_inv(a, m);
                p.eq(format!("inv/{m}/{a}"), (a * inv).rem_euclid(m), 1);
                p.eq(format!("inv2/{m}/{a}"), mod_inv(inv, m), a);
            }
        }
        p.eq("poly0", poly_eval_mod(&vec![2.0, 3.0, 1.0], 5, 7), 0);
        p.eq("poly1", poly_eval_mod(&vec![2.0, 3.0, 1.0], 1, 7), 6);
        p.demand("poly-nan", emath_rt::poly_eval_mod_checked(&[f64::NAN], 1, 7).is_err(), "NaN refuses");
        p.demand("poly-inf", emath_rt::poly_eval_mod_checked(&[f64::INFINITY], 1, 7).is_err(), "inf refuses");
        p.eq("rs", rs_encode(&vec![1.0, 2.0], 4, 7), vec![1.0, 3.0, 5.0, 0.0]);
        p.eq("ham1", hamming_distance(&[1.0, 2.0, 3.0], &[1.0, 5.0, 3.0]), 1);
        p.eq("ham0", hamming_distance(&[1.0, 2.0], &[1.0, 2.0]), 0);
        p.eq("ham-neg0", hamming_distance(&[0.0], &[-0.0]), 1);
        let two53 = 1i64 << 53;
        p.demand("past-ne", !eq_i64_f64(two53 + 1, two53 as f64), "widening lie refused");
        p.eq("past-cmp", cmp_i64_f64(two53 + 1, two53 as f64), Some(core::cmp::Ordering::Greater));
        p.demand("exact", eq_i64_f64(two53, two53 as f64), "2^53 exact");
        p.eq("fact0", factorial(0), 1);
        p.eq("fact5", factorial(5), 120);
        p.eq("fact20", factorial(20), 2_432_902_008_176_640_000);
    });
    p.case("drivers", |p| {
        p.eq("fold-add", fold_add(&|x| x * x, 1, 4, 0.0), 14.0);
        p.eq("fold-mul", fold_mul(&|x| x, 1, 4, 1.0), 6.0);
        p.demand("all-t", fold_all(&|x| x < 5.0, 1, 5, true), "forall <5");
        p.demand("all-f", !fold_all(&|x| x < 4.0, 1, 5, true), "forall <4 fails");
        p.demand("any", fold_any(&|x| x == 4.0, 1, 5, false), "exists 4");
        p.eq("empty-add", fold_add(&|x| x, 5, 5, 7.0), 7.0);
        p.eq("i64-fact", fold_mul_i64(&|i| i, 1, 21, 1), 2_432_902_008_176_640_000);
        p.eq("i64-sum", fold_add_i64(&|i| i, 1, 5, 0), 10);
        p.close("simpson-quad", simpson(&|x| x * x, 0.0, 1.0, 64), 1.0 / 3.0, 1e-12);
        p.close("simpson-sin", simpson(&|x| x.sin(), 0.0, std::f64::consts::PI, 64), 2.0, 1e-6);
        p.close("limit-sinc", sample_limit(&|x| x.sin() / x, 0.0, 1.0), 1.0, 1e-3);
        let l = sample_limit(&|x| x.abs(), 0.0, 1.0);
        p.demand("limit-dir", l >= 0.0 && (l - 0.0).abs() < 1e-3, "one-sided |x| limit");
    });
    p.case("source", |p| {
        for s in ["pub fn vec_add", "pub fn einsum_checked", "pub fn vec_index_checked", "pub fn tensor_slice_checked", "pub struct Tensor"] {
            p.contains(format!("src/{s}"), SOURCE, s);
        }
        p.demand("no-inner-attr", !SOURCE.contains("#!["), "no inner attributes");
        let imports = SOURCE.lines().filter(|l| { let t = l.trim_start(); t.starts_with("use ") || t.starts_with("extern ") }).count();
        p.eq("no-imports", imports, 0);
        let unsafe_hits = SOURCE.lines().filter(|l| { let t = l.trim_start(); !t.starts_with("//") && ["unsafe fn", "unsafe impl", "unsafe trait", "unsafe mod", "unsafe {"].iter().any(|q| t.contains(q)) }).count();
        p.eq("no-unsafe", unsafe_hits, 0);
    });
    p.case("nullvector", |p| {
        use emath_rt::primitive_int_nullvector as null;
        p.eq("combustion", null(&vec![vec![2, 0, 2], vec![0, 2, 1]]).expect("computes"), Some(vec![2, 1, -2]));
        p.eq("thermite", null(&vec![vec![2, 0, 0, 1], vec![3, 0, 3, 0], vec![0, 1, 2, 0]]).expect("computes"), Some(vec![1, 2, -1, -2]));
        p.eq("hydro", null(&vec![vec![2, 0, 2], vec![4, 2, 6]]).expect("computes"), Some(vec![1, 1, -1]));
        p.eq("reversed", null(&vec![vec![2, 0, 2], vec![1, 2, 0]]).expect("computes"), Some(vec![2, -1, -2]));
        p.eq("scaled", null(&vec![vec![2, 2, 0], vec![2, 1, 2]]).expect("computes"), Some(vec![2, -2, -1]));
        p.eq("none-zero", null(&vec![vec![2, 0], vec![0, 1]]).expect("computes"), None);
        p.eq("none-under", null(&vec![vec![2, 0, 2, 0], vec![0, 2, 1, 2]]).expect("computes"), None);
        p.eq("perm", null(&vec![vec![0, 2, 2], vec![1, 0, 2]]).expect("computes"), Some(vec![2, 1, -1]));
        p.demand("ragged", null(&[vec![1, 2], vec![3]]).is_err(), "ragged refuses");
        p.demand("empty", null(&[]).is_err(), "empty refuses");
    });
    p.finish();
}
