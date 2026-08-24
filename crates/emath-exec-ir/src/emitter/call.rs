//! Built-in function dispatch: lowers `emit_call` built-in calls to EMIR ops.

use emath_core::Span;
use emath_ir::{ExprNode, Literal, SemanticPackage};

use crate::{DomainObligation, EdgePolicy, EmirExprRef, EmirOp, EmirValue};

impl super::Emitter {
    pub(crate) fn emit_call(
        &mut self,
        package: &SemanticPackage,
        function: &str,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
        // Normalize namespace prefixes: math::sin → sin, linalg::dot → dot, etc.
        // This lets users write either `sin(x)` or `math::sin(x)`.
        let function_owned;
        let function = if let Some(bare) = function
            .strip_prefix("math::")
            .or_else(|| function.strip_prefix("linalg::"))
            .or_else(|| function.strip_prefix("pde::"))
            .or_else(|| function.strip_prefix("coding::"))
            .or_else(|| function.strip_prefix("core::math::"))
        {
            function_owned = bare.to_string();
            &function_owned
        } else {
            function
        };
        // Arity is enforced in every build, debug or release (bug-hunt
        // residual: debug_assert let empty/1-arg unary calls through to an
        // indexing panic and silently dropped extras in release).
        let unary = matches!(
            function,
            "exp"
                | "ln"
                | "log"
                | "sqrt"
                | "sin"
                | "cos"
                | "tan"
                | "tanh"
                | "abs"
                | "floor"
                | "ceil"
                | "round"
                | "sign"
                | "log2"
                | "log10"
                | "sinh"
                | "cosh"
                | "atan"
                | "cbrt"
                | "recip"
                | "fract"
                | "is_finite"
                | "norm"
                | "transpose"
                | "length"
                | "core::math::exp"
                | "core::math::ln"
                | "core::math::log"
                | "core::math::sqrt"
                | "core::math::sin"
                | "core::math::cos"
                | "core::math::tan"
                | "core::math::tanh"
                | "core::math::abs"
                | "core::math::floor"
                | "core::math::ceil"
                | "core::math::is_finite"
        );
        let binary = matches!(
            function,
            "min"
                | "max"
                | "atan2"
                | "pow"
                | "mod"
                | "hypot"
                | "dot"
                | "core::math::min"
                | "core::math::max"
                | "core::math::atan2"
                | "core::math::pow"
                | "core::math::mod"
                | "core::math::hypot"
        );
        let ternary = matches!(function, "lerp" | "core::math::lerp" | "clamp" | "core::math::clamp");
        let expected = match (unary, binary, ternary) {
            (true, false, false) => Some(1),
            (false, true, false) => Some(2),
            (false, false, true) => Some(3),
            _ => None,
        };
        if let Some(expected) = expected {
            if args.len() != expected {
                return Err(format!(
                    "`{function}` expects {expected} operand(s), got {}",
                    args.len()
                ));
            }
        }
        match function {
            "exp" | "core::math::exp" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Exp(v), span)
            }
            "ln" | "log" | "core::math::ln" | "core::math::log" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Ln(v), span)
            }
            "sqrt" | "core::math::sqrt" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::SqrtNonNegative);
                self.push(EmirOp::Sqrt(v), span)
            }
            "sin" | "core::math::sin" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sin(v), span)
            }
            "cos" | "core::math::cos" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cos(v), span)
            }
            "tan" | "core::math::tan" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Tan(v), span)
            }
            "tanh" | "core::math::tanh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Tanh(v), span)
            }
            "abs" | "core::math::abs" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Abs(v), span)
            }
            "floor" | "core::math::floor" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Floor(v), span)
            }
            "ceil" | "core::math::ceil" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Ceil(v), span)
            }
            "round" | "core::math::round" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Round(v), span)
            }
            "sign" | "core::math::sign" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sign(v), span)
            }
            "log2" | "core::math::log2" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Log2(v), span)
            }
            "log10" | "core::math::log10" => {
                let v = self.emit(package, args[0])?;
                self.obligations.push(DomainObligation::LogPositive);
                self.push(EmirOp::Log10(v), span)
            }
            "sinh" | "core::math::sinh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Sinh(v), span)
            }
            "cosh" | "core::math::cosh" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cosh(v), span)
            }
            "atan" | "core::math::atan" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Atan(v), span)
            }
            "cbrt" | "core::math::cbrt" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Cbrt(v), span)
            }
            "recip" | "core::math::recip" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Recip(v), span)
            }
            "fract" | "core::math::fract" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Fract(v), span)
            }
            "norm" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::VectorNorm(v), span)
            }
            "laplacian" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`laplacian` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                // Phase 1: dx must be a positive literal so the stencil weights
                // are fixed at emission (no runtime division, no IEEE inf/NaN).
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Clamp,
                    },
                    span,
                )
            }
            "laplacian_neumann" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`laplacian_neumann` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian_neumann` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian_neumann` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Neumann,
                    },
                    span,
                )
            }
            "laplacian_dirichlet" => {
                if args.len() != 4 {
                    return Err(format!(
                        "`laplacian_dirichlet` expects 4 operands (vector, dx, g_left, g_right), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`laplacian_dirichlet` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`laplacian_dirichlet` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let g_left = match package.expr(args[2]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => f64::from_bits(*bits),
                    _ => return Err(
                        "`laplacian_dirichlet` left boundary value must be a literal constant in Phase 1"
                            .to_string(),
                    ),
                };
                let g_right = match package.expr(args[3]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => f64::from_bits(*bits),
                    _ => return Err(
                        "`laplacian_dirichlet` right boundary value must be a literal constant in Phase 1"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (dx * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![inv, -2.0 * inv, inv],
                        center: 1,
                        edge: EdgePolicy::Dirichlet { left: g_left, right: g_right },
                    },
                    span,
                )
            }
            "laplacian_2d" | "laplacian_2d_neumann" => {
                if args.len() != 2 {
                    return Err(format!(
                        "`{function}` expects 2 operands (matrix, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`{function}` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(format!(
                        "`{function}` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                    )),
                };
                let inv = 1.0 / (dx * dx);
                // 5-point Laplacian: [[0,1,0],[1,-4,1],[0,1,0]] / dx^2.
                let weights = vec![
                    0.0, inv, 0.0,
                    inv, -4.0 * inv, inv,
                    0.0, inv, 0.0,
                ];
                let edge = if function == "laplacian_2d_neumann" {
                    EdgePolicy::Neumann
                } else {
                    EdgePolicy::Clamp
                };
                self.push(
                    EmirOp::Stencil2d {
                        input,
                        weights,
                        center: (1, 1),
                        edge,
                    },
                    span,
                )
            }
            "gradient" => {
                // 1-D central-difference first derivative (du/dx).
                // Reuses Stencil1d with weights [-1/(2dx), 0, +1/(2dx)] and
                // Clamp edges (one-sided at the boundary).
                if args.len() != 2 {
                    return Err(format!(
                        "`gradient` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`gradient` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(
                        "`gradient` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                            .to_string(),
                    ),
                };
                let inv = 1.0 / (2.0 * dx);
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: vec![-inv, 0.0, inv],
                        center: 1,
                        edge: EdgePolicy::Clamp,
                    },
                    span,
                )
            }
            "gradient_2d_x" | "gradient_2d_y" => {
                // 2-D central-difference first derivative of a scalar field
                // along one axis. Reuses Stencil2d with the 1-D central-
                // difference taps embedded in the middle row (du/dc, x) or
                // middle column (du/dr, y); the other taps are zero. Clamp
                // edges (one-sided at the boundary).
                if args.len() != 2 {
                    return Err(format!(
                        "`{function}` expects 2 operands (matrix, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = match package.expr(args[1]) {
                    Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
                        let v = f64::from_bits(*bits);
                        if !v.is_finite() || v <= 0.0 {
                            return Err(format!(
                                "`{function}` dx must be a positive finite literal, got {v:?}"
                            ));
                        }
                        v
                    }
                    _ => return Err(format!(
                        "`{function}` dx must be a positive literal constant in Phase 1; variable dx is not yet supported"
                    )),
                };
                let inv = 1.0 / (2.0 * dx);
                let weights = if function == "gradient_2d_x" {
                    // du/dc: taps at (1,0)=-inv and (1,2)=+inv.
                    vec![0.0, 0.0, 0.0, -inv, 0.0, inv, 0.0, 0.0, 0.0]
                } else {
                    // du/dr: taps at (0,1)=-inv and (2,1)=+inv.
                    vec![0.0, -inv, 0.0, 0.0, 0.0, 0.0, 0.0, inv, 0.0]
                };
                self.push(
                    EmirOp::Stencil2d {
                        input,
                        weights,
                        center: (1, 1),
                        edge: EdgePolicy::Clamp,
                    },
                    span,
                )
            }
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
            "min" | "core::math::min" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Min(l, r), span)
            }
            "max" | "core::math::max" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Max(l, r), span)
            }
            "atan2" | "core::math::atan2" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Atan2(l, r), span)
            }
            "mod" | "core::math::mod" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Mod(l, r), span)
            }
            "hypot" | "core::math::hypot" => {
                let l = self.emit(package, args[0])?;
                let r = self.emit(package, args[1])?;
                self.push(EmirOp::Hypot(l, r), span)
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
                let lo_clamped = self.push(EmirOp::Max(vx, vlo), span)?;
                self.push(EmirOp::Min(lo_clamped, vhi), span)
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
            "factorial" | "core::math::factorial" => {
                let n = self.emit(package, args[0])?;
                self.push(EmirOp::Factorial(n), span)
            }
            "mod_inv" | "core::math::mod_inv" => {
                let a = self.emit(package, args[0])?;
                let m = self.emit(package, args[1])?;
                self.push(EmirOp::ModInv(a, m), span)
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
            other => Err(format!("unknown function `{other}` in strict-f64 subset")),
        }
    }
}
