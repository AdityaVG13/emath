//! Domain op lowering: optimization, polynomials, sequences, ODEs, control, category, probability, tensors, finite fields.

use super::*;

pub(super) fn op_domain_exprs(
    op: &EmirOp,
    program: &EmirProgram,
    kinds: &[ScalarKind],
) -> Result<Expr, BackendError> {
    match op {
        EmirOp::LpMinimize(a, b, c) => Ok(rt_call(
            "lp_minimize",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *b),
                operand_ref(program, *c),
            ],
        )),
        EmirOp::ParetoFront(points) => {
            Ok(rt_call("pareto_front", vec![operand_ref(program, *points)]))
        }
        // Polynomial kernels.
        EmirOp::PolyMul(a, b) => Ok(rt_call(
            "poly_mul",
            vec![operand_ref(program, *a), operand_ref(program, *b)],
        )),
        EmirOp::PolyEval(p, x) => Ok(rt_call(
            "poly_eval",
            vec![operand_ref(program, *p), operand_ref(program, *x)],
        )),
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => Ok(rt_call(
            "sequence_generate",
            vec![
                operand_ref(program, *initial),
                operand_ref(program, *recurrence),
                // Scalar budget: the runtime kernel takes an owned
                // `f64` (not a slice ref), cast from the register's
                // scalar kind when it is an integer count.
                typed_operand(program, *budget, ScalarKind::F64, &kinds),
            ],
        )),
        EmirOp::SequenceConvolve { left, right, count } => Ok(rt_call(
            "sequence_convolve",
            vec![
                operand_ref(program, *left),
                operand_ref(program, *right),
                typed_operand(program, *count, ScalarKind::F64, &kinds),
            ],
        )),
        // ODE stepping kernels (thin nucleus): typed wrappers
        // in `emath_rt::dynamics` (Newton backward Euler, velocity
        // Verlet), same refusal surface as the LP/Pareto renders.
        EmirOp::OdeBackwardEuler(rate, y0, h) => Ok(rt_call(
            "dynamics::ode_backward_euler",
            vec![
                operand_ref(program, *rate),
                operand_ref(program, *y0),
                operand_ref(program, *h),
            ],
        )),
        EmirOp::OdeVelocityVerlet(a, q, v, h) => Ok(rt_call(
            "dynamics::ode_velocity_verlet",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *q),
                operand_ref(program, *v),
                operand_ref(program, *h),
            ],
        )),
        // Spectral Poisson (thin nucleus): typed wrapper in
        // `emath_rt::pde` (Dirichlet sine diagonalization).
        EmirOp::PoissonDirichletSine(load) => Ok(rt_call(
            "pde::poisson_dirichlet_sine",
            vec![operand_ref(program, *load)],
        )),
        // Control surface (thin B43): raw kernels in `emath_rt`
        // (Routh–Hurwitz stability, Faddeev–LeVerrier characteristic
        // polynomial, pivoted solve); refusals surface through the
        // reference interpreter's typed E-CONTROL codes.
        EmirOp::ControlTransferEval(num, den, x) => Ok(rt_call(
            "control_transfer_eval",
            vec![
                operand_ref(program, *num),
                operand_ref(program, *den),
                operand_ref(program, *x),
            ],
        )),
        EmirOp::ControlDcGain(a, b, c) => Ok(rt_call(
            "control_state_space_dc_gain",
            vec![
                operand_ref(program, *a),
                operand_ref(program, *b),
                operand_ref(program, *c),
            ],
        )),
        EmirOp::ControlPolesStable(den) => Ok(rt_call(
            "control_poles_stable",
            vec![operand_ref(program, *den)],
        )),
        // Finite-category kernels (thin B39): raw kernels in
        // `emath_rt` (law gate over the dense composition table, face
        // path-pair commutativity); refusals surface through the
        // reference interpreter's typed E-CAT codes.
        EmirOp::CategoryCheck(dom, cod, comp) => Ok(rt_call(
            "category_check",
            vec![
                operand_ref(program, *dom),
                operand_ref(program, *cod),
                operand_ref(program, *comp),
            ],
        )),
        EmirOp::CategoryDiagramCommutative(dom, cod, comp, faces) => Ok(rt_call(
            "category_diagram_commutative",
            vec![
                operand_ref(program, *dom),
                operand_ref(program, *cod),
                operand_ref(program, *comp),
                operand_ref(program, *faces),
            ],
        )),
        // Probability nucleus: typed wrappers in
        // `emath_rt::probability` (SplitMix64 stream + exact
        // densities); the family code is the stable kernel encoding.
        EmirOp::ProbSample {
            kind,
            params,
            seed,
            draws,
            stream,
        } => Ok(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_sample_in_stream".to_string(),
            ],
            args: vec![
                Expr::Raw(format!(
                    "emath_rt::probability::Family::{}",
                    match kind {
                        ProbKind::Normal => "Normal",
                        ProbKind::Uniform => "Uniform",
                        ProbKind::Bernoulli => "Bernoulli",
                    }
                )),
                operand_ref(program, *params),
                operand_ref(program, *seed),
                operand_ref(program, *draws),
                stream
                    .map(|value| {
                        Expr::Raw(format!("&{}", render_expr(&operand_ref(program, value))))
                    })
                    .unwrap_or_else(|| Expr::Str(String::new())),
            ],
        }),
        EmirOp::ProbDensity { kind, params, x } => Ok(Expr::Call {
            path: vec![
                "emath_rt".to_string(),
                "probability".to_string(),
                "prob_density".to_string(),
            ],
            args: vec![
                Expr::Raw(format!("{} /* {} */", kind.code(), kind.name())),
                operand_ref(program, *params),
                operand_ref(program, *x),
            ],
        }),
        EmirOp::TensorCreate { shape, elements } => {
            let shape_lits = shape
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let data = elements
                .iter()
                .map(|elem| render_expr(&typed_operand(program, *elem, ScalarKind::F64, &kinds)))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Expr::Raw(format!(
                "emath_rt::Tensor {{ shape: vec![{shape_lits}], data: vec![{data}] }}"
            )))
        }
        EmirOp::TensorIndex { tensor, indices } => {
            Ok(tensor_index_call(program, *tensor, indices, &kinds))
        }
        EmirOp::TensorSlice { tensor, axes } => {
            Ok(tensor_slice_call(program, *tensor, axes, &kinds))
        }
        EmirOp::TensorAdd(l, r) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {left}.shape.clone(), data: emath_rt::tensor_add(&{left}.data, &{right}.data) }}",
            left = render_expr(&operand(program, *l)),
            right = render_expr(&operand(program, *r)),
        ))),
        EmirOp::TensorSub(l, r) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {left}.shape.clone(), data: emath_rt::tensor_sub(&{left}.data, &{right}.data) }}",
            left = render_expr(&operand(program, *l)),
            right = render_expr(&operand(program, *r)),
        ))),
        EmirOp::TensorScale(tensor, scale) => Ok(Expr::Raw(format!(
            "emath_rt::Tensor {{ shape: {tensor}.shape.clone(), data: emath_rt::tensor_scale(&{tensor}.data, {scale}) }}",
            tensor = render_expr(&operand(program, *tensor)),
            scale = render_expr(&typed_operand(program, *scale, ScalarKind::F64, &kinds)),
        ))),
        EmirOp::Einsum { subscripts, inputs } => {
            let operands = inputs
                .iter()
                .map(|v| {
                    format!(
                        "emath_rt::EinsumIn::einsum_operand(&{})",
                        render_expr(&operand(program, *v))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut escaped = String::with_capacity(subscripts.len());
            for ch in subscripts.chars() {
                match ch {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    ch => escaped.push(ch),
                }
            }
            let reshape = match emath_rt::einsum_output_rank(subscripts) {
                0 => "einsum_as_scalar",
                1 => "einsum_as_vector",
                2 => "einsum_as_matrix",
                _ => "einsum_as_tensor",
            };
            Ok(Expr::Raw(format!(
                "emath_rt::{reshape}(\"{escaped}\", &[{operands}])"
            )))
        }
        EmirOp::Factorial(n) => Ok(rt_call(
            "factorial",
            vec![typed_operand(program, *n, ScalarKind::I64, &kinds)],
        )),
        EmirOp::ModInv(a, m) => {
            let (ka, km) = (kind_at(&kinds, *a), kind_at(&kinds, *m));
            if matches!(ka, ScalarKind::BigInt) || matches!(km, ScalarKind::BigInt) {
                // Stage-2 (emath-t63iz): big lane; same kernels as the
                // interpreter (structural parity via the SOURCE embed).
                let m_expr = render_expr(&operand(program, *m));
                let a_expr = big_field_operand(program, *a, ka, &m_expr);
                let m_big = big_modulus_operand(program, *m, km);
                Ok(rt_call("big_mod_inv", vec![a_expr, m_big]))
            } else {
                Ok(rt_call(
                    "mod_inv",
                    vec![
                        typed_operand(program, *a, ScalarKind::I64, &kinds),
                        typed_operand(program, *m, ScalarKind::I64, &kinds),
                    ],
                ))
            }
        }
        // Tonelli-Shanks root through the shared emath_rt kernel —
        // generated Rust matches the interpreter bit-for-bit (same
        // posture as mod_inv above).
        EmirOp::SqrtMod(a, p) => {
            let (ka, kp) = (kind_at(&kinds, *a), kind_at(&kinds, *p));
            if matches!(ka, ScalarKind::BigInt) || matches!(kp, ScalarKind::BigInt) {
                let p_expr = render_expr(&operand(program, *p));
                let a_expr = big_field_operand(program, *a, ka, &p_expr);
                let p_big = big_modulus_operand(program, *p, kp);
                Ok(rt_call("big_sqrt_mod", vec![a_expr, p_big]))
            } else {
                Ok(rt_call(
                    "sqrt_mod",
                    vec![
                        typed_operand(program, *a, ScalarKind::I64, &kinds),
                        typed_operand(program, *p, ScalarKind::I64, &kinds),
                    ],
                ))
            }
        }
        // Square-and-multiply over i128 intermediates — the shared
        // emath_rt kernel keeps generated Rust bit-identical to the
        // interpreter (same posture as mod_inv above).
        EmirOp::PowMod(b, e, m) => {
            let (kb, ke, km) = (
                kind_at(&kinds, *b),
                kind_at(&kinds, *e),
                kind_at(&kinds, *m),
            );
            if matches!(kb, ScalarKind::BigInt)
                || matches!(ke, ScalarKind::BigInt)
                || matches!(km, ScalarKind::BigInt)
            {
                let m_expr = render_expr(&operand(program, *m));
                let b_expr = big_field_operand(program, *b, kb, &m_expr);
                let e_expr = big_exponent_operand(program, *e, ke);
                let m_big = big_modulus_operand(program, *m, km);
                Ok(rt_call("big_pow_mod", vec![b_expr, e_expr, m_big]))
            } else {
                Ok(rt_call(
                    "pow_mod",
                    vec![
                        typed_operand(program, *b, ScalarKind::I64, &kinds),
                        typed_operand(program, *e, ScalarKind::I64, &kinds),
                        typed_operand(program, *m, ScalarKind::I64, &kinds),
                    ],
                ))
            }
        }
        // Universal exact-Euclidean remainder. Mirrors
        // ModInv's parity posture: the interpreter enforces the positive
        // modulus as a typed EvalFault; the generated Rust emits exact
        // rem_euclid on admitted (positive-modulus) programs. rem_euclid
        // is total for a positive i64 modulus — no panic path, exact i64.
        EmirOp::IntRem(a, m) => {
            let (ka, km) = (kind_at(&kinds, *a), kind_at(&kinds, *m));
            if matches!(ka, ScalarKind::BigInt) || matches!(km, ScalarKind::BigInt) {
                let m_expr = render_expr(&operand(program, *m));
                let a_expr = big_field_operand(program, *a, ka, &m_expr);
                let m_big = big_modulus_operand(program, *m, km);
                Ok(rt_call("big_int_rem", vec![a_expr, m_big]))
            } else {
                Ok(Expr::Raw(format!(
                    "((__e{} as i64).rem_euclid(__e{} as i64))",
                    a.0, m.0
                )))
            }
        }
        EmirOp::Congruence(a, b, m) => Ok(Expr::Raw(format!(
            "(((__e{} as i64) - (__e{} as i64)).rem_euclid(__e{} as i64) == 0)",
            a.0, b.0, m.0
        ))),
        EmirOp::PolyEvalMod(coeffs, x, p) => {
            let (kx, kp) = (kind_at(&kinds, *x), kind_at(&kinds, *p));
            if matches!(kx, ScalarKind::BigInt) || matches!(kp, ScalarKind::BigInt) {
                let p_expr = render_expr(&operand(program, *p));
                let x_expr = big_field_operand(program, *x, kx, &p_expr);
                let p_big = big_modulus_operand(program, *p, kp);
                Ok(rt_call(
                    "big_poly_eval_mod",
                    vec![Expr::Raw(format!("&__e{}", coeffs.0)), x_expr, p_big],
                ))
            } else {
                Ok(Expr::Raw(format!(
                    "emath_rt::poly_eval_mod(&__e{}, {} as i64, {} as i64)",
                    coeffs.0,
                    render_expr(&operand(program, *x)),
                    render_expr(&operand(program, *p)),
                )))
            }
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            let kp = kind_at(&kinds, *p);
            if matches!(kp, ScalarKind::BigInt) {
                let p_big = big_modulus_operand(program, *p, kp);
                Ok(rt_call(
                    "big_rs_encode",
                    vec![
                        Expr::Raw(format!("&__e{}", coeffs.0)),
                        Expr::Raw(format!("(__e{}) as i64", n.0)),
                        p_big,
                    ],
                ))
            } else {
                Ok(Expr::Raw(format!(
                    "emath_rt::rs_encode(&__e{}, __e{} as i64, __e{} as i64)",
                    coeffs.0, n.0, p.0
                )))
            }
        }
        EmirOp::HammingDistance(a, b) => Ok(Expr::Raw(format!(
            "emath_rt::hamming_distance(&__e{}, &__e{})",
            a.0, b.0
        ))),
        _ => unreachable!("op_domain_exprs routed a non-matching op"),
    }
}

// ── Stage-2 big-lane rendering (emath-t63iz) ─────────────────────────────

/// An operand into the big lane for generated Rust: a `BigInt` register
/// passes through (already `UBig`); an `I64` field element embeds via
/// the exact-Euclidean kernel against the rendered modulus expression.
fn big_field_operand(
    program: &EmirProgram,
    value: EmirValue,
    kind: ScalarKind,
    modulus_expr: &str,
) -> Expr {
    let raw = operand(program, value);
    match kind {
        ScalarKind::BigInt => Expr::Raw(format!("&{}", render_expr(&raw))),
        _ => Expr::Raw(format!(
            "&emath_rt::UBig::from_i64_rem(({}) as i64, &{modulus_expr})",
            render_expr(&raw)
        )),
    }
}

/// A non-negative exponent into the big lane (the kernel refuses
/// negatives; generated Rust runs admitted programs only).
fn big_exponent_operand(program: &EmirProgram, value: EmirValue, kind: ScalarKind) -> Expr {
    let raw = operand(program, value);
    match kind {
        ScalarKind::BigInt => Expr::Raw(format!("&{}", render_expr(&raw))),
        _ => Expr::Raw(format!(
            "&emath_rt::UBig::from_u64(({}) as u64)",
            render_expr(&raw)
        )),
    }
}

/// The modulus operand into the big lane (positive by admission).
fn big_modulus_operand(program: &EmirProgram, value: EmirValue, kind: ScalarKind) -> Expr {
    let raw = operand(program, value);
    match kind {
        ScalarKind::BigInt => Expr::Raw(format!("&{}", render_expr(&raw))),
        _ => Expr::Raw(format!(
            "&emath_rt::UBig::from_u64(({}) as u64)",
            render_expr(&raw)
        )),
    }
}
