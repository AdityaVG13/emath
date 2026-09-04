//! gamma-, text-, and linalg-analysis builtins (emit_call arms moved verbatim from call.rs).

use super::*;

impl super::super::Emitter {
    pub(super) fn emit_call_text_linalg(
        &mut self,
        function: &str,
        package: &SemanticPackage,
        args: &[EmirExprRef],
        span: Span,
    ) -> Result<EmirValue, String> {
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
            _ => unreachable!("emit_call_text_linalg routed a non-matching builtin"),
        }
    }
}
syntax error;
