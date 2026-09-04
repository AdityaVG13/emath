//! vector, number-theory, interval, and carrier builtins (emit_call arms moved verbatim from call.rs).

use super::*;

impl super::super::Emitter {
    pub(super) fn emit_call_math_misc(
        &mut self,
        function: &str,
        package: &SemanticPackage,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
        match function {
            "transpose" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::MatrixTranspose(v), span)
            }
            "length" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::VectorLength(v), span)
            }
            "dot" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::VectorDot(l, r), span)
            }
            "pow" | "core::math::pow" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.obligations.push(DomainObligation::PowFiniteResult);
                self.push(EmirOp::F64Pow(l, r), span)
            }
            "is_finite" | "core::math::is_finite" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::IsFinite(v), span)
            }
            "lerp" | "core::math::lerp" => {
                // lerp(a, b, t) = a + (b - a) * t
                let va = self.emit(package, args[0])?;
                let vb = self.emit(package, args[1])?;
                let vt = self.emit(package, args[2])?;
                let diff = self.push(EmirOp::F64Sub(vb, va), span)?;
                let interp = self.push(EmirOp::F64Mul(diff, vt), span)?;
                self.push(EmirOp::F64Add(va, interp), span)
            }
            "clamp" | "core::math::clamp" => {
                // clamp(x, lo, hi) = min(max(x, lo), hi)
                let vx = self.emit(package, args[0])?;
                let vlo = self.emit(package, args[1])?;
                let vhi = self.emit(package, args[2])?;
                let lo_clamped = self.push(EmirOp::BinaryBuiltin(BuiltinId::Max, vx, vlo), span)?;
                self.push(EmirOp::BinaryBuiltin(BuiltinId::Min, lo_clamped, vhi), span)
            }
            "einsum" => {
                if args.len() < 2 {
                    return Err("`einsum` expects at least 2 arguments".to_string());
                }
                // Extract the subscripts string from the first argument.
                let first_expr = package
                    .expr(args[0])
                    .ok_or("einsum: first argument expression not found")?;
                let subscripts = match first_expr {
                    ExprNode::Literal(Literal::Text(s)) => s.clone(),
                    _ => return Err("`einsum` first argument must be a string literal".to_string()),
                };
                // Emit the tensor arguments.
                let mut inputs = Vec::with_capacity(args.len() - 1);
                for arg in &args[1..] {
                    inputs.push(self.emit(package, *arg)?);
                }
                self.push(EmirOp::Einsum { subscripts, inputs }, span)
            }
            "grad" => {
                // Reverse-mode AD: compile body as sub-program, compute
                // gradients w.r.t. all inputs in one backward pass.
                if args.len() != 1 {
                    return Err("`grad` expects 1 argument".to_string());
                }
                let sc = self.states.len();
                let mut body_emitter = self.sub_emitter();
                let body_result = body_emitter.emit(package, args[0])?;
                let body_program = body_emitter.finish(body_result, sc)?;
                let var_indices: Vec<u16> = (0..u16::try_from(self.inputs.len())
                    .map_err(|_| "too many inputs for grad")?)
                    .collect();
                self.push(
                    EmirOp::ReverseMode {
                        body: body_program,
                        var_indices,
                    },
                    span,
                )
            }
            "factorial" | "core::math::factorial" => {
                let n = self.emit(package, args[0])?;
                self.push(EmirOp::Factorial(n), span)
            }
            "mod_inv" | "core::math::mod_inv" => {
                let a = self.emit(package, args[0])?;
                let m = self.emit(package, args[1])?;
                self.push(EmirOp::ModInv(a, m), span)
            }
            "field_inv" | "core::math::field_inv" => {
                // Prime-field inverse: field_inv(a, p) is
                // the exact modular inverse over the prime field. Arity
                // is refused TYPED (never a panic on a short arg list).
                if args.len() != 2 {
                    return Err(format!(
                        "`field_inv` expects 2 arguments (a, p), got {}",
                        args.len()
                    ));
                }
                let a = self.emit(package, args[0])?;
                let p = self.emit(package, args[1])?;
                self.push(EmirOp::ModInv(a, p), span)
            }
            "pow_mod" | "core::math::pow_mod" => {
                // Modular exponentiation: square-and-multiply over i128
                // intermediates (i64 operands never overflow). Arity is
                // refused TYPED (never a panic on a short arg list).
                if args.len() != 3 {
                    return Err(format!(
                        "`pow_mod` expects 3 arguments (base, exponent, modulus), got {}",
                        args.len()
                    ));
                }
                let b = self.emit(package, args[0])?;
                let e = self.emit(package, args[1])?;
                let m = self.emit(package, args[2])?;
                self.push(EmirOp::PowMod(b, e, m), span)
            }
            "sqrt_mod" | "core::math::sqrt_mod" => {
                // Tonelli-Shanks modular square root; non-residues refuse
                // TYPED (never a fabricated root).
                if args.len() != 2 {
                    return Err(format!(
                        "`sqrt_mod` expects 2 arguments (a, p), got {}",
                        args.len()
                    ));
                }
                let a = self.emit(package, args[0])?;
                let p = self.emit(package, args[1])?;
                self.push(EmirOp::SqrtMod(a, p), span)
            }
            "int_rem" | "core::math::int_rem" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`int_rem` expects 2 arguments (a, m), got {}",
                        args.len()
                    ));
                }
                let a = self.emit(package, args[0])?;
                let m = self.emit(package, args[1])?;
                self.push(EmirOp::IntRem(a, m), span)
            }
            "congruence" => {
                let a = self.emit(package, args[0])?;
                let b = self.emit(package, args[1])?;
                let m = self.emit(package, args[2])?;
                self.push(EmirOp::Congruence(a, b, m), span)
            }
            "poly_eval_mod" | "core::math::poly_eval_mod" => {
                let c = self.emit(package, args[0])?;
                let x = self.emit(package, args[1])?;
                let p = self.emit(package, args[2])?;
                self.push(EmirOp::PolyEvalMod(c, x, p), span)
            }
            "rs_encode" | "core::math::rs_encode" => {
                let c = self.emit(package, args[0])?;
                let n = self.emit(package, args[1])?;
                let p = self.emit(package, args[2])?;
                self.push(EmirOp::RSEncode(c, n, p), span)
            }
            "hamming_distance" => {
                let a = self.emit(package, args[0])?;
                let b = self.emit(package, args[1])?;
                self.push(EmirOp::HammingDistance(a, b), span)
            }
            "interval" => {
                // Certified interval constructor `interval(lo, hi)`.
                // Ill-formed bounds fault at run (IntervalCreate), never
                // silently swap or clamp.
                if args.len() != 2 {
                    return Err(format!(
                        "`interval` expects 2 operands (lo, hi), got {}",
                        args.len()
                    ));
                }
                let lo = self.emit(package, args[0])?;
                let hi = self.emit(package, args[1])?;
                self.push(EmirOp::IntervalCreate(lo, hi), span)
            }
            "intersect" => {
                // Interval intersection; an empty result is a typed
                // run refusal (IntervalIntersect).
                if args.len() != 2 {
                    return Err(format!(
                        "`intersect` expects 2 operands, got {}",
                        args.len()
                    ));
                }
                let a = self.emit(package, args[0])?;
                let b = self.emit(package, args[1])?;
                self.push(EmirOp::IntervalIntersect(a, b), span)
            }
            // Option/Result call surface: the nine names lower to
            // the SAME total value-semantics ops the term-compiler binds
            // and the reference VM executes (OptionSome/OptionNone/
            // OptionIsSome/OptionUnwrapOr, ResultOk/ResultErr/
            // ResultIsOk/ResultUnwrapOr/ResultErrorOf). There is NO
            // panicking unwrap: unwrap_or is the honesty gate and its
            // default evaluates eagerly. Arity is enforced HERE (the
            // strict-f64 gate lets no malformed call through); backend
            // codegen owns the typed carrier refusal.
            "option_some" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`option_some` expects 1 payload operand, got {}",
                        args.len()
                    ));
                }
                let payload = self.emit(package, args[0])?;
                self.push(EmirOp::OptionSome(payload), span)
            }
            "option_none" => {
                if !args.is_empty() {
                    return Err(format!(
                        "`option_none` expects 0 operands, got {}",
                        args.len()
                    ));
                }
                self.push(EmirOp::OptionNone, span)
            }
            "option_is_some" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`option_is_some` expects 1 operand, got {}",
                        args.len()
                    ));
                }
                let carrier = self.emit(package, args[0])?;
                self.push(EmirOp::OptionIsSome(carrier), span)
            }
            "option_unwrap_or" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`option_unwrap_or` expects (carrier, default), got {}",
                        args.len()
                    ));
                }
                let carrier = self.emit(package, args[0])?;
                let default = self.emit(package, args[1])?;
                self.push(EmirOp::OptionUnwrapOr(carrier, default), span)
            }
            "result_ok" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`result_ok` expects 1 payload operand, got {}",
                        args.len()
                    ));
                }
                let payload = self.emit(package, args[0])?;
                self.push(EmirOp::ResultOk(payload), span)
            }
            "result_err" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`result_err` expects 1 payload operand, got {}",
                        args.len()
                    ));
                }
                let payload = self.emit(package, args[0])?;
                self.push(EmirOp::ResultErr(payload), span)
            }
            "result_is_ok" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`result_is_ok` expects 1 operand, got {}",
                        args.len()
                    ));
                }
                let carrier = self.emit(package, args[0])?;
                self.push(EmirOp::ResultIsOk(carrier), span)
            }
            "result_unwrap_or" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`result_unwrap_or` expects (carrier, default), got {}",
                        args.len()
                    ));
                }
                let carrier = self.emit(package, args[0])?;
                let default = self.emit(package, args[1])?;
                self.push(EmirOp::ResultUnwrapOr(carrier, default), span)
            }
            "result_error_of" => {
                if args.len() != 1 {
                    return Err(format!(
                        "`result_error_of` expects 1 operand, got {}",
                        args.len()
                    ));
                }
                let carrier = self.emit(package, args[0])?;
                self.push(EmirOp::ResultErrorOf(carrier), span)
            }
            _ => unreachable!("emit_call_math_misc routed a non-matching builtin"),
        }
    }

    /// Registry-builtins lowered from CALL syntax. One mechanism for
    /// every `BuiltinId`: no new opcodes, no per-domain branches. The
    /// contracts are the registry's own (builtin.rs is the single
    /// source of truth):
    ///
    /// - `abs(x)` = |x| (piecewise-expressible; dual derivative sgn with
    ///   this crate's sgn(0)=0), `min`/`max` IEEE minimum/maximum
    ///   (NaN-ignoring on one NaN operand).
    /// - `sqrt` keeps the typed `SqrtNonNegative` obligation; a negative
    ///   domain argument evaluates to IEEE NaN — a labeled value, never
    ///   a crash, never a fabricated finite number (program.rs).
    /// - `atan2` is IEEE atan2: deterministic libm, ±0/π at the origin
    ///   per IEEE sign rules, NaN only from NaN operands.
    pub(super) fn emit_call_builtin(
        &mut self,
        id: BuiltinId,
        package: &SemanticPackage,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
        let expected = id.arity();
        if args.len() != expected {
            return Err(format!(
                "`{}` expects {expected} operand(s), got {}",
                id.name(),
                args.len()
            ));
        }
        if id.arity() == 1 {
            let operand = self.emit(package, args[0])?;
            for obligation in id.domain_obligations() {
                self.obligations.push(*obligation);
            }
            self.push(EmirOp::UnaryBuiltin(id, operand), span)
        } else {
            let left = self.emit(package, args[0])?;
            let right = self.emit(package, args[1])?;
            self.push(EmirOp::BinaryBuiltin(id, left, right), span)
        }
    }
}
