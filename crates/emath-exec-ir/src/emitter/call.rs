//! Built-in function dispatch: lowers `emit_call` built-in calls to EMIR ops.

use emath_core::{Span, special::SpecialFn};
use emath_ir::{ExprNode, Literal, SemanticPackage};

use crate::{BuiltinId, DomainObligation, EdgePolicy, EmirExprRef, EmirOp, EmirValue, ProbKind};

mod math_misc;
mod pde;
mod text_linalg;

fn positive_literal(
    package: &SemanticPackage,
    expression: EmirExprRef,
    function: &str,
    axis: &str,
) -> Result<f64, String> {
    match package.expr(expression) {
        Some(ExprNode::Literal(Literal::FloatBits(bits))) => {
            let value = f64::from_bits(*bits);
            if value.is_finite() && value > 0.0 {
                Ok(value)
            } else {
                Err(format!(
                    "`{function}` {axis} must be a positive finite literal, got {value:?}"
                ))
            }
        }
        _ => Err(format!(
            "`{function}` {axis} must be a positive literal constant"
        )),
    }
}

fn stencil3d_weights(axis_weights: [[f64; 3]; 3]) -> Vec<f64> {
    let mut weights = vec![0.0; 27];
    let center = 13;
    weights[center] = axis_weights[0][1] + axis_weights[1][1] + axis_weights[2][1];
    weights[4] = axis_weights[0][0];
    weights[22] = axis_weights[0][2];
    weights[10] = axis_weights[1][0];
    weights[16] = axis_weights[1][2];
    weights[12] = axis_weights[2][0];
    weights[14] = axis_weights[2][2];
    weights
}

fn derivative3d_weights(axis: usize, spacing: f64) -> Vec<f64> {
    let inv = 1.0 / (2.0 * spacing);
    let mut taps = [[0.0; 3]; 3];
    taps[axis] = [-inv, 0.0, inv];
    stencil3d_weights(taps)
}

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
        if function == "rat" {
            if args.len() != 2 {
                return Err(format!("`rat` expects 2 arguments, found {}", args.len()));
            }
            let num = self.emit(package, args[0])?;
            let den = self.emit(package, args[1])?;
            return self.push(EmirOp::RatConstruct { num, den }, span);
        }
        if function == "rat_add" {
            if args.len() != 2 {
                return Err(format!(
                    "`rat_add` expects 2 arguments, found {}",
                    args.len()
                ));
            }
            let left = self.emit(package, args[0])?;
            let right = self.emit(package, args[1])?;
            return self.push(EmirOp::RatAdd(left, right), span);
        }
        if function == "rat_norm" {
            if args.len() != 1 {
                return Err(format!(
                    "`rat_norm` expects 1 argument, found {}",
                    args.len()
                ));
            }
            let value = self.emit(package, args[0])?;
            return self.push(EmirOp::RatNorm(value), span);
        }
        if function == "series_at" {
            if args.len() != 2 {
                return Err(format!(
                    "`series_at` expects 2 arguments, found {}",
                    args.len()
                ));
            }
            let series = self.emit(package, args[0])?;
            let time = self.emit(package, args[1])?;
            return self.push(EmirOp::SeriesSample { series, time }, span);
        }
        if matches!(
            function,
            "normal_sample" | "uniform_sample" | "bernoulli_sample"
        ) {
            if !matches!(args.len(), 3 | 4) {
                return Err(format!(
                    "`{function}` expects (params, seed, draws[, stream]), found {} arguments",
                    args.len()
                ));
            }
            let kind = match function {
                "normal_sample" => ProbKind::Normal,
                "uniform_sample" => ProbKind::Uniform,
                _ => ProbKind::Bernoulli,
            };
            let params = self.emit(package, args[0])?;
            let seed = self.emit(package, args[1])?;
            let draws = self.emit(package, args[2])?;
            let stream = args
                .get(3)
                .map(|argument| self.emit(package, *argument))
                .transpose()?;
            return self.push(
                EmirOp::ProbSample {
                    kind,
                    params,
                    seed,
                    draws,
                    stream,
                },
                span,
            );
        }
        // Arity is enforced in every build, debug or release (bug-hunt
        // residual: debug_assert let empty/1-arg unary calls through to an
        // indexing panic and silently dropped extras in release).
        let builtin_id = BuiltinId::from_name(function);
        let unary = builtin_id.is_some_and(|id| id.arity() == 1)
            || matches!(
                function,
                "is_finite"
                    | "norm"
                    | "transpose"
                    | "length"
                    | "poisson_sine"
                    | "eigvals"
                    | "eigvecs"
                    | "singular_values"
                    | "svd_factors"
                    | "sparse_triplets"
                    | "out_degrees"
                    | "graph_laplacian"
                    | "graph_symmetrize"
                    | "pareto_front"
                    | "lu"
                    | "qr"
            );
        let binary = builtin_id.is_some_and(|id| id.arity() == 2)
            || matches!(
                function,
                "pow"
                    | "dot"
                    | "normal_density"
                    | "uniform_density"
                    | "bernoulli_pmf"
                    | "solve_iterative"
                    | "bellman_ford"
                    | "sparse_from_triplets"
                    | "poly_add"
                    | "poly_mul"
                    | "poly_eval"
                    | "reachability"
                    | "bfs_order"
                    | "shortest_distances"
                    | "solve_linear"
                    | "outer_product"
            );
        let ternary = matches!(function, "lerp" | "clamp" | "lp_minimize");
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
            "gamma"
            | "gamma_error_bound"
            | "beta"
            | "beta_error_bound"
            | "erf"
            | "erf_error_bound"
            | "zeta"
            | "zeta_error_bound"
            | "lambert_w0"
            | "lambert_w0_error_bound"
            | "elliptic_k"
            | "elliptic_k_error_bound"
            | "elliptic_e"
            | "elliptic_e_error_bound"
            | "elliptic_pi"
            | "elliptic_pi_error_bound"
            | "__format_text"
            | "text_length"
            | "nfc"
            | "section"
            | "document"
            | "render_markdown"
            | "render_latex"
            | "norm"
            | "poisson_sine"
            | "eigvals"
            | "eigvecs"
            | "singular_values"
            | "svd_factors"
            | "solve_iterative"
            | "bellman_ford"
            | "sparse_triplets"
            | "sparse_from_triplets"
            | "reachability"
            | "bfs_order"
            | "shortest_distances"
            | "out_degrees"
            | "graph_laplacian"
            | "graph_symmetrize"
            | "lp_minimize"
            | "pareto_front"
            | "solve_linear"
            | "lu"
            | "qr"
            | "outer_product"
            | "poly_add"
            | "poly_mul"
            | "poly_eval"
            | "generating_function"
            | "convolution"
            | "normal_density"
            | "uniform_density"
            | "bernoulli_pmf"
            | "not"
            | "core::logic::not" => self.emit_call_text_linalg(function, package, args, span),
            "laplacian_3d"
            | "laplacian_3d_neumann"
            | "gradient_3d_x"
            | "gradient_3d_y"
            | "gradient_3d_z"
            | "div"
            | "div_3d" => self.emit_call_pde(function, package, args, span),
            "transpose"
            | "length"
            | "dot"
            | "pow"
            | "core::math::pow"
            | "is_finite"
            | "core::math::is_finite"
            | "lerp"
            | "core::math::lerp"
            | "clamp"
            | "core::math::clamp"
            | "einsum"
            | "grad"
            | "factorial"
            | "core::math::factorial"
            | "mod_inv"
            | "core::math::mod_inv"
            | "field_inv"
            | "core::math::field_inv"
            | "int_rem"
            | "sqrt_mod"
            | "core::math::sqrt_mod"
            | "core::math::int_rem"
            | "pow_mod"
            | "core::math::pow_mod"
            | "congruence"
            | "poly_eval_mod"
            | "core::math::poly_eval_mod"
            | "rs_encode"
            | "core::math::rs_encode"
            | "hamming_distance"
            | "interval"
            | "intersect"
            | "option_some"
            | "option_none"
            | "option_is_some"
            | "option_unwrap_or"
            | "result_ok"
            | "result_err"
            | "result_is_ok"
            | "result_unwrap_or"
            | "result_error_of" => self.emit_call_math_misc(function, package, args, span),
            other => match BuiltinId::from_name(other) {
                // Registry-builtins written as calls share the exact
                // `BuiltinId` contracts the unary/binary SIR paths use
                // (no new opcodes, no per-domain branches). abs/min/max
                // are admitted outright; sqrt/atan2 carry their
                // documented contracts (see emit_call_builtin).
                Some(id) => self.emit_call_builtin(id, package, args, span),
                None => Err(format!("unknown function `{other}` in strict-f64 subset")),
            },
        }
    }
}
