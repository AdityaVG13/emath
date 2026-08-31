//! Built-in function dispatch: lowers `emit_call` built-in calls to EMIR ops.

use emath_core::{Span, special::SpecialFn};
use emath_ir::{ExprNode, Literal, SemanticPackage};

use crate::{BuiltinId, DomainObligation, EdgePolicy, EmirExprRef, EmirOp, EmirValue, ProbKind};

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

fn derivative1d_weights(spacing: f64) -> Vec<f64> {
    let inv = 1.0 / (2.0 * spacing);
    vec![-inv, 0.0, inv]
}

fn derivative2d_weights(axis: usize, spacing: f64) -> Vec<f64> {
    let inv = 1.0 / (2.0 * spacing);
    match axis {
        0 => vec![0.0, 0.0, 0.0, -inv, 0.0, inv, 0.0, 0.0, 0.0],
        _ => vec![0.0, -inv, 0.0, 0.0, 0.0, 0.0, 0.0, inv, 0.0],
    }
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
                    | "poles_stable"
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
        let ternary = matches!(
            function,
            "lerp" | "clamp" | "transfer_eval" | "dc_gain" | "lp_minimize"
        );
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
            | "elliptic_pi_error_bound" => {
                let error_bound = function.ends_with("_error_bound");
                let base = function.strip_suffix("_error_bound").unwrap_or(function);
                let special = match base {
                    "gamma" => SpecialFn::Gamma,
                    "beta" => SpecialFn::Beta,
                    "erf" => SpecialFn::Erf,
                    "zeta" => SpecialFn::Zeta,
                    "lambert_w0" => SpecialFn::LambertW0,
                    "elliptic_k" => SpecialFn::EllipticK,
                    "elliptic_e" => SpecialFn::EllipticE,
                    "elliptic_pi" => SpecialFn::EllipticPi,
                    _ => unreachable!(),
                };
                let arguments = args
                    .iter()
                    .map(|argument| self.emit(package, *argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.push(
                    EmirOp::SpecialFunction {
                        function: special,
                        arguments,
                        error_bound,
                    },
                    span,
                )
            }
            "__format_text" => {
                let Some(template_id) = args.first() else {
                    return Err("`__format_text` requires a template".to_string());
                };
                let template = match package.expr(*template_id) {
                    Some(ExprNode::Literal(Literal::Text(template))) => template.clone(),
                    _ => return Err("`__format_text` template must be a text literal".to_string()),
                };
                let arguments = args[1..]
                    .iter()
                    .map(|argument| self.emit(package, *argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.push(
                    EmirOp::FormatText {
                        template,
                        arguments,
                    },
                    span,
                )
            }
            "text_length" => {
                let text = self.emit(package, args[0])?;
                self.push(EmirOp::TextLength(text), span)
            }
            "nfc" => {
                let text = self.emit(package, args[0])?;
                self.push(EmirOp::TextNfc(text), span)
            }
            "section" => {
                let heading = self.emit(package, args[0])?;
                let body = self.emit(package, args[1])?;
                self.push(EmirOp::ReportSection { heading, body }, span)
            }
            "document" => {
                let title = self.emit(package, args[0])?;
                let section = self.emit(package, args[1])?;
                self.push(EmirOp::ReportDocument { title, section }, span)
            }
            "render_markdown" => {
                let document = self.emit(package, args[0])?;
                self.push(EmirOp::ReportMarkdown(document), span)
            }
            "render_latex" => {
                let document = self.emit(package, args[0])?;
                self.push(EmirOp::ReportLatex(document), span)
            }
            f if BuiltinId::from_name(f).is_some() => {
                let id = BuiltinId::from_name(f).unwrap();
                match id.arity() {
                    1 => {
                        let v = self.emit(package, args[0])?;
                        for obl in id.domain_obligations() {
                            self.obligations.push(*obl);
                        }
                        self.push(EmirOp::UnaryBuiltin(id, v), span)
                    }
                    2 => {
                        let l = self.emit(package, args[0])?;
                        let r = self.emit(package, args[1])?;
                        for obl in id.domain_obligations() {
                            self.obligations.push(*obl);
                        }
                        self.push(EmirOp::BinaryBuiltin(id, l, r), span)
                    }
                    _ => unreachable!(),
                }
            }
            "norm" => {
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::VectorNorm(v), span)
            }
            "poisson_sine" => {
                let load = self.emit(package, args[0])?;
                self.push(EmirOp::PoissonDirichletSine(load), span)
            }
            "eigvals" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::EigenSymmetric(matrix), span)
            }
            "eigvecs" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::EigenVectorsSymmetric(matrix), span)
            }
            "singular_values" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::SvdSingularValues(matrix), span)
            }
            "svd_factors" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::SvdFactors(matrix), span)
            }
            "solve_iterative" => {
                let matrix = self.emit(package, args[0])?;
                let rhs = self.emit(package, args[1])?;
                self.push(EmirOp::CgSolve(matrix, rhs), span)
            }
            "bellman_ford" => {
                let matrix = self.emit(package, args[0])?;
                let source = self.emit(package, args[1])?;
                self.push(EmirOp::GraphBellmanFord(matrix, source), span)
            }
            "sparse_triplets" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::GraphSparseTriplets(matrix), span)
            }
            "sparse_from_triplets" => {
                let n = self.emit(package, args[0])?;
                let triplets = self.emit(package, args[1])?;
                self.push(EmirOp::GraphSparseFromTriplets(n, triplets), span)
            }
            "reachability" | "bfs_order" | "shortest_distances" => {
                let matrix = self.emit(package, args[0])?;
                let source = self.emit(package, args[1])?;
                let op = match function {
                    "reachability" => EmirOp::GraphReachable(matrix, source),
                    "bfs_order" => EmirOp::GraphBfsOrder(matrix, source),
                    _ => EmirOp::GraphDijkstra(matrix, source),
                };
                self.push(op, span)
            }
            "out_degrees" | "graph_laplacian" | "graph_symmetrize" => {
                let matrix = self.emit(package, args[0])?;
                let op = match function {
                    "out_degrees" => EmirOp::GraphDegreeOut(matrix),
                    "graph_laplacian" => EmirOp::GraphLaplacian(matrix),
                    _ => EmirOp::GraphSymmetrize(matrix),
                };
                self.push(op, span)
            }
            "lp_minimize" => {
                let constraints = self.emit(package, args[0])?;
                let bounds = self.emit(package, args[1])?;
                let objective = self.emit(package, args[2])?;
                self.push(EmirOp::LpMinimize(constraints, bounds, objective), span)
            }
            "pareto_front" => {
                let points = self.emit(package, args[0])?;
                self.push(EmirOp::ParetoFront(points), span)
            }
            "solve_linear" => {
                let matrix = self.emit(package, args[0])?;
                let rhs = self.emit(package, args[1])?;
                self.push(EmirOp::LinearSolve(matrix, rhs), span)
            }
            "lu" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::LuFactors(matrix), span)
            }
            "qr" => {
                let matrix = self.emit(package, args[0])?;
                self.push(EmirOp::QrFactors(matrix), span)
            }
            "outer_product" => {
                let left = self.emit(package, args[0])?;
                let right = self.emit(package, args[1])?;
                self.push(EmirOp::OuterProduct(left, right), span)
            }
            "transfer_eval" => {
                let numerator = self.emit(package, args[0])?;
                let denominator = self.emit(package, args[1])?;
                let point = self.emit(package, args[2])?;
                self.push(
                    EmirOp::ControlTransferEval(numerator, denominator, point),
                    span,
                )
            }
            "dc_gain" => {
                let matrix = self.emit(package, args[0])?;
                let input = self.emit(package, args[1])?;
                let output = self.emit(package, args[2])?;
                self.push(EmirOp::ControlDcGain(matrix, input, output), span)
            }
            "poles_stable" => {
                let denominator = self.emit(package, args[0])?;
                self.push(EmirOp::ControlPolesStable(denominator), span)
            }
            "poly_add" => {
                let left = self.emit(package, args[0])?;
                let right = self.emit(package, args[1])?;
                self.push(EmirOp::VectorAdd(left, right), span)
            }
            "poly_mul" => {
                let left = self.emit(package, args[0])?;
                let right = self.emit(package, args[1])?;
                self.push(EmirOp::PolyMul(left, right), span)
            }
            "poly_eval" => {
                let coefficients = self.emit(package, args[0])?;
                let point = self.emit(package, args[1])?;
                self.push(EmirOp::PolyEval(coefficients, point), span)
            }
            "generating_function" => {
                let initial = self.emit(package, args[0])?;
                let recurrence = self.emit(package, args[1])?;
                let budget = self.emit(package, args[2])?;
                self.push(
                    EmirOp::SequenceGenerate {
                        initial,
                        recurrence,
                        budget,
                    },
                    span,
                )
            }
            "convolution" => {
                let left = self.emit(package, args[0])?;
                let right = self.emit(package, args[1])?;
                let count = self.emit(package, args[2])?;
                self.push(EmirOp::SequenceConvolve { left, right, count }, span)
            }
            "normal_density" | "uniform_density" | "bernoulli_pmf" => {
                let kind = match function {
                    "normal_density" => ProbKind::Normal,
                    "uniform_density" => ProbKind::Uniform,
                    _ => ProbKind::Bernoulli,
                };
                let params = self.emit(package, args[0])?;
                let x = self.emit(package, args[1])?;
                self.push(EmirOp::ProbDensity { kind, params, x }, span)
            }
            "not" | "core::logic::not" => {
                // Boolean complement, callable so `notation` targets like
                // `core::logic::not` compute (operand typing is enforced
                // at admission).
                if args.len() != 1 {
                    return Err(format!("`not` expects 1 operand, got {}", args.len()));
                }
                let v = self.emit(package, args[0])?;
                self.push(EmirOp::Not(v), span)
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
                let dx = positive_literal(package, args[1], function, "dx")?;
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
                let dx = positive_literal(package, args[1], function, "dx")?;
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
                let dx = positive_literal(package, args[1], function, "dx")?;
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
                        edge: EdgePolicy::Dirichlet {
                            left: g_left,
                            right: g_right,
                        },
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
                let dx = positive_literal(package, args[1], function, "dx")?;
                let inv = 1.0 / (dx * dx);
                // 5-point Laplacian: [[0,1,0],[1,-4,1],[0,1,0]] / dx^2.
                let weights = vec![0.0, inv, 0.0, inv, -4.0 * inv, inv, 0.0, inv, 0.0];
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
                // OneSided edges (linear ghost → first-order one-sided
                // difference, exact on linear fields). Clamp on this stencil
                // would return half the true slope at the boundary.
                if args.len() != 2 {
                    return Err(format!(
                        "`gradient` expects 2 operands (vector, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = positive_literal(package, args[1], function, "dx")?;
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: derivative1d_weights(dx),
                        center: 1,
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )
            }
            "gradient_2d_x" | "gradient_2d_y" => {
                // 2-D central-difference first derivative of a scalar field
                // along one axis. Reuses Stencil2d with the 1-D central-
                // difference taps embedded in the middle row (du/dc, x) or
                // middle column (du/dr, y); the other taps are zero.
                // OneSided edges: linear ghost so a slope-1 ramp is 1 at
                // the boundary, not the Clamp-central artifact 0.5.
                if args.len() != 2 {
                    return Err(format!(
                        "`{function}` expects 2 operands (matrix, dx), got {}",
                        args.len()
                    ));
                }
                let input = self.emit(package, args[0])?;
                let dx = positive_literal(package, args[1], function, "dx")?;
                let axis = usize::from(function == "gradient_2d_y");
                self.push(
                    EmirOp::Stencil2d {
                        input,
                        weights: derivative2d_weights(axis, dx),
                        center: (1, 1),
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )
            }
            "laplacian_3d" | "laplacian_3d_neumann" => {
                if !matches!(args.len(), 2 | 4) {
                    return Err(format!(
                        "`{function}` expects (tensor, spacing) or (tensor, dx, dy, dz)"
                    ));
                }
                let input = self.emit(package, args[0])?;
                let spacing = if args.len() == 2 {
                    let h = positive_literal(package, args[1], function, "spacing")?;
                    [h, h, h]
                } else {
                    [
                        positive_literal(package, args[1], function, "dx")?,
                        positive_literal(package, args[2], function, "dy")?,
                        positive_literal(package, args[3], function, "dz")?,
                    ]
                };
                let inv = spacing.map(|h| 1.0 / (h * h));
                let weights = stencil3d_weights([
                    [inv[0], -2.0 * inv[0], inv[0]],
                    [inv[1], -2.0 * inv[1], inv[1]],
                    [inv[2], -2.0 * inv[2], inv[2]],
                ]);
                let edge = if function == "laplacian_3d_neumann" {
                    EdgePolicy::Neumann
                } else {
                    EdgePolicy::Clamp
                };
                self.push(
                    EmirOp::Stencil3d {
                        input,
                        weights,
                        center: (1, 1, 1),
                        edge,
                    },
                    span,
                )
            }
            "gradient_3d_x" | "gradient_3d_y" | "gradient_3d_z" => {
                if args.len() != 2 {
                    return Err(format!("`{function}` expects (tensor, spacing)"));
                }
                let input = self.emit(package, args[0])?;
                let spacing = positive_literal(package, args[1], function, "spacing")?;
                let axis = match function {
                    "gradient_3d_x" => 0,
                    "gradient_3d_y" => 1,
                    _ => 2,
                };
                self.push(
                    EmirOp::Stencil3d {
                        input,
                        weights: derivative3d_weights(axis, spacing),
                        center: (1, 1, 1),
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )
            }
            "div_1d" => {
                if args.len() != 2 {
                    return Err("`div_1d` expects (vx, dx)".to_string());
                }
                let input = self.emit(package, args[0])?;
                let dx = positive_literal(package, args[1], function, "dx")?;
                self.push(
                    EmirOp::Stencil1d {
                        input,
                        weights: derivative1d_weights(dx),
                        center: 1,
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )
            }
            "div_2d" => {
                if !matches!(args.len(), 3 | 4) {
                    return Err(
                        "`div_2d` expects (vx, vy, spacing) or (vx, vy, dx, dy)".to_string()
                    );
                }
                let vx = self.emit(package, args[0])?;
                let vy = self.emit(package, args[1])?;
                let (dx, dy) = if args.len() == 3 {
                    let h = positive_literal(package, args[2], function, "spacing")?;
                    (h, h)
                } else {
                    (
                        positive_literal(package, args[2], function, "dx")?,
                        positive_literal(package, args[3], function, "dy")?,
                    )
                };
                let x = self.push(
                    EmirOp::Stencil2d {
                        input: vx,
                        weights: derivative2d_weights(0, dx),
                        center: (1, 1),
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )?;
                let y = self.push(
                    EmirOp::Stencil2d {
                        input: vy,
                        weights: derivative2d_weights(1, dy),
                        center: (1, 1),
                        edge: EdgePolicy::OneSided,
                    },
                    span,
                )?;
                self.push(EmirOp::MatrixAdd(x, y), span)
            }
            "div" | "div_3d" => {
                if !matches!(args.len(), 4 | 6) {
                    return Err(format!(
                        "`{function}` expects (vx, vy, vz, spacing) or (vx, vy, vz, dx, dy, dz)"
                    ));
                }
                let fields = [
                    self.emit(package, args[0])?,
                    self.emit(package, args[1])?,
                    self.emit(package, args[2])?,
                ];
                let spacing = if args.len() == 4 {
                    let h = positive_literal(package, args[3], function, "spacing")?;
                    [h, h, h]
                } else {
                    [
                        positive_literal(package, args[3], function, "dx")?,
                        positive_literal(package, args[4], function, "dy")?,
                        positive_literal(package, args[5], function, "dz")?,
                    ]
                };
                let mut derivatives = [EmirValue(0); 3];
                for axis in 0..3 {
                    derivatives[axis] = self.push(
                        EmirOp::Stencil3d {
                            input: fields[axis],
                            weights: derivative3d_weights(axis, spacing[axis]),
                            center: (1, 1, 1),
                            edge: EdgePolicy::OneSided,
                        },
                        span,
                    )?;
                }
                let xy = self.push(EmirOp::TensorAdd(derivatives[0], derivatives[1]), span)?;
                self.push(EmirOp::TensorAdd(xy, derivatives[2]), span)
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
                // Prime-field inverse (aj8d pass 7): field_inv(a, p) is
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
                // Certified interval constructor `interval(lo, hi)` (8pjn).
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
            // Option/Result call surface (aj8d): the nine names lower to
            // the SAME total value-semantics ops the term-compiler binds
            // and the reference VM executes (OptionSome/OptionNone/
            // OptionIsSome/OptionUnwrapOr, ResultOk/ResultErr/
            // ResultIsOk/ResultUnwrapOr/ResultErrorOf). There is NO
            // panicking unwrap: unwrap_or is the honesty gate and its
            // default evaluates eagerly. Arity is enforced HERE (the
            // strict-f64 gate lets no malformed call through); backend
            // codegen owns the typed carrier refusal (pass-5 lane).
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
            other => Err(format!("unknown function `{other}` in strict-f64 subset")),
        }
    }
}
