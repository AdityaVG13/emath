//! Domain op evaluation: exact linear algebra, optimization, polynomials, sequences, ODEs, control, category, probability, tensors, finite fields.

use super::*;

pub(super) fn eval_domain_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::GraphSparseTriplets(adj) => {
            // Sparse COO extraction: ascending (u, v)
            // triplets of the nonzero entries.
            let (rows, cols, data) = matrix_of(registers, adj, name)?;
            let triplets =
                emath_rt::graph::sparse_triplets(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(triplets))
        }
        EmirOp::IntNullspace(matrix) => {
            // Exact integer null vector: the generic primitive
            // over the dense carrier. Every entry must be an exact
            // small integer (E-NULLSPACE-001); the nullspace must be
            // exactly one-dimensional (E-NULLSPACE-002); the result is
            // the canonical primitive vector (f64-exact integers).
            let (rows, cols, data) = matrix_of(registers, matrix, name)?;
            let mut int_rows: Vec<Vec<i64>> = Vec::with_capacity(rows);
            for chunk in data.chunks(cols) {
                let mut row = Vec::with_capacity(cols);
                for &x in chunk {
                    // Exact small-integer check: integral value inside
                    // the i64 range is representable exactly in f64.
                    if x.fract() != 0.0 || x < -2f64.powi(63) || x >= 2f64.powi(63) {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "E-NULLSPACE-001: non-integral entry in integer \
                                     nullspace input",
                        });
                    }
                    row.push(x as i64);
                }
                int_rows.push(row);
            }
            let null_vector = emath_rt::primitive_int_nullvector(&int_rows).map_err(|_| {
                EvalFault::Arithmetic {
                    op: name,
                    detail: "E-NULLSPACE-001: exact-integer overflow in nullspace input",
                }
            })?;
            let Some(vector) = null_vector else {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "E-NULLSPACE-002: integer matrix has no exactly one-dimensional \
                             nullspace",
                });
            };
            Ok(Value::Vector(vector.iter().map(|&v| v as f64).collect()))
        }
        EmirOp::ExactProductDelta(p_value, q_value) => {
            // Exact integer product difference: the
            // generic exact-rational equality primitive. Products run
            // over u128 with overflow refusal; entries must be exact
            // small integers. The difference is returned as f64 (exact
            // while |delta| < 2^53, guaranteed by the u128 guard).
            let exact_index = |x: f64| -> Result<u64, EvalFault> {
                if x.fract() != 0.0 || x < 0.0 || x >= 9_007_199_254_740_992.0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "E-EXACT-001: entries must be exact small nonnegative integers",
                    });
                }
                Ok(x as u64)
            };
            let p = vector_of(registers, p_value, name)?;
            let q = vector_of(registers, q_value, name)?;
            if p.len() != q.len() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "E-EXACT-001: numerator and denominator vectors differ in length",
                });
            }
            let product = |values: &[f64]| -> Result<u128, EvalFault> {
                let mut acc = 1u128;
                for &x in values {
                    acc = acc.checked_mul(u128::from(exact_index(x)?)).ok_or(
                        EvalFault::Arithmetic {
                            op: name,
                            detail: "E-EXACT-002: exact product overflow (use reduced K_i)",
                        },
                    )?;
                }
                Ok(acc)
            };
            let pp = product(p)?;
            let qq = product(q)?;
            // Exact compare BEFORE any cast (false-zero fix, mail 93):
            // entries are < 2^53 but products are u128, so distinct
            // exact products above 2^53 can cast to the same f64 and
            // falsely certify consistency. Compare in u128, subtract
            // the magnitude exactly, then apply the sign for the
            // diagnostic scalar.
            if pp == qq {
                return Ok(Value::F64(0.0));
            }
            let (magnitude, negative) = if pp > qq {
                (pp - qq, false)
            } else {
                (qq - pp, true)
            };
            let delta = magnitude as f64;
            Ok(Value::F64(if negative { -delta } else { delta }))
        }
        EmirOp::GraphSparseFromTriplets(n_value, triplets_value) => {
            let n = f64_of(registers, n_value, name)?;
            let triplets = vector_of(registers, triplets_value, name)?.to_vec();
            let flat = emath_rt::graph::sparse_from_triplets(n, &triplets).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            let side = (flat.len() as f64).sqrt() as usize;
            Ok(Value::Matrix {
                rows: side,
                cols: side,
                data: flat,
            })
        }
        EmirOp::LpMinimize(a_value, b_value, c_value) => {
            // Standard-form LP via Bland's-rule simplex:
            // deterministic smallest-index pivoting; unbounded
            // objectives refuse typed (E-LP-001) — never a wrong
            // finite "optimum".
            let (m, n, a_flat) = matrix_of(registers, a_value, name)?;
            let b = vector_of(registers, b_value, name)?.to_vec();
            let c = vector_of(registers, c_value, name)?.to_vec();
            let x =
                emath_rt::optimization::lp_minimize(&a_flat, m, n, &b, &c).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(x))
        }
        EmirOp::ParetoFront(points_value) => {
            // Strict Pareto mask: rows are objective vectors
            // (all minimized); the mask is the portfolio artifact's
            // deterministic data.
            let (rows, cols, data) = matrix_of(registers, points_value, name)?;
            let mask =
                emath_rt::optimization::pareto_front(&data, rows, cols).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(mask))
        }
        EmirOp::PolyMul(a_value, b_value) => {
            // Cauchy convolution over ascending coefficients (the B28
            // compute layer); empty operand = the zero polynomial.
            let a = vector_of(registers, a_value, name)?.to_vec();
            let b = vector_of(registers, b_value, name)?.to_vec();
            let product = emath_rt::polynomial::poly_mul(&a, &b).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(product))
        }
        EmirOp::PolyEval(poly_value, point_value) => {
            // Horner evaluation (ascending coefficients); empty
            // coefficients evaluate to 0.0 (the zero polynomial).
            let coefficients = vector_of(registers, poly_value, name)?.to_vec();
            let point = f64_of(registers, point_value, name)?;
            let value = emath_rt::polynomial::poly_eval(&coefficients, point).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(value))
        }
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => {
            let initial = vector_of(registers, initial, name)?;
            let recurrence = vector_of(registers, recurrence, name)?;
            let budget = f64_of(registers, budget, name)?;
            let values =
                emath_rt::sequence::generate(initial, recurrence, budget).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::SequenceConvolve { left, right, count } => {
            let left = vector_of(registers, left, name)?;
            let right = vector_of(registers, right, name)?;
            let count = f64_of(registers, count, name)?;
            let values = emath_rt::sequence::convolve(left, right, count).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(values))
        }
        EmirOp::OdeBackwardEuler(rate_value, y0_value, h_value) => {
            // Backward Euler on the scalar carrier: Newton to
            // machine tolerance; typed refusals E-ODE-001/003/004.
            let rate = vector_of(registers, rate_value, name)?.to_vec();
            let y0 = f64_of(registers, y0_value, name)?;
            let h = f64_of(registers, h_value, name)?;
            let y1 = emath_rt::dynamics::ode_backward_euler(&rate, y0, h).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(y1))
        }
        EmirOp::OdeVelocityVerlet(a_value, q_value, v_value, h_value) => {
            // Velocity Verlet on the separable scalar carrier
            // kick-drift-kick; typed refusals E-ODE-003/004.
            let acceleration = vector_of(registers, a_value, name)?.to_vec();
            let q0 = f64_of(registers, q_value, name)?;
            let v0 = f64_of(registers, v_value, name)?;
            let h = f64_of(registers, h_value, name)?;
            let (q1, v1) = emath_rt::dynamics::ode_velocity_verlet(&acceleration, q0, v0, h)
                .map_err(|error| EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                })?;
            Ok(Value::Vector(vec![q1, v1]))
        }
        EmirOp::PoissonDirichletSine(load_value) => {
            // Spectral Poisson on the Dirichlet unit interval
            // discrete sine diagonalization typed refusals
            // E-PDE-001/002.
            let load = vector_of(registers, load_value, name)?.to_vec();
            let field = emath_rt::pde::poisson_dirichlet_sine(&load).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Vector(field))
        }
        EmirOp::ControlTransferEval(num_value, den_value, x_value) => {
            // Transfer-function evaluation (thin B43): Horner
            // both sides; typed refusals E-CONTROL-001/002.
            let num = vector_of(registers, num_value, name)?.to_vec();
            let den = vector_of(registers, den_value, name)?.to_vec();
            let x = f64_of(registers, x_value, name)?;
            let value = emath_rt::control::transfer_eval(&num, &den, x).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(value))
        }
        EmirOp::ControlDcGain(a_value, b_value, c_value) => {
            // State-space DC gain (thin B43): Faddeev–LeVerrier
            // characteristic polynomial + Routh–Hurwitz gate, then a
            // pivoted solve; typed refusals E-CONTROL-001..005.
            let (rows, cols, a_flat) = matrix_of(registers, a_value, name)?;
            let a = (0..rows)
                .map(|r| a_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let b = vector_of(registers, b_value, name)?.to_vec();
            let c = vector_of(registers, c_value, name)?.to_vec();
            let gain = emath_rt::control::state_space_dc_gain(&a, &b, &c).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::F64(gain))
        }
        EmirOp::ControlPolesStable(den_value) => {
            // Routh–Hurwitz strict stability (thin B43); typed
            // refusals E-CONTROL-001/002/005.
            let den = vector_of(registers, den_value, name)?.to_vec();
            let stable = emath_rt::control::poles_stable(&den).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Bool(stable))
        }
        EmirOp::CategoryCheck(dom_value, cod_value, comp_value) => {
            // Finite-category law gate (thin B39): certifies the
            // dense composition table; typed refusals E-CAT-001..007.
            let dom = vector_of(registers, dom_value, name)?.to_vec();
            let cod = vector_of(registers, cod_value, name)?.to_vec();
            let (rows, cols, comp_flat) = matrix_of(registers, comp_value, name)?;
            let comp = (0..rows)
                .map(|r| comp_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let valid = emath_rt::category::category_check(&dom, &cod, &comp).map_err(|error| {
                EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                }
            })?;
            Ok(Value::Bool(valid))
        }
        EmirOp::CategoryDiagramCommutative(dom_value, cod_value, comp_value, faces_value) => {
            // Diagram commutativity over face path-pairs (thin
            // B39); the carrier certifies first, typed refusals
            // E-CAT-001..007.
            let dom = vector_of(registers, dom_value, name)?.to_vec();
            let cod = vector_of(registers, cod_value, name)?.to_vec();
            let (rows, cols, comp_flat) = matrix_of(registers, comp_value, name)?;
            let comp = (0..rows)
                .map(|r| comp_flat[r * cols..(r + 1) * cols].to_vec())
                .collect::<Vec<_>>();
            let faces = vector_of(registers, faces_value, name)?.to_vec();
            let mask = emath_rt::category::diagram_commutative(&dom, &cod, &comp, &faces).map_err(
                |error| EvalFault::CapabilityRefused {
                    capability: name.to_string(),
                    code: error.code().to_string(),
                },
            )?;
            Ok(Value::Vector(
                mask.iter()
                    .map(|face| if *face { 1.0 } else { 0.0 })
                    .collect(),
            ))
        }
        EmirOp::ProbSample {
            kind,
            params: params_value,
            seed: seed_value,
            draws: draws_value,
            stream: stream_value,
        } => {
            // Seeded sampling from an admitted family:
            // SplitMix64 stream; typed refusals E-PROB-001/002/003.
            let params = vector_of(registers, params_value, name)?.to_vec();
            let seed = f64_of(registers, seed_value, name)?;
            let draws = f64_of(registers, draws_value, name)?;
            let family = match kind {
                crate::ProbKind::Normal => emath_rt::probability::Family::Normal,
                crate::ProbKind::Uniform => emath_rt::probability::Family::Uniform,
                crate::ProbKind::Bernoulli => emath_rt::probability::Family::Bernoulli,
            };
            let stream_path = match stream_value {
                Some(value) => match register(registers, value)? {
                    Value::Text(path) => path.as_str(),
                    _ => {
                        return Err(EvalFault::TypeConfusion {
                            register: value.0,
                            op: name,
                        });
                    }
                },
                None => "",
            };
            let stream = emath_rt::probability::prob_sample_in_stream(
                family,
                &params,
                seed,
                draws,
                stream_path,
            )
            .map_err(|error| EvalFault::CapabilityRefused {
                capability: name.to_string(),
                code: error.code().to_string(),
            })?;
            Ok(Value::Vector(stream))
        }
        EmirOp::ProbDensity {
            kind,
            params: params_value,
            x: x_value,
        } => {
            // Exact density / PMF: closed forms, not
            // estimates; same refusal surface as ProbSample.
            let params = vector_of(registers, params_value, name)?.to_vec();
            let x = f64_of(registers, x_value, name)?;
            let family = match kind {
                crate::ProbKind::Normal => emath_rt::probability::Family::Normal,
                crate::ProbKind::Uniform => emath_rt::probability::Family::Uniform,
                crate::ProbKind::Bernoulli => emath_rt::probability::Family::Bernoulli,
            };
            let density =
                emath_rt::probability::prob_density(family, &params, x).map_err(|error| {
                    EvalFault::CapabilityRefused {
                        capability: name.to_string(),
                        code: error.code().to_string(),
                    }
                })?;
            Ok(Value::F64(density))
        }
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => {
            let expected = shape_product(shape).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "tensor size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "tensor element count does not match shape product",
                });
            }
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Tensor {
                shape: shape.clone(),
                data,
            })
        }
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            let (shape, data) = tensor_of(registers, tensor, name)?;
            if indices.len() != shape.len() {
                return Err(EvalFault::TypeConfusion {
                    register: tensor.0,
                    op: name,
                });
            }
            let mut raw = Vec::with_capacity(indices.len());
            for &index in indices {
                raw.push(f64_of(registers, index, name)?);
            }
            emath_rt::tensor_index_checked(shape, data, &raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::TensorSlice { tensor, ref axes } => {
            eval_tensor_slice(registers, tensor, axes, name)
        }
        EmirOp::TensorAdd(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: emath_rt::tensor_add(d1, d2),
            })
        }
        EmirOp::TensorSub(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: emath_rt::tensor_sub(d1, d2),
            })
        }
        EmirOp::TensorScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Tensor { shape, data }, Value::F64(scale))
                | (Value::F64(scale), Value::Tensor { shape, data }) => Ok(Value::Tensor {
                    shape: shape.clone(),
                    data: emath_rt::tensor_scale(data, *scale),
                }),
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => eval_einsum(registers, subscripts, inputs, name),
        EmirOp::Factorial(n) => {
            let n = i64_of(registers, n, name)?;
            let result = emath_rt::factorial_checked(n)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::ModInv(a, m) => {
            // Stage-2 dual dispatch (emath-t63iz): any BigInt operand
            // promotes the whole op to the big lane; all-I64 keeps the
            // stage-1 kernel bit-for-bit.
            if is_big(registers, &[a, m]) {
                let modulus = big_modulus_of(registers, m, name)?;
                let base = big_field_of(registers, a, &modulus, name)?;
                let result = emath_rt::big_mod_inv_checked(&base, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigInt(result))
            } else {
                let a = i64_of(registers, a, name)?;
                let m = i64_of(registers, m, name)?;
                let result = emath_rt::mod_inv_checked(a, m)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::I64(result))
            }
        }
        EmirOp::SqrtMod(a, p) => {
            if is_big(registers, &[a, p]) {
                let modulus = big_modulus_of(registers, p, name)?;
                let base = big_field_of(registers, a, &modulus, name)?;
                let result = emath_rt::big_sqrt_mod_checked(&base, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigInt(result))
            } else {
                let a = i64_of(registers, a, name)?;
                let p = i64_of(registers, p, name)?;
                let result = emath_rt::sqrt_mod_checked(a, p)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::I64(result))
            }
        }
        EmirOp::PowMod(b, e, m) => {
            if is_big(registers, &[b, e, m]) {
                let modulus = big_modulus_of(registers, m, name)?;
                let base = big_field_of(registers, b, &modulus, name)?;
                let exponent = big_exponent_of(registers, e, name)?;
                let result = emath_rt::big_pow_mod_checked(&base, &exponent, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigInt(result))
            } else {
                let b = i64_of(registers, b, name)?;
                let e = i64_of(registers, e, name)?;
                let m = i64_of(registers, m, name)?;
                let result = emath_rt::pow_mod_checked(b, e, m)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::I64(result))
            }
        }
        EmirOp::IntRem(a, m) => {
            if is_big(registers, &[a, m]) {
                let modulus = big_modulus_of(registers, m, name)?;
                let dividend = big_field_of(registers, a, &modulus, name)?;
                let result = emath_rt::big_int_rem_checked(&dividend, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigInt(result))
            } else {
                let a = i64_of(registers, a, name)?;
                let m = i64_of(registers, m, name)?;
                if m <= 0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "int-rem: modulus must be positive",
                    });
                }
                // Exact Euclidean remainder; result is always non-negative
                // and in [0, m). rem_euclid(i64::MIN, -1) cannot overflow
                // because a positive modulus is enforced above.
                Ok(Value::I64(a.rem_euclid(m)))
            }
        }
        EmirOp::Congruence(a, b, m) => {
            if is_big(registers, &[a, b, m]) {
                let modulus = big_modulus_of(registers, m, name)?;
                let left = big_field_of(registers, a, &modulus, name)?;
                let right = big_field_of(registers, b, &modulus, name)?;
                // a ≡ b (mod m) ⇔ exact-Euclidean remainders agree.
                let left = emath_rt::big_int_rem_checked(&left, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                let right = emath_rt::big_int_rem_checked(&right, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::Bool(left == right))
            } else {
                let a = i64_of(registers, a, name)?;
                let b = i64_of(registers, b, name)?;
                let m = i64_of(registers, m, name)?;
                if m == 0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "cong: modulus must be non-zero",
                    });
                }
                Ok(Value::Bool((a - b).rem_euclid(m) == 0))
            }
        }
        EmirOp::PolyEvalMod(coeffs, x, p) => {
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            if is_big(registers, &[x, p]) {
                let modulus = big_modulus_of(registers, p, name)?;
                let point = big_field_of(registers, x, &modulus, name)?;
                let result = emath_rt::big_poly_eval_mod_checked(&coeff_vec, &point, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigInt(result))
            } else {
                let x = i64_of(registers, x, name)?;
                let p = i64_of(registers, p, name)?;
                let result = emath_rt::poly_eval_mod_checked(coeff_vec, x, p)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::I64(result))
            }
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            let n = i64_of(registers, n, name)?;
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data.clone(),
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            if is_big(registers, &[p]) {
                let modulus = big_modulus_of(registers, p, name)?;
                let codeword = emath_rt::big_rs_encode_checked(&coeff_vec, n, &modulus)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::BigVector(codeword))
            } else {
                let p = i64_of(registers, p, name)?;
                let codeword = emath_rt::rs_encode_checked(&coeff_vec, n, p)
                    .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
                Ok(Value::Vector(codeword))
            }
        }
        EmirOp::HammingDistance(a, b) => {
            let a_vec = match register(registers, a)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: a.0,
                        op: name,
                    });
                }
            };
            let b_vec = match register(registers, b)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: b.0,
                        op: name,
                    });
                }
            };
            let dist = emath_rt::hamming_distance_checked(a_vec, b_vec)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(dist))
        }
        _ => unreachable!("eval_domain_op routed a non-matching op"),
    }
}

// ── Stage-2 big-lane helpers (emath-t63iz) ───────────────────────────────

/// True when ANY register in `values` holds a stage-2 `BigInt`. This is
/// the dispatch predicate: one big operand promotes the whole op to the
/// big lane (result follows operands).
fn is_big(registers: &[Value], values: &[EmirValue]) -> bool {
    values
        .iter()
        .any(|value| matches!(register(registers, *value), Ok(Value::BigInt(_))))
}

/// The modulus as a big lane value: `BigInt` passes through; an `I64`
/// must be positive (the stage-1 refusal, unchanged wording).
fn big_modulus_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<emath_rt::UBig, EvalFault> {
    match register(registers, value)? {
        Value::BigInt(big) => {
            if big.is_zero() {
                Err(EvalFault::Arithmetic {
                    op,
                    detail: "modulus must be positive",
                })
            } else {
                Ok(big.clone())
            }
        }
        Value::I64(m) if *m > 0 => Ok(emath_rt::UBig::from_u64(*m as u64)),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "modulus must be positive",
        }),
    }
}

/// A field element into the big lane: `BigInt` passes through (the
/// kernels reduce); `I64` embeds via the exact-Euclidean kernel — the
/// same `rem_euclid` semantics as the stage-1 lane, swapped
/// representation.
fn big_field_of(
    registers: &[Value],
    value: EmirValue,
    modulus: &emath_rt::UBig,
    op: &'static str,
) -> Result<emath_rt::UBig, EvalFault> {
    match register(registers, value)? {
        Value::BigInt(big) => Ok(big.clone()),
        Value::I64(v) => emath_rt::big_int_rem_i64_checked(*v, modulus)
            .map_err(|detail| EvalFault::Arithmetic { op, detail }),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// A `pow_mod` exponent into the big lane: `BigInt` passes through
/// (unsigned by construction); `I64` must be non-negative — the stage-1
/// refusal, unchanged wording.
fn big_exponent_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<emath_rt::UBig, EvalFault> {
    match register(registers, value)? {
        Value::BigInt(big) => Ok(big.clone()),
        Value::I64(e) if *e >= 0 => Ok(emath_rt::UBig::from_u64(*e as u64)),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "pow-mod: exponent must be non-negative",
        }),
    }
}
