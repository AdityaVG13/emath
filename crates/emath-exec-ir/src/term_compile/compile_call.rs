//! `Compiler::compile_call`: named-call compilation to generic EMIR ops.

use super::*;

impl Compiler {
    pub(crate) fn compile_call(
        &mut self,
        name: &str,
        args: &[(EmirValue, Shape)],
    ) -> Result<(EmirValue, Shape), TermCompileError> {
        // Closed comparison vocabulary: scalar/scalar →
        // the generic comparison ops; the result is a Bool, and the
        // closed vocabulary composes booleans NOWHERE (Shape::Bool is
        // rejected by every other arm).
        if let Some(comparison) = (match name {
            "lt" => Some(EmirOp::Lt as fn(EmirValue, EmirValue) -> EmirOp),
            "le" => Some(EmirOp::Le as fn(EmirValue, EmirValue) -> EmirOp),
            "gt" => Some(EmirOp::Gt as fn(EmirValue, EmirValue) -> EmirOp),
            "ge" => Some(EmirOp::Ge as fn(EmirValue, EmirValue) -> EmirOp),
            "eq" => Some(EmirOp::Eq as fn(EmirValue, EmirValue) -> EmirOp),
            "ne" => Some(EmirOp::Ne as fn(EmirValue, EmirValue) -> EmirOp),
            _ => None,
        }) {
            return match args {
                [(a, Shape::Scalar), (b, Shape::Scalar)] => {
                    Ok((self.push(comparison(*a, *b))?, Shape::Bool))
                }
                [_] | [_, _, ..] => Err(TermCompileError::ArityMismatch {
                    symbol: name.to_string(),
                    expected: 2,
                    actual: args.len(),
                }),
                _ => Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: "comparisons admit scalar/scalar only".to_string(),
                }),
            };
        }
        // Strict-f64 arithmetic first (the cell policies are strict; the
        // core numeric vocabulary keeps its spelling).
        match (name, args) {
            (op @ ("add" | "sub" | "mul" | "div"), [a, b]) => {
                return self.compile_arith(op, *a, *b);
            }
            (op @ ("add" | "sub" | "mul" | "div"), _) => {
                return Err(TermCompileError::ArityMismatch {
                    symbol: op.to_string(),
                    expected: 2,
                    actual: args.len(),
                });
            }
            _ => {}
        }
        // Generic math builtins: scalar -> UnaryBuiltin/BinaryBuiltin;
        // vector -> elementwise map (broadcast) over the closed registry.
        if let Some(builtin) = BuiltinId::from_name(name) {
            return match (builtin.arity(), args) {
                (1, [(source, Shape::Vector)]) => Ok((
                    self.push(EmirOp::VectorMap {
                        builtin,
                        source: *source,
                    })?,
                    Shape::Vector,
                )),
                (1, [(source, Shape::Scalar)]) => Ok((
                    self.push(EmirOp::UnaryBuiltin(builtin, *source))?,
                    Shape::Scalar,
                )),
                (2, [(a, Shape::Scalar), (b, Shape::Scalar)]) => Ok((
                    self.push(EmirOp::BinaryBuiltin(builtin, *a, *b))?,
                    Shape::Scalar,
                )),
                (2, _) => Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: "binary builtin broadcast over a vector is not in \
                             the closed reference vocabulary; vector-scalar \
                             arithmetic is add/sub/mul/div"
                        .to_string(),
                }),
                (_, _) => Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: format!(
                        "builtin arity {} does not match the term's application",
                        builtin.arity()
                    ),
                }),
            };
        }
        // Closed vector aggregation and construction vocabulary.
        match (name, args) {
            // Linear-algebra names (B35): the registry path binds
            // the SAME generic ops the emitter path already lowers —
            // zero new op variants, zero per-op VM code.
            ("norm", [(source, Shape::Vector)]) => {
                Ok((self.push(EmirOp::VectorNorm(*source))?, Shape::Scalar))
            }
            ("norm", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "norm requires exactly one vector-shaped argument".to_string(),
            }),
            ("dot", [(a, Shape::Vector), (b, Shape::Vector)]) => {
                Ok((self.push(EmirOp::VectorDot(*a, *b))?, Shape::Scalar))
            }
            ("dot", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "dot requires exactly two vector-shaped arguments".to_string(),
            }),
            // Dense matrix×vector: the registry
            // name binds the SAME generic op the emitter path lowers —
            // zero new op variants, zero per-op VM code (the B35
            // precedent). The chemistry mass-balance cell is DATA over
            // this name.
            ("matvec", [(matrix, Shape::Matrix), (vector, Shape::Vector)]) => Ok((
                self.push(EmirOp::MatrixMulVector(*matrix, *vector))?,
                Shape::Vector,
            )),
            ("matvec", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "matvec requires exactly (matrix, vector)".to_string(),
            }),
            // Exact integer null vector: matrix → primitive
            // vector; a name binding on the generic IntNullspace op.
            ("int_nullspace", [(matrix, Shape::Matrix)]) => Ok((
                self.push(EmirOp::IntNullspace(*matrix))?,
                Shape::Vector,
            )),
            ("int_nullspace", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "int_nullspace requires exactly one matrix".to_string(),
            }),
            // Exact integer product difference: the
            // generic exact-rational equality primitive; (vector,
            // vector) → scalar.
            ("exact_product_delta", [(p, Shape::Vector), (q, Shape::Vector)]) => Ok((
                self.push(EmirOp::ExactProductDelta(*p, *q))?,
                Shape::Scalar,
            )),
            ("exact_product_delta", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "exact_product_delta requires exactly two vectors".to_string(),
            }),
            ("solve_linear", [(matrix, Shape::Matrix), (rhs, Shape::Vector)]) => Ok((
                self.push(EmirOp::LinearSolve(*matrix, *rhs))?,
                Shape::Vector,
            )),
            ("solve_linear", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "solve_linear requires (matrix, vector)".to_string(),
            }),
            ("lu", [(matrix, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::LuFactors(*matrix))?, Shape::Matrix))
            }
            ("lu", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "lu requires exactly one matrix".to_string(),
            }),
            ("qr", [(matrix, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::QrFactors(*matrix))?, Shape::Matrix))
            }
            ("qr", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "qr requires exactly one matrix".to_string(),
            }),
            ("outer_product", [(left, Shape::Vector), (right, Shape::Vector)]) => Ok((
                self.push(EmirOp::OuterProduct(*left, *right))?,
                Shape::Matrix,
            )),
            ("outer_product", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "outer_product requires exactly two vectors".to_string(),
            }),
            // Graph algorithms: the call
            // surface binds the slice-1 EMIR ops over the dense
            // adjacency carrier. A non-matrix value in the adjacency
            // slot refuses at COMPILE (the closed vocabulary's shape
            // law) — never a silent mis-lowering.
            (graph @ ("reachability" | "bfs_order" | "shortest_distances"), [adj, source])
                if matches!(adj.1, Shape::Matrix) && source.1 == Shape::Scalar =>
            {
                let operand = match graph {
                    "reachability" => EmirOp::GraphReachable(adj.0, source.0),
                    "bfs_order" => EmirOp::GraphBfsOrder(adj.0, source.0),
                    _ => EmirOp::GraphDijkstra(adj.0, source.0),
                };
                Ok((self.push(operand)?, Shape::Vector))
            }
            (graph @ ("reachability" | "bfs_order" | "shortest_distances"), _) => {
                Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: format!(
                        "{graph} requires a matrix adjacency carrier and a scalar source vertex"
                    ),
                })
            }
            ("out_degrees", [(adj, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::GraphDegreeOut(*adj))?, Shape::Vector))
            }
            ("out_degrees", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "out_degrees requires exactly one matrix adjacency carrier".to_string(),
            }),
            // Spectral basics: the unnormalized
            // Laplacian; the spectrum composes through the EXISTING
            // symmetric eigen op (undirected carriers only — the
            // documented fence).
            ("graph_laplacian", [(adj, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::GraphLaplacian(*adj))?, Shape::Matrix))
            }
            ("graph_laplacian", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "graph_laplacian requires exactly one matrix adjacency carrier".to_string(),
            }),
            // Symmetrized adjacency: matrix in, matrix
            // out; a scalar adjacency refuses at COMPILE.
            ("graph_symmetrize", [(adj, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::GraphSymmetrize(*adj))?, Shape::Matrix))
            }
            ("graph_symmetrize", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "graph_symmetrize requires exactly one matrix adjacency carrier"
                    .to_string(),
            }),
            // Negative-edge shortest paths: (matrix,
            // scalar source) in, distance vector out; wrong shapes
            // refuse at COMPILE.
            ("bellman_ford", [(adj, Shape::Matrix), (source, Shape::Scalar)]) => Ok((
                self.push(EmirOp::GraphBellmanFord(*adj, *source))?,
                Shape::Vector,
            )),
            ("bellman_ford", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "bellman_ford requires (matrix adjacency carrier, scalar source)"
                    .to_string(),
            }),
            // Sparse storage: extraction is matrix →
            // vector; build is (scalar n, vector triplets) → matrix.
            ("sparse_triplets", [(adj, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::GraphSparseTriplets(*adj))?, Shape::Vector))
            }
            ("sparse_triplets", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "sparse_triplets requires exactly one matrix adjacency carrier".to_string(),
            }),
            ("sparse_from_triplets", [(n, Shape::Scalar), (triplets, Shape::Vector)]) => Ok((
                self.push(EmirOp::GraphSparseFromTriplets(*n, *triplets))?,
                Shape::Matrix,
            )),
            ("sparse_from_triplets", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "sparse_from_triplets requires (scalar vertex count, vector triplets)"
                    .to_string(),
            }),
            // Optimization: the standard-form
            // LP and the strict Pareto front over finite carriers.
            // Non-matrix constraint/objective carriers refuse at
            // COMPILE (the closed vocabulary's shape law).
            ("lp_minimize", [(a, Shape::Matrix), (b, Shape::Vector), (c, Shape::Vector)]) => {
                Ok((self.push(EmirOp::LpMinimize(*a, *b, *c))?, Shape::Vector))
            }
            ("lp_minimize", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "lp_minimize requires (constraint matrix, right side, objective) \
                         in shapes (matrix, vector, vector)"
                    .to_string(),
            }),
            ("pareto_front", [(points, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::ParetoFront(*points))?, Shape::Vector))
            }
            ("pareto_front", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "pareto_front requires exactly one matrix objective carrier".to_string(),
            }),
            // Polynomials as values:
            // dense ascending coefficient vectors. Addition is the
            // EXISTING generic vector add (a name binding, the
            // precedent); multiplication is the convolution; evaluation
            // is Horner. Non-vector coefficient slots refuse at COMPILE.
            ("poly_add", [(a, Shape::Vector), (b, Shape::Vector)]) => {
                Ok((self.push(EmirOp::VectorAdd(*a, *b))?, Shape::Vector))
            }
            ("poly_add", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poly_add requires exactly two vector-shaped coefficient carriers"
                    .to_string(),
            }),
            ("poly_mul", [(a, Shape::Vector), (b, Shape::Vector)]) => {
                Ok((self.push(EmirOp::PolyMul(*a, *b))?, Shape::Vector))
            }
            ("poly_mul", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poly_mul requires exactly two vector-shaped coefficient carriers"
                    .to_string(),
            }),
            ("poly_eval", [(p, Shape::Vector), (x, Shape::Scalar)]) => {
                Ok((self.push(EmirOp::PolyEval(*p, *x))?, Shape::Scalar))
            }
            ("poly_eval", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poly_eval requires (coefficient vector, scalar point)".to_string(),
            }),
            // Spectral Poisson (thin nucleus): vector load in,
            // vector field out; a scalar load refuses at COMPILE.
            ("poisson_sine", [(f, Shape::Vector)]) => {
                Ok((self.push(EmirOp::PoissonDirichletSine(*f))?, Shape::Vector))
            }
            ("poisson_sine", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poisson_sine requires a vector-shaped interior load".to_string(),
            }),
            // Control surface (thin B43): transfer eval is
            // (vector num, vector den, scalar point) → scalar; DC gain
            // is (matrix A, vector b, vector c) → scalar; the
            // Routh–Hurwitz predicate is vector → bool. Wrong shapes
            // refuse at COMPILE.
            (
                "transfer_eval",
                [
                    (num, Shape::Vector),
                    (den, Shape::Vector),
                    (x, Shape::Scalar),
                ],
            ) => Ok((
                self.push(EmirOp::ControlTransferEval(*num, *den, *x))?,
                Shape::Scalar,
            )),
            ("transfer_eval", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "transfer_eval requires (vector numerator, vector denominator, \
                         scalar point)"
                    .to_string(),
            }),
            ("dc_gain", [(a, Shape::Matrix), (b, Shape::Vector), (c, Shape::Vector)]) => {
                Ok((self.push(EmirOp::ControlDcGain(*a, *b, *c))?, Shape::Scalar))
            }
            ("dc_gain", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "dc_gain requires (matrix A, vector b, vector c)".to_string(),
            }),
            ("poles_stable", [(den, Shape::Vector)]) => {
                Ok((self.push(EmirOp::ControlPolesStable(*den))?, Shape::Bool))
            }
            ("poles_stable", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poles_stable requires exactly one vector denominator".to_string(),
            }),
            // Finite-category surface (thin B39): the law gate is
            // (vector dom, vector cod, matrix comp) → bool;
            // commutativity adds the vector face stream. Wrong shapes
            // refuse at COMPILE.
            (
                "category_check",
                [
                    (dom, Shape::Vector),
                    (cod, Shape::Vector),
                    (comp, Shape::Matrix),
                ],
            ) => Ok((
                self.push(EmirOp::CategoryCheck(*dom, *cod, *comp))?,
                Shape::Bool,
            )),
            ("category_check", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "category_check requires (vector dom, vector cod, matrix comp)".to_string(),
            }),
            (
                "diagram_commutative",
                [
                    (dom, Shape::Vector),
                    (cod, Shape::Vector),
                    (comp, Shape::Matrix),
                    (faces, Shape::Vector),
                ],
            ) => Ok((
                self.push(EmirOp::CategoryDiagramCommutative(
                    *dom, *cod, *comp, *faces,
                ))?,
                Shape::Vector,
            )),
            ("diagram_commutative", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "diagram_commutative requires (vector dom, vector cod, matrix comp, \
                         vector faces)"
                    .to_string(),
            }),
            // Probability nucleus: seeded sampling + exact
            // densities. Params are vector carriers; seed/draws/x are
            // scalars. Wrong carrier shapes refuse at COMPILE.
            (
                op @ ("normal_sample" | "uniform_sample" | "bernoulli_sample"),
                [
                    (params, Shape::Vector),
                    (seed, Shape::Scalar),
                    (draws, Shape::Scalar),
                ],
            ) => {
                let kind = match op {
                    "normal_sample" => ProbKind::Normal,
                    "uniform_sample" => ProbKind::Uniform,
                    _ => ProbKind::Bernoulli,
                };
                Ok((
                    self.push(EmirOp::ProbSample {
                        kind,
                        params: *params,
                        seed: *seed,
                        draws: *draws,
                        stream: None,
                    })?,
                    Shape::Vector,
                ))
            }
            ("normal_sample" | "uniform_sample" | "bernoulli_sample", _) => {
                Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: "sampling calls require (params vector, scalar seed, scalar draws)"
                        .to_string(),
                })
            }
            (
                op @ ("normal_density" | "uniform_density" | "bernoulli_pmf"),
                [(params, Shape::Vector), (x, Shape::Scalar)],
            ) => {
                let kind = match op {
                    "normal_density" => ProbKind::Normal,
                    "uniform_density" => ProbKind::Uniform,
                    _ => ProbKind::Bernoulli,
                };
                Ok((
                    self.push(EmirOp::ProbDensity {
                        kind,
                        params: *params,
                        x: *x,
                    })?,
                    Shape::Scalar,
                ))
            }
            ("normal_density" | "uniform_density" | "bernoulli_pmf", _) => {
                Err(TermCompileError::ShapeMismatch {
                    symbol: name.to_string(),
                    detail: "density calls require (params vector, scalar point)".to_string(),
                })
            }
            // ── Option/Result call surface ──────────────────
            // Nine names binding the TOTAL value-semantics ops the
            // interp already executes (Some/None/Ok/Err constructors,
            // is_some/is_ok polarity, the unwrap_or honesty gate — a
            // missing value yields the caller's eagerly-evaluated
            // default, NO panicking unwrap exists at this layer — and
            // error_of, the Result error composed AS an Option).
            // Nested payloads are the type-honest rule: a
            // carrier is an acceptable payload for the three CONSTRUCTORS
            // and an acceptable unwrap_or default when the retrieved
            // payload is a carrier (Some(None), Some(Some(5)),
            // Ok(Some(1))). Bool still composes nowhere (booleans compose
            // in the closed vocabulary). Carriers are still refused in
            // the FIRST slot of predicates/unwrap/error_of, and Bool
            // refuses in every slot. Every mismatch is a TYPED
            // TermCompileError, never a panic.
            ("option_some", [(payload, shape)]) if shape.is_payload_candidate() => Ok((
                self.push(EmirOp::OptionSome(*payload))?,
                Shape::OptionCarrier,
            )),
            ("option_some", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "option_some".to_string(),
                detail: format!(
                    "option_some requires exactly one Scalar/Vector/Matrix/Option/Result payload, got {shape:?}"
                ),
            }),
            ("option_some", _) => Err(TermCompileError::ArityMismatch {
                symbol: "option_some".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            ("option_none", []) => Ok((self.push(EmirOp::OptionNone)?, Shape::OptionCarrier)),
            ("option_none", _) => Err(TermCompileError::ArityMismatch {
                symbol: "option_none".to_string(),
                expected: 0,
                actual: args.len(),
            }),
            ("option_is_some", [(carrier, Shape::OptionCarrier)]) => Ok((
                self.push(EmirOp::OptionIsSome(*carrier))?,
                Shape::Bool,
            )),
            ("option_is_some", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "option_is_some".to_string(),
                detail: format!("option_is_some requires an Option carrier, got {shape:?}"),
            }),
            ("option_is_some", _) => Err(TermCompileError::ArityMismatch {
                symbol: "option_is_some".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            ("option_unwrap_or", [(carrier, Shape::OptionCarrier), (default, shape)])
                if shape.is_concrete_payload() || matches!(shape, Shape::OptionCarrier) =>
            {
                Ok((
                    self.push(EmirOp::OptionUnwrapOr(*carrier, *default))?,
                    *shape,
                ))
            }
            ("option_unwrap_or", _) => Err(TermCompileError::ShapeMismatch {
                symbol: "option_unwrap_or".to_string(),
                detail: "option_unwrap_or requires (Option carrier, Scalar/Vector/Matrix/Option/Result default)"
                    .to_string(),
            }),
            ("result_ok", [(payload, shape)]) if shape.is_payload_candidate() => Ok((
                self.push(EmirOp::ResultOk(*payload))?,
                Shape::ResultCarrier,
            )),
            ("result_ok", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "result_ok".to_string(),
                detail: format!(
                    "result_ok requires exactly one Scalar/Vector/Matrix/Option/Result payload, got {shape:?}"
                ),
            }),
            ("result_ok", _) => Err(TermCompileError::ArityMismatch {
                symbol: "result_ok".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            ("result_err", [(payload, shape)]) if shape.is_payload_candidate() => Ok((
                self.push(EmirOp::ResultErr(*payload))?,
                Shape::ResultCarrier,
            )),
            ("result_err", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "result_err".to_string(),
                detail: format!(
                    "result_err requires exactly one Scalar/Vector/Matrix/Option/Result payload, got {shape:?}"
                ),
            }),
            ("result_err", _) => Err(TermCompileError::ArityMismatch {
                symbol: "result_err".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            ("result_is_ok", [(carrier, Shape::ResultCarrier)]) => Ok((
                self.push(EmirOp::ResultIsOk(*carrier))?,
                Shape::Bool,
            )),
            ("result_is_ok", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "result_is_ok".to_string(),
                detail: format!("result_is_ok requires a Result carrier, got {shape:?}"),
            }),
            ("result_is_ok", _) => Err(TermCompileError::ArityMismatch {
                symbol: "result_is_ok".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            ("result_unwrap_or", [(carrier, Shape::ResultCarrier), (default, shape)])
                if shape.is_concrete_payload() || matches!(shape, Shape::ResultCarrier) =>
            {
                Ok((
                    self.push(EmirOp::ResultUnwrapOr(*carrier, *default))?,
                    *shape,
                ))
            }
            ("result_unwrap_or", _) => Err(TermCompileError::ShapeMismatch {
                symbol: "result_unwrap_or".to_string(),
                detail: "result_unwrap_or requires (Result carrier, Scalar/Vector/Matrix/Option/Result default)"
                    .to_string(),
            }),
            ("result_error_of", [(carrier, Shape::ResultCarrier)]) => Ok((
                self.push(EmirOp::ResultErrorOf(*carrier))?,
                Shape::OptionCarrier,
            )),
            ("result_error_of", [(_, shape)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "result_error_of".to_string(),
                detail: format!("result_error_of requires a Result carrier, got {shape:?}"),
            }),
            ("result_error_of", _) => Err(TermCompileError::ArityMismatch {
                symbol: "result_error_of".to_string(),
                expected: 1,
                actual: args.len(),
            }),
            // Field: field_inv(a, p) = a^-1 mod p — the
            // exact modular inverse over the prime field. Operand order
            // mirrors the emitter's `mod_inv` (a, m) surface
            // (crates/emath-exec-ir/src/emitter/call.rs): value first,
            // modulus second. Both operands are scalar integers; the
            // result is a Scalar. field_add/field_mul are NOT registered:
            // no generic modular Add/Mul EmirOp exists in the closed
            // vocabulary (inventory: only ModInv/Congruence/PolyEvalMod/
            // RSEncode) — handoff spec, never a half-wired name.
            ("field_inv", [(a, Shape::Scalar), (p, Shape::Scalar)]) => Ok((
                self.push(EmirOp::ModInv(*a, *p))?,
                Shape::Scalar,
            )),
            ("field_inv", [(_, _), (_, _)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "field_inv".to_string(),
                detail: format!(
                    "field_inv requires exactly two scalar operands (a, p); got {} argument(s)",
                    args.len()
                ),
            }),
            ("field_inv", _) => Err(TermCompileError::ArityMismatch {
                symbol: "field_inv".to_string(),
                expected: 2,
                actual: args.len(),
            }),
            // Universal exact-Euclidean remainder: int_rem(a, m)
            // = a.rem_euclid(m) on i64. Two scalar operands (value first,
            // modulus second, mirroring mod_inv) → Scalar. No field-named op:
            // int_rem is the generic primitive the capability-cell field
            // arithmetic composes.
            ("int_rem", [(a, Shape::Scalar), (m, Shape::Scalar)]) => Ok((
                self.push(EmirOp::IntRem(*a, *m))?,
                Shape::Scalar,
            )),
            ("int_rem", [(_, _), (_, _)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "int_rem".to_string(),
                detail: format!(
                    "int_rem requires exactly two scalar operands (a, m); got {} argument(s)",
                    args.len()
                ),
            }),
            ("int_rem", _) => Err(TermCompileError::ArityMismatch {
                symbol: "int_rem".to_string(),
                expected: 2,
                actual: args.len(),
            }),
            // Tonelli-Shanks modular square root: sqrt_mod(a, p) → the
            // smaller root x with x² ≡ a (mod p) for prime p. Two scalar
            // operands (value first, modulus second, mirroring mod_inv)
            // → Scalar. Non-residues refuse typed — never a fabricated
            // root.
            ("sqrt_mod", [(a, Shape::Scalar), (p, Shape::Scalar)]) => Ok((
                self.push(EmirOp::SqrtMod(*a, *p))?,
                Shape::Scalar,
            )),
            ("sqrt_mod", [(_, _), (_, _)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "sqrt_mod".to_string(),
                detail: format!(
                    "sqrt_mod requires exactly two scalar operands (a, p); got {} argument(s)",
                    args.len()
                ),
            }),
            ("sqrt_mod", _) => Err(TermCompileError::ArityMismatch {
                symbol: "sqrt_mod".to_string(),
                expected: 2,
                actual: args.len(),
            }),
            // Modular exponentiation: pow_mod(base, exp, m) via
            // square-and-multiply over i128 intermediates (i64 operands
            // never overflow). Three scalar operands (value, exponent,
            // modulus — mirroring mod_inv's value-first order) → Scalar.
            // No field-named op: pow_mod is the generic primitive the
            // capability-cell field arithmetic composes.
            ("pow_mod", [(b, Shape::Scalar), (e, Shape::Scalar), (m, Shape::Scalar)]) => Ok((
                self.push(EmirOp::PowMod(*b, *e, *m))?,
                Shape::Scalar,
            )),
            ("pow_mod", [(_, _), (_, _), (_, _)]) => Err(TermCompileError::ShapeMismatch {
                symbol: "pow_mod".to_string(),
                detail: format!(
                    "pow_mod requires exactly three scalar operands (base, exponent, modulus); got {} argument(s)",
                    args.len()
                ),
            }),
            ("pow_mod", _) => Err(TermCompileError::ArityMismatch {
                symbol: "pow_mod".to_string(),
                expected: 3,
                actual: args.len(),
            }),
            (op @ ("sum" | "vmax" | "vmin"), [(source, Shape::Vector)]) => {
                let reduce = match op {
                    "sum" => ReduceId::Sum,
                    "vmax" => ReduceId::Max,
                    _ => ReduceId::Min,
                };
                Ok((
                    self.push(EmirOp::VectorReduce {
                        reduce,
                        source: *source,
                    })?,
                    Shape::Scalar,
                ))
            }
            (op @ ("sum" | "vmax" | "vmin"), _) => Err(TermCompileError::ShapeMismatch {
                symbol: op.to_string(),
                detail: "requires exactly one vector-shaped argument".to_string(),
            }),
            ("neg", [(source, Shape::Scalar)]) => {
                Ok((self.push(EmirOp::Neg(*source))?, Shape::Scalar))
            }
            ("neg", [(source, Shape::Vector)]) => {
                // -(v) == scale(v, -1.0) exactly (sign flip is exact in
                // IEEE-754; no extra rounding is introduced).
                let minus_one = self.push(EmirOp::ConstF64((-1.0_f64).to_bits()))?;
                Ok((
                    self.push(EmirOp::VectorScale(*source, minus_one))?,
                    Shape::Vector,
                ))
            }
            ("vec", list) if list.iter().all(|(_, shape)| *shape == Shape::Scalar) => {
                let elements = list.iter().map(|(value, _)| *value).collect();
                Ok((self.push(EmirOp::VectorCreate(elements))?, Shape::Vector))
            }
            ("vec", _) => Err(TermCompileError::ShapeMismatch {
                symbol: "vec".to_string(),
                detail: "vector literals are built from scalar elements".to_string(),
            }),
            (symbol, _) => Err(TermCompileError::UnknownOperator {
                symbol: symbol.to_string(),
            }),
        }
    }
}
