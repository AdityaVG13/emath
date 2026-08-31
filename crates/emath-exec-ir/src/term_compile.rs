//! Compile cell reference semantics into generic EMIR bytecode (fjxh.5).
//!
//! A pure cell's formula is a quoted [`emath_term::Term`] (the
//! `emath-term` first-order term IR) plus a closed parameter list. This
//! module lowers that term into the SAME generic [`EmirProgram`] the
//! reference VM already executes: elementwise vector math through the
//! closed [`BuiltinId`] registry, broadcast arithmetic through a closed
//! four-op set, aggregation through a closed reduce set. No per-op Rust
//! function is required in the VM seam for a pure cell: adding a cell is
//! one registry ENTRY (data), never a new op variant or dispatch arm.
//!
//! Zero core delta: `emath-ir` op/expr enums do not grow; the new EMIR
//! ops (`vector-map`, `vector-map-scalar`, `vector-reduce`,
//! `vector-all-finite`) are generic vocabulary in the exec-ir VM layer.
//! The strict vs Genesis/custom firewall is preserved: cells dispatch
//! from data at the seam, never from domain-named branches.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use emath_core::Span;
use emath_term::{Signature, SymbolId, Term, TermError, VariableId};

use crate::{
    BuiltinId, EmirOp, EmirProgram, EmirValue, ProbKind, ReduceId, VectorScalarOp, optimize,
};

/// Declared shape of one cell parameter. Closed set; matrix/tensor
/// parameter shapes are later spine work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamShape {
    /// IEEE-754 binary64 scalar.
    Scalar,
    /// Rank-1 Float64 vector.
    Vector,
    /// Dense row-major Float64 matrix (the graph/linear-algebra
    /// carrier; r2-graphs-masa slice 2 opened the call surface).
    Matrix,
}

impl ParamShape {
    /// Stable token for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Vector => "vector",
            Self::Matrix => "matrix",
        }
    }
}

/// Inferred element shape of a compiled subterm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Scalar,
    Vector,
    /// Dense matrix carrier (`Matrix<Float64>` params and the graph
    /// call slots; r2-graphs-masa slice 2).
    Matrix,
    /// Comparison results (`lt`/`le`/`gt`/`ge`/`eq`/`ne`): the closed
    /// vocabulary composes booleans NOWHERE (every other arm matches
    /// Scalar/Vector only).
    Bool,
    /// An Option carrier (`option_some`/`option_none`). Carriers are
    /// OPAQUE at the shape level: the inner payload shape is not
    /// tracked — only the polarity/unwrap/error_of arms admit them
    /// (aj8d call surface).
    OptionCarrier,
    /// A Result carrier (`result_ok`/`result_err`). Same opacity law as
    /// the Option carrier; the error payload shares the Result slot
    /// (`Value::Result { ok, payload }`, aj8d).
    ResultCarrier,
}

impl Shape {
    /// The concrete payload/default shapes the Option/Result call
    /// surface admits: Scalar, Vector, Matrix. Carriers refuse (they
    /// are opaque) and Bool refuses (the closed vocabulary composes
    /// booleans NOWHERE).
    const fn is_concrete_payload(self) -> bool {
        matches!(self, Self::Scalar | Self::Vector | Self::Matrix)
    }

    /// True for a (nested) carrier shape. Nested payloads are the
    /// type-honest rule lifted in emath-option-result-graph-field-aj8d
    /// pass 3: `Option<T>` and `Result<T,E>` are themselves payloads, so
    /// a carrier is an acceptable payload for the three CONSTRUCTORS
    /// (`option_some`/`result_ok`/`result_err`) and an acceptable
    /// unwrap_or DEFAULT when the retrieved payload is a carrier. Bool
    /// still composes nowhere (not a carrier, not concrete).
    const fn is_payload_candidate(self) -> bool {
        self.is_concrete_payload()
            || matches!(self, Self::OptionCarrier | Self::ResultCarrier)
    }
}

/// Data-driven contract guard run at the VM seam BEFORE the compiled
/// body, in declared order. Guards are cell data, not VM branches: a
/// violation is the capability layer's typed refusal (`E-CELL-006`),
/// never a silent value and never a partial authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgGuard {
    /// The argument must be a non-empty vector (an empty input means no
    /// numeric policy was declared for the normalization).
    NonEmpty(usize),
    /// Every element of the argument vector must be finite (the
    /// strict-f64 finite policy refuses non-finite inputs).
    AllFinite(usize),
}

/// Compile-time refusal of a quoted reference term. Closed set: every
/// refusal names what was wrong; nothing is silently lowered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermCompileError {
    /// A symbol was used without an arity declaration (from the
    /// emath-term signature check).
    UnknownSymbol {
        /// The undeclared symbol.
        symbol: String,
    },
    /// A symbol application used the wrong number of arguments.
    ArityMismatch {
        /// Symbol identity.
        symbol: String,
        /// Declared arity.
        expected: usize,
        /// Observed arity.
        actual: usize,
    },
    /// One symbol was declared with conflicting arities.
    ConflictingArity {
        /// Symbol identity.
        symbol: String,
        /// Earlier declaration.
        first: usize,
        /// Conflicting declaration.
        second: usize,
    },
    /// An operator outside the closed generic vocabulary (no per-op Rust
    /// function is minted on the fly — that is the bead's law).
    UnknownOperator {
        /// The out-of-vocabulary operator.
        symbol: String,
    },
    /// A free variable outside the declared parameter list.
    UnknownVariable {
        /// The undeclared variable name.
        name: String,
    },
    /// Operand shapes do not fit the closed vocabulary (e.g. reduce over
    /// a scalar, vector-vector elementwise divide, scalar-first broadcast).
    ShapeMismatch {
        /// The operator that refused.
        symbol: String,
        /// Why the shape combination is not admitted.
        detail: String,
    },
    /// A constant symbol that does not parse as an f64 literal.
    BadLiteral {
        /// The non-numeric constant text.
        text: String,
    },
    /// A malformed cell contract (guard index outside params, param
    /// count overflow).
    MalformedContract {
        /// Why the contract is malformed.
        detail: String,
    },
}

impl TermCompileError {
    /// The operator or symbol the refusal names (empty for contract
    /// errors).
    #[must_use]
    pub fn symbol(&self) -> &str {
        match self {
            Self::UnknownSymbol { symbol }
            | Self::ArityMismatch { symbol, .. }
            | Self::ConflictingArity { symbol, .. }
            | Self::UnknownOperator { symbol }
            | Self::ShapeMismatch { symbol, .. }
            | Self::BadLiteral { text: symbol } => symbol,
            Self::UnknownVariable { name } => name,
            Self::MalformedContract { .. } => "",
        }
    }
}

impl fmt::Display for TermCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSymbol { symbol } => {
                write!(
                    formatter,
                    "reference term uses undeclared symbol `{symbol}`"
                )
            }
            Self::ArityMismatch {
                symbol,
                expected,
                actual,
            } => write!(
                formatter,
                "reference operator `{symbol}` applied to {actual} argument(s), \
                 signature declares {expected}"
            ),
            Self::ConflictingArity {
                symbol,
                first,
                second,
            } => write!(
                formatter,
                "reference symbol `{symbol}` declared with conflicting arities \
                 {first} and {second}"
            ),
            Self::UnknownOperator { symbol } => write!(
                formatter,
                "reference operator `{symbol}` is outside the closed generic \
                 vocabulary; pure cells compile from data, not per-op VM branches"
            ),
            Self::UnknownVariable { name } => write!(
                formatter,
                "reference term uses variable `{name}` outside the declared \
                 parameter list"
            ),
            Self::ShapeMismatch { symbol, detail } => {
                write!(formatter, "reference operator `{symbol}`: {detail}")
            }
            Self::BadLiteral { text } => {
                write!(
                    formatter,
                    "reference constant `{text}` is not an f64 literal"
                )
            }
            Self::MalformedContract { detail } => {
                write!(formatter, "malformed cell reference contract: {detail}")
            }
        }
    }
}

impl std::error::Error for TermCompileError {}

/// Run declared contract guards over the argument values (one shared
/// implementation for the VM seam and the specializer's residual entry).
/// A violation is the capability layer's typed refusal (`E-CELL-006`);
/// a non-vector argument is a typed confusion — never a coercion.
pub(crate) fn run_guards(
    capability: &str,
    guards: &[ArgGuard],
    args: &[crate::interp::Value],
) -> Result<(), crate::interp::EvalFault> {
    use crate::interp::{EvalFault, Value};
    for guard in guards {
        let index = match guard {
            ArgGuard::NonEmpty(index) | ArgGuard::AllFinite(index) => *index,
        };
        let Some(value) = args.get(index) else {
            return Err(EvalFault::TypeConfusion {
                register: index as u32,
                op: "apply-capability",
            });
        };
        let elements: &[f64] = match value {
            Value::Vector(elements) => elements,
            // Matrix carriers (r2-graphs-masa slice 2): the guard
            // semantics ("every entry finite" / "non-empty") apply to
            // the flat storage — the adjacency carrier is guarded like
            // any numeric argument.
            Value::Matrix { data, .. } => data,
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: index as u32,
                    op: "apply-capability",
                });
            }
        };
        let violated = match guard {
            ArgGuard::NonEmpty(_) => elements.is_empty(),
            ArgGuard::AllFinite(_) => elements.iter().any(|x| !x.is_finite()),
        };
        if violated {
            return Err(EvalFault::CapabilityRefused {
                capability: capability.to_string(),
                code: "E-CELL-006".to_string(),
            });
        }
    }
    Ok(())
}

/// Post-body certificate predicate (cell DATA, rymw first proof). A
/// cell whose contract is an EXACT zero certificate (mass balance,
/// conservation residual, boundary residual) declares the refusal code
/// for a nonzero program result here; the interpreter enforces it with
/// no domain branch. This is the generic primitive the mass-balance
/// proof needs: cells refuse on COMPUTED outcomes, not just on inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultGuard {
    /// Refuse typed whenever the program result is a vector with a
    /// nonzero entry: the refusal names the first violating index and
    /// its exact residual.
    AllZero {
        /// The typed refusal code (e.g. `MassImbalance`).
        code: &'static str,
    },
}

/// A compiled pure cell: declared params, data-driven guards, the
/// generic bytecode program, and an optional post-body zero certificate
/// guard. The capability name is carried for registry lookup and
/// diagnostics.
#[derive(Clone, Debug)]
pub struct CompiledCell {
    /// Canonical capability path (`std.tensor.softmax`).
    pub capability: String,
    /// Declared parameters, in argument order.
    pub params: Vec<(String, ParamShape)>,
    /// Contract guards run at the seam before the body, in order.
    pub guards: Vec<ArgGuard>,
    /// Optional post-body certificate: refuses typed when the result
    /// vector violates the declared predicate.
    pub result_guard: Option<ResultGuard>,
    /// Generic bytecode the reference VM executes.
    pub program: EmirProgram,
}

struct Compiler {
    ops: Vec<(EmirOp, Span)>,
    params: Vec<(String, ParamShape)>,
}

impl Compiler {
    fn push(&mut self, op: EmirOp) -> Result<EmirValue, TermCompileError> {
        let id =
            u32::try_from(self.ops.len()).map_err(|_| TermCompileError::MalformedContract {
                detail: "reference program exceeds u32::MAX ops".to_string(),
            })?;
        self.ops.push((op, Span::default()));
        Ok(EmirValue(id))
    }

    fn compile_term(&mut self, term: &Term) -> Result<(EmirValue, Shape), TermCompileError> {
        match term {
            Term::Variable(variable) => {
                let position = self
                    .params
                    .iter()
                    .position(|(name, _)| name == &variable.0)
                    .ok_or_else(|| TermCompileError::UnknownVariable {
                        name: variable.0.clone(),
                    })?;
                let index =
                    u16::try_from(position).map_err(|_| TermCompileError::MalformedContract {
                        detail: "param count exceeds u16::MAX".to_string(),
                    })?;
                let shape = match self.params[position].1 {
                    ParamShape::Scalar => Shape::Scalar,
                    ParamShape::Vector => Shape::Vector,
                    ParamShape::Matrix => Shape::Matrix,
                };
                let value = self.push(EmirOp::LoadInput(index))?;
                Ok((value, shape))
            }
            Term::Constant(symbol) => {
                let value: f64 =
                    symbol
                        .0
                        .trim()
                        .parse()
                        .map_err(|_| TermCompileError::BadLiteral {
                            text: symbol.0.clone(),
                        })?;
                let register = self.push(EmirOp::ConstF64(value.to_bits()))?;
                Ok((register, Shape::Scalar))
            }
            Term::Apply {
                operator,
                arguments,
            } => {
                let mut compiled = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    compiled.push(self.compile_term(argument)?);
                }
                self.compile_call(&operator.0, &compiled)
            }
        }
    }

    fn compile_call(
        &mut self,
        name: &str,
        args: &[(EmirValue, Shape)],
    ) -> Result<(EmirValue, Shape), TermCompileError> {
        // Closed comparison vocabulary (fjxh.14 cohort): scalar/scalar →
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
            // Linear-algebra names (4wj0, B35): the registry path binds
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
            // Dense matrix×vector (rymw chemistry proof): the registry
            // name binds the SAME generic op the emitter path lowers —
            // zero new op variants, zero per-op VM code (the 4wj0/B35
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
            // Exact integer null vector (rymw): matrix → primitive
            // vector; a name binding on the generic IntNullspace op.
            ("int_nullspace", [(matrix, Shape::Matrix)]) => Ok((
                self.push(EmirOp::IntNullspace(*matrix))?,
                Shape::Vector,
            )),
            ("int_nullspace", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "int_nullspace requires exactly one matrix".to_string(),
            }),
            // Exact integer product difference (rymw thermo): the
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
            // Graph algorithms (r2-graphs-masa slice 2): the call
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
            // Spectral basics (r2-graphs-masa slice 3): the unnormalized
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
            // Symmetrized adjacency (masa slice 4): matrix in, matrix
            // out; a scalar adjacency refuses at COMPILE.
            ("graph_symmetrize", [(adj, Shape::Matrix)]) => {
                Ok((self.push(EmirOp::GraphSymmetrize(*adj))?, Shape::Matrix))
            }
            ("graph_symmetrize", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "graph_symmetrize requires exactly one matrix adjacency carrier"
                    .to_string(),
            }),
            // Negative-edge shortest paths (masa slice 5): (matrix,
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
            // Sparse storage (masa slice 6): extraction is matrix →
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
            // Optimization (r3-lp-milp-wlif slice 1): the standard-form
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
            // Polynomials as values (r3-funcspaces-poly-hjor slice 1):
            // dense ascending coefficient vectors. Addition is the
            // EXISTING generic vector add (a name binding, the 4wj0
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
            // Spectral Poisson (xx0x.4 thin nucleus): vector load in,
            // vector field out; a scalar load refuses at COMPILE.
            ("poisson_sine", [(f, Shape::Vector)]) => {
                Ok((self.push(EmirOp::PoissonDirichletSine(*f))?, Shape::Vector))
            }
            ("poisson_sine", _) => Err(TermCompileError::ShapeMismatch {
                symbol: name.to_string(),
                detail: "poisson_sine requires a vector-shaped interior load".to_string(),
            }),
            // Control surface (zxkl thin B43): transfer eval is
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
            // Finite-category surface (88wo thin B39): the law gate is
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
            // Probability nucleus (xx0x.5): seeded sampling + exact
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
            // ── Option/Result call surface (aj8d) ──────────────────
            // Nine names binding the TOTAL value-semantics ops the
            // interp already executes (Some/None/Ok/Err constructors,
            // is_some/is_ok polarity, the unwrap_or honesty gate — a
            // missing value yields the caller's eagerly-evaluated
            // default, NO panicking unwrap exists at this layer — and
            // error_of, the Result error composed AS an Option).
            // Nested payloads (pass 3) are the type-honest rule: a
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
            // Field (aj8d pass 7): field_inv(a, p) = a^-1 mod p — the
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
            // Universal exact-Euclidean remainder (aj8d pass 6): int_rem(a, m)
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

    fn compile_arith(
        &mut self,
        op: &str,
        a: (EmirValue, Shape),
        b: (EmirValue, Shape),
    ) -> Result<(EmirValue, Shape), TermCompileError> {
        let mismatch = |detail: String| TermCompileError::ShapeMismatch {
            symbol: op.to_string(),
            detail,
        };
        match (op, a.1, b.1) {
            ("add", Shape::Scalar, Shape::Scalar) => {
                Ok((self.push(EmirOp::F64Add(a.0, b.0))?, Shape::Scalar))
            }
            ("sub", Shape::Scalar, Shape::Scalar) => {
                Ok((self.push(EmirOp::F64Sub(a.0, b.0))?, Shape::Scalar))
            }
            ("mul", Shape::Scalar, Shape::Scalar) => {
                Ok((self.push(EmirOp::F64Mul(a.0, b.0))?, Shape::Scalar))
            }
            ("div", Shape::Scalar, Shape::Scalar) => {
                Ok((self.push(EmirOp::F64Div(a.0, b.0))?, Shape::Scalar))
            }
            ("add", Shape::Vector, Shape::Vector) => {
                Ok((self.push(EmirOp::VectorAdd(a.0, b.0))?, Shape::Vector))
            }
            ("sub", Shape::Vector, Shape::Vector) => {
                Ok((self.push(EmirOp::VectorSub(a.0, b.0))?, Shape::Vector))
            }
            ("mul", Shape::Vector, Shape::Scalar) => {
                Ok((self.push(EmirOp::VectorScale(a.0, b.0))?, Shape::Vector))
            }
            ("add", Shape::Vector, Shape::Scalar) => Ok((
                self.push(EmirOp::VectorMapScalar {
                    op: VectorScalarOp::Add,
                    vector: a.0,
                    scalar: b.0,
                })?,
                Shape::Vector,
            )),
            ("sub", Shape::Vector, Shape::Scalar) => Ok((
                self.push(EmirOp::VectorMapScalar {
                    op: VectorScalarOp::Sub,
                    vector: a.0,
                    scalar: b.0,
                })?,
                Shape::Vector,
            )),
            ("div", Shape::Vector, Shape::Scalar) => Ok((
                self.push(EmirOp::VectorMapScalar {
                    op: VectorScalarOp::Div,
                    vector: a.0,
                    scalar: b.0,
                })?,
                Shape::Vector,
            )),
            (_, Shape::Scalar, Shape::Vector) => Err(mismatch(
                "canonical broadcast order is (vector, scalar); write the \
                 vector operand first"
                    .to_string(),
            )),
            (_, Shape::Vector, Shape::Vector) => Err(mismatch(
                "elementwise vector-vector multiply/divide is not in the \
                 closed reference vocabulary"
                    .to_string(),
            )),
            (_, _, _) => Err(mismatch(
                "unsupported operand shapes for strict arithmetic".to_string(),
            )),
        }
    }
}

/// Compile a quoted cell reference term into generic bytecode.
///
/// `signature` is checked with emath-term's own validator first, then
/// every operator is mapped onto the closed generic vocabulary (strict
/// arithmetic, the builtin registry, the closed vector map/reduce set).
/// The compiled program is optimized with the same passes as any other
/// EMIR program. A pure cell needs NO per-op Rust function in the VM
/// seam: the registry below is data.
pub fn compile_reference(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    compile_reference_inner(term, signature, params, guards, None, capability)
}

/// [`compile_reference`] plus a post-body zero-certificate guard: the
/// compiled cell refuses typed with the guard's code when its result
/// vector has a nonzero entry. Cell DATA — the seam enforces it
/// generically, no domain branch.
pub fn compile_reference_guarded(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    result_guard: ResultGuard,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    compile_reference_inner(term, signature, params, guards, Some(result_guard), capability)
}

fn compile_reference_inner(
    term: &Term,
    signature: &Signature,
    params: &[(String, ParamShape)],
    guards: Vec<ArgGuard>,
    result_guard: Option<ResultGuard>,
    capability: &str,
) -> Result<CompiledCell, TermCompileError> {
    // emath-term's structural validation (unknown symbols, arity).
    signature.validate(term).map_err(|error| match error {
        TermError::UnknownSymbol(symbol) => TermCompileError::UnknownSymbol { symbol: symbol.0 },
        TermError::ArityMismatch {
            symbol,
            expected,
            actual,
        } => TermCompileError::ArityMismatch {
            symbol: symbol.0,
            expected,
            actual,
        },
        TermError::ConflictingArity {
            symbol,
            first,
            second,
        } => TermCompileError::ConflictingArity {
            symbol: symbol.0,
            first,
            second,
        },
    })?;
    // Guards must reference declared arguments.
    for guard in &guards {
        let index = match guard {
            ArgGuard::NonEmpty(index) | ArgGuard::AllFinite(index) => *index,
        };
        if index >= params.len() {
            return Err(TermCompileError::MalformedContract {
                detail: format!(
                    "guard references argument {index} outside the {} declared param(s)",
                    params.len()
                ),
            });
        }
    }
    let input_count =
        u16::try_from(params.len()).map_err(|_| TermCompileError::MalformedContract {
            detail: "param count exceeds u16::MAX".to_string(),
        })?;
    let mut compiler = Compiler {
        ops: Vec::new(),
        params: params.to_vec(),
    };
    let (result, _shape) = compiler.compile_term(term)?;
    let mut program = EmirProgram {
        ops: compiler.ops,
        result,
        input_count,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    optimize::optimize_program(&mut program);
    Ok(CompiledCell {
        capability: capability.to_string(),
        params: params.to_vec(),
        guards,
        result_guard,
        program,
    })
}

/// The `std.tensor.softmax` reference formula of record, as a quoted
/// term: `exp(sub(x, vmax(x)))` normalized by `sum(exp(sub(x, vmax(x))))`
/// — the stable-max form (shift invariance is the cell's declared law,
/// and the shift keeps strict-f64 exp finite for large logits).
fn softmax_reference_term() -> (Term, Signature) {
    let x = || Term::Variable(VariableId("x".into()));
    let shifted = || Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![
            x(),
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![x()],
            },
        ],
    };
    let exps = || Term::Apply {
        operator: SymbolId("exp".into()),
        arguments: vec![shifted()],
    };
    let term = Term::Apply {
        operator: SymbolId("div".into()),
        arguments: vec![
            exps(),
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![exps()],
            },
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("exp", 1usize),
        ("sub", 2),
        ("div", 2),
        ("sum", 1),
        ("vmax", 1),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("softmax formula signature is conflict-free");
    }
    (term, signature)
}

fn compile_std_softmax() -> Result<CompiledCell, TermCompileError> {
    let (term, signature) = softmax_reference_term();
    compile_reference(
        &term,
        &signature,
        &[("x".to_string(), ParamShape::Vector)],
        vec![ArgGuard::NonEmpty(0), ArgGuard::AllFinite(0)],
        "std.tensor.softmax",
    )
}

/// A single-argument scalar cell term: `<op>(x)` (the cohort's unary
/// shapes use the same closed vocabulary).
fn scalar_unary_term(op: &str) -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(op.into()), 1)
        .expect("single-op signature is conflict-free");
    let term = Term::Apply {
        operator: SymbolId(op.into()),
        arguments: vec![Term::Variable(VariableId("x".into()))],
    };
    (term, signature)
}

/// A two-argument scalar cell term: `<op>(x, y)`.
fn scalar_binary_term(op: &str) -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(op.into()), 2)
        .expect("two-arg signature is conflict-free");
    let term = Term::Apply {
        operator: SymbolId(op.into()),
        arguments: vec![
            Term::Variable(VariableId("x".into())),
            Term::Variable(VariableId("y".into())),
        ],
    };
    (term, signature)
}

/// Compiled std-cell registry: cells ship as DATA (quoted reference term
/// + guards), compiled once to generic bytecode. Adding a pure cell is
/// one registry entry — the VM seam and the op set never grow per-op.
pub fn std_cell_registry() -> &'static HashMap<String, CompiledCell> {
    static REGISTRY: OnceLock<HashMap<String, CompiledCell>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut map = HashMap::new();
        // The fjxh.14 cohort (7 migrated ops + softmax). Each entry is
        // standalone data (independent rollback): quoted term + guards,
        // compiled by the SAME closed vocabulary — zero per-op VM code.
        // Scalar ops declare the unguarded-scalar policy (NaN propagates
        // — the declared strict-f64 behavior for unguarded scalars).
        let mut insert = |cell: CompiledCell| {
            map.insert(cell.capability.clone(), cell);
        };
        for (name, op) in [
            ("std.math.sin", "sin"),
            ("std.math.exp", "exp"),
            ("std.math.sqrt", "sqrt"),
        ] {
            let (term, signature) = scalar_unary_term(op);
            match compile_reference(
                &term,
                &signature,
                &[("x".to_string(), ParamShape::Scalar)],
                Vec::new(),
                name,
            ) {
                Ok(cell) => insert(cell),
                Err(error) => panic!("std scalar cell failed to compile: {error}"),
            }
        }
        // Chemistry cohort (r3-chem-bio-stdlib): the Boltzmann softmax
        // reference term as `std.chem.softmax` cell data — a pure vector
        // cell (shift-invariant exp-normalize), compiled through the SAME
        // closed vocabulary, zero per-op VM code.
        let (term, signature) = softmax_reference_term();
        match compile_reference(
            &term,
            &signature,
            &[("x".to_string(), ParamShape::Vector)],
            vec![ArgGuard::NonEmpty(0), ArgGuard::AllFinite(0)],
            "std.chem.softmax",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.softmax reference failed to compile: {error}"),
        }
        // Chemistry lane (sci-chemistry-rymw first proof): the
        // stoichiometric mass-balance cell is registry DATA over the
        // EXISTING dense matrix×vector op. `matvec(S, s)` is the
        // per-element residual S·s (S = signed composition matrix,
        // elements × species; s = signed coefficients, reactants
        // positive). The zero-certificate result guard refuses typed
        // `MassImbalance` when any residual is nonzero. f64 represents
        // small-integer stoichiometry EXACTLY, so an all-zero residual
        // is an exact mass-balance certificate, not a tolerance.
        let (term, signature) = mass_balance_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("S".to_string(), ParamShape::Matrix),
                ("s".to_string(), ParamShape::Vector),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
            ResultGuard::AllZero {
                code: "MassImbalance",
            },
            "std.chem.mass_balance",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.mass_balance reference failed to compile: {error}"),
        }
        // Chemistry balancing (rymw second proof): derive the canonical
        // primitive coefficient vector from the sign-blind species
        // composition matrix, as registry DATA over the generic
        // `int_nullspace` op. No domain/logic code lives in the seam —
        // the op is the generic exact-integer primitive. (No result
        // guard: the coefficient vector is legitimately nonzero; the
        // mass-balance cell certifies it.)
        let (term, signature) = balance_reference_term();
        match compile_reference(
            &term,
            &signature,
            &[("S".to_string(), ParamShape::Matrix)],
            vec![ArgGuard::AllFinite(0)],
            "std.chem.balance",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.balance reference failed to compile: {error}"),
        }
        // Molecular-graph rewrite checker (rymw molecular-graph slice):
        // valence preservation across the (L, K, R) span as registry
        // DATA over generic ops, with the scalar-capable AllZero
        // certificate guard.
        let (term, signature) = rewrite_preserve_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("L".to_string(), ParamShape::Matrix),
                ("K".to_string(), ParamShape::Matrix),
                ("R".to_string(), ParamShape::Matrix),
                ("u".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
                ArgGuard::AllFinite(3),
            ],
            ResultGuard::AllZero {
                code: "ValenceImbalance",
            },
            "std.chem.graph_rewrite_preserve",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.graph_rewrite_preserve failed to compile: {error}"),
        }
        // Thermo-equilibrium (rymw thermo slice): Wegscheider cycle
        // consistency as registry DATA over the generic exact product
        // delta op, with the AllZero scalar certificate guard.
        let (term, signature) = cycle_consistent_reference_term();
        match compile_reference_guarded(
            &term,
            &signature,
            &[
                ("P".to_string(), ParamShape::Vector),
                ("Q".to_string(), ParamShape::Vector),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
            ResultGuard::AllZero {
                code: "CycleInconsistency",
            },
            "std.chem.cycle_consistent",
        ) {
            Ok(cell) => insert(cell),
            Err(error) => panic!("std.chem.cycle_consistent failed to compile: {error}"),
        }
        for (name, op) in [
            ("std.math.add", "add"),
            ("std.math.mul", "mul"),
            ("std.math.lt", "lt"),
        ] {
            let (term, signature) = scalar_binary_term(op);
            match compile_reference(
                &term,
                &signature,
                &[
                    ("x".to_string(), ParamShape::Scalar),
                    ("y".to_string(), ParamShape::Scalar),
                ],
                Vec::new(),
                name,
            ) {
                Ok(cell) => insert(cell),
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // The sum reduction (vector → scalar, guarded finite policy).
        match scalar_unary_sum() {
            Ok(cell) => {
                map.insert(cell.capability.clone(), cell);
            }
            Err(error) => panic!("std.tensor.sum reference failed to compile: {error}"),
        }
        // The linear-algebra norm family + inner product (4wj0, B35):
        // registry DATA over the closed vector vocabulary the interp
        // already executes — L2 is the generic VectorNorm op; L1/Linf
        // compose the abs map with the sum/max reduces; the inner
        // product is the generic dot. Zero per-op VM code.
        for (name, term, signature, params, guards) in linear_algebra_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // Graph algorithms (r2-graphs-masa slice 2): registry DATA over
        // the slice-1 EMIR ops, Matrix-typed params, all-finite weight
        // guard (E-GRAPH-004 at the seam).
        for (name, term, signature, params, guards) in graph_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        // Geometry primitives (emath-talo): registry DATA over the
        // closed vector vocabulary the interp already executes — cross
        // via bit-exact dot-with-basis component extraction, normalize
        // as the generic vector-scalar divide, distance as
        // norm(a-b). Zero per-op VM code, no geometry kernel.
        for (name, term, signature, params, guards) in geometry_cells() {
            match compile_reference(&term, &signature, &params, guards, name) {
                Ok(cell) => {
                    map.insert(cell.capability.clone(), cell);
                }
                Err(error) => panic!("{name} reference failed to compile: {error}"),
            }
        }
        match compile_std_softmax() {
            Ok(cell) => {
                map.insert(cell.capability.clone(), cell);
            }
            // The std formula is statically validated against its
            // signature; a failure here is a build-time contract break,
            // not a runtime condition.
            Err(error) => panic!("std.tensor.softmax reference failed to compile: {error}"),
        }
        map
    })
}

/// The geometry primitive cells (emath-talo): `std.geometry.cross` /
/// `std.geometry.normalize` / `std.geometry.distance` as registry DATA
/// over the SAME closed vector vocabulary — no geometry kernel, no new
/// op, no index operator. Component extraction inside `cross` is
/// bit-exact dot-with-basis: `dot(u, e_i) == u[i]` exactly for finite
/// inputs (`x·1 = x`, `x·0 = ±0`, and `x + ±0 = x`), so the compiled
/// formula is the textbook cross product over the extracted components.
/// Zero per-op VM code.
fn geometry_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let two_vector_params = || {
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ]
    };
    let a_b_vector_params = || {
        vec![
            ("a".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
        ]
    };
    let one_vector_param = || vec![("v".to_string(), ParamShape::Vector)];
    let guarded = |count: usize| {
        (0..count)
            .flat_map(|index| [ArgGuard::NonEmpty(index), ArgGuard::AllFinite(index)])
            .collect::<Vec<_>>()
    };
    let axis = |x: &str, y: &str, z: &str| Term::Apply {
        operator: SymbolId("vec".into()),
        arguments: vec![
            Term::Constant(SymbolId(x.into())),
            Term::Constant(SymbolId(y.into())),
            Term::Constant(SymbolId(z.into())),
        ],
    };
    let e1 = || axis("1.0", "0.0", "0.0");
    let e2 = || axis("0.0", "1.0", "0.0");
    let e3 = || axis("0.0", "0.0", "1.0");
    let dot = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("dot".into()),
        arguments: vec![a, b],
    };
    let mul = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("mul".into()),
        arguments: vec![a, b],
    };
    let sub = |a: Term, b: Term| Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![a, b],
    };
    let u = || Term::Variable(VariableId("u".into()));
    let v = || Term::Variable(VariableId("v".into()));

    // cross(u, v): the three components assembled from bit-exact
    // basis-dot extractions; right-handed orientation is the term's
    // data (the permutation laws in tests/emath-sema/tests/geometry3d.rs
    // discriminate it).
    let cross = {
        let mut signature = Signature::default();
        // The basis-vector coordinates are nullary constant symbols and
        // must be declared like any other symbol (arity 0).
        for (symbol, arity) in [
            ("vec", 3usize),
            ("dot", 2),
            ("mul", 2),
            ("sub", 2),
            ("0.0", 0),
            ("1.0", 0),
        ] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("cross signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("vec".into()),
                arguments: vec![
                    sub(
                        mul(dot(u(), e2()), dot(v(), e3())),
                        mul(dot(u(), e3()), dot(v(), e2())),
                    ),
                    sub(
                        mul(dot(u(), e3()), dot(v(), e1())),
                        mul(dot(u(), e1()), dot(v(), e3())),
                    ),
                    sub(
                        mul(dot(u(), e1()), dot(v(), e2())),
                        mul(dot(u(), e2()), dot(v(), e1())),
                    ),
                ],
            },
            signature,
            two_vector_params(),
            guarded(2),
        )
    };
    // normalize(v): v / norm(v) — the generic vector-scalar divide.
    // A zero-norm input divides by zero: IEEE gives NaN/Inf under the
    // declared strict-f64 unguarded policy (the geometric no-claim —
    // never a synthesized unit vector).
    let normalize = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("div", 2usize), ("norm", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("normalize signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("div".into()),
                arguments: vec![
                    v(),
                    Term::Apply {
                        operator: SymbolId("norm".into()),
                        arguments: vec![v()],
                    },
                ],
            },
            signature,
            one_vector_param(),
            guarded(1),
        )
    };
    // distance(a, b): norm(a - b) — the generic vector subtract + norm.
    let distance = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("norm", 1usize), ("sub", 2)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("distance signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("norm".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("sub".into()),
                    arguments: vec![
                        Term::Variable(VariableId("a".into())),
                        Term::Variable(VariableId("b".into())),
                    ],
                }],
            },
            signature,
            a_b_vector_params(),
            guarded(2),
        )
    };
    vec![
        ("std.geometry.cross", cross.0, cross.1, cross.2, cross.3),
        (
            "std.geometry.normalize",
            normalize.0,
            normalize.1,
            normalize.2,
            normalize.3,
        ),
        (
            "std.geometry.distance",
            distance.0,
            distance.1,
            distance.2,
            distance.3,
        ),
    ]
}

/// The chemistry mass-balance reference term: `matvec(S, s)` — the
/// per-element residual of the signed stoichiometric system (rymw).
/// The chemistry balancing reference term: `int_nullspace(S)` — the
/// canonical primitive coefficient vector of the sign-blind species
/// composition matrix (rymw second proof).
fn balance_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("int_nullspace".into()), 1)
        .expect("int_nullspace signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("int_nullspace".into()),
            arguments: vec![Term::Variable(VariableId("S".into()))],
        },
        signature,
    )
}

/// The chemistry rewrite-preservation reference term (molecular-graph
/// slice): the valence certificate
/// `sum(abs(matvec(L,u)-matvec(K,u))) + sum(abs(matvec(K,u)-matvec(R,u)))`
/// over a rule triple (L, K, R) of context-row × union-column matrices
/// with `u` the all-ones vector (row sums = bond-order valences). The
/// guard refuses typed `ValenceImbalance` when the certificate is
/// nonzero. Pure registry data over generic ops; no domain code.
/// The cycle-consistency reference term (thermo slice): the exact
/// rational product difference `exact_product_delta(P, Q)` — the
/// Wegscheider certificate `∏P − ∏Q` — guarded AllZero with the
/// `CycleInconsistency` typed refusal.
fn cycle_consistent_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("exact_product_delta".into()), 2)
        .expect("exact_product_delta signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("exact_product_delta".into()),
            arguments: vec![
                Term::Variable(VariableId("P".into())),
                Term::Variable(VariableId("Q".into())),
            ],
        },
        signature,
    )
}

fn rewrite_preserve_reference_term() -> (Term, Signature) {    let violation = |a: &str, b: &str| {
        Term::Apply {
            operator: SymbolId("sum".into()),
            arguments: vec![Term::Apply {
                operator: SymbolId("abs".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("sub".into()),
                    arguments: vec![
                        Term::Apply {
                            operator: SymbolId("matvec".into()),
                            arguments: vec![
                                Term::Variable(VariableId(a.into())),
                                Term::Variable(VariableId("u".into())),
                            ],
                        },
                        Term::Apply {
                            operator: SymbolId("matvec".into()),
                            arguments: vec![
                                Term::Variable(VariableId(b.into())),
                                Term::Variable(VariableId("u".into())),
                            ],
                        },
                    ],
                }],
            }],
        }
    };
    let term = Term::Apply {
        operator: SymbolId("add".into()),
        arguments: vec![violation("L", "K"), violation("K", "R")],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("matvec", 2usize),
        ("sub", 2),
        ("abs", 1),
        ("sum", 1),
        ("add", 2),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("rewrite preserve signature is conflict-free");
    }
    (term, signature)
}

/// The chemistry mass-balance reference term: `matvec(S, s)` — the
/// per-element residual of the signed stoichiometric system (rymw).
fn mass_balance_reference_term() -> (Term, Signature) {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("matvec".into()), 2)
        .expect("matvec signature is conflict-free");
    (
        Term::Apply {
            operator: SymbolId("matvec".into()),
            arguments: vec![
                Term::Variable(VariableId("S".into())),
                Term::Variable(VariableId("s".into())),
            ],
        },
        signature,
    )
}

/// The `std.tensor.sum` reference: `sum(x)` over the declared vector,
/// guarded AllFinite (the finite policy is the reduction's contract).
fn scalar_unary_sum() -> Result<CompiledCell, TermCompileError> {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("sum".into()), 1)
        .expect("sum signature is conflict-free");
    compile_reference(
        &Term::Apply {
            operator: SymbolId("sum".into()),
            arguments: vec![Term::Variable(VariableId("x".into()))],
        },
        &signature,
        &[("x".to_string(), ParamShape::Vector)],
        vec![ArgGuard::AllFinite(0)],
        "std.tensor.sum",
    )
}

/// The linear-algebra registry cells (4wj0, B35) as quoted terms:
/// `(name, term, signature, params, guards)` tuples over the closed
/// vocabulary. L2 norm is the generic `norm` name; L1 composes the abs
/// map with the sum reduce; Linf composes abs with the vmax reduce; the
/// inner product is the generic dot. All are guarded AllFinite (the
/// vector contract).
fn linear_algebra_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let vector_param = || vec![("v".to_string(), ParamShape::Vector)];
    let two_vector_params = || {
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ]
    };
    let all_finite = |count: usize| (0..count).map(ArgGuard::AllFinite).collect();
    // L2: norm(v) — the generic norm name lowers to VectorNorm.
    let l2 = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("norm".into()), 1)
            .expect("norm signature is conflict-free");
        (
            Term::Apply {
                operator: SymbolId("norm".into()),
                arguments: vec![Term::Variable(VariableId("v".into()))],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // L1: sum(map(abs, v)) — abs over the vector, then the sum reduce.
    let l1 = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("abs", 1usize), ("sum", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("norm1 signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("abs".into()),
                    arguments: vec![Term::Variable(VariableId("v".into()))],
                }],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // Linf: vmax(map(abs, v)) — abs over the vector, then the max reduce.
    let linf = {
        let mut signature = Signature::default();
        for (symbol, arity) in [("abs", 1usize), ("vmax", 1)] {
            signature
                .insert(SymbolId(symbol.into()), arity)
                .expect("norminf signature is conflict-free");
        }
        (
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![Term::Apply {
                    operator: SymbolId("abs".into()),
                    arguments: vec![Term::Variable(VariableId("v".into()))],
                }],
            },
            signature,
            vector_param(),
            all_finite(1),
        )
    };
    // Inner product: dot(u, v) — the generic dot.
    let inner = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("dot".into()), 2)
            .expect("dot signature is conflict-free");
        (
            Term::Apply {
                operator: SymbolId("dot".into()),
                arguments: vec![
                    Term::Variable(VariableId("u".into())),
                    Term::Variable(VariableId("v".into())),
                ],
            },
            signature,
            two_vector_params(),
            all_finite(2),
        )
    };
    let direct = |operator: &'static str, params: Vec<(String, ParamShape)>| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), params.len())
            .expect("linear algebra signature is conflict-free");
        let arguments = params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect();
        let guards = all_finite(params.len());
        (
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments,
            },
            signature,
            params,
            guards,
        )
    };
    let solve = direct(
        "solve_linear",
        vec![
            ("A".to_string(), ParamShape::Matrix),
            ("b".to_string(), ParamShape::Vector),
        ],
    );
    let lu = direct("lu", vec![("A".to_string(), ParamShape::Matrix)]);
    let qr = direct("qr", vec![("A".to_string(), ParamShape::Matrix)]);
    let outer = direct(
        "outer_product",
        vec![
            ("u".to_string(), ParamShape::Vector),
            ("v".to_string(), ParamShape::Vector),
        ],
    );
    vec![
        ("std.linalg.norm", l2),
        ("std.linalg.norm1", l1),
        ("std.linalg.norminf", linf),
        ("std.linalg.inner_product", inner),
        ("std.linalg.solve_linear", solve),
        ("std.linalg.lu", lu),
        ("std.linalg.qr", qr),
        ("std.linalg.outer_product", outer),
    ]
    .into_iter()
    .map(|(name, (term, signature, params, guards))| (name, term, signature, params, guards))
    .collect()
}

/// The graph algorithm cells (r2-graphs-masa slice 2): registry DATA
/// over the slice-1 EMIR ops — zero per-op VM code (the fjxh.14
/// anti-LOC law). `std.graph.shortest_distances` declares the
/// all-finite weight guard so a NaN/Inf weight refuses typed
/// (`E-GRAPH-004` at the VM seam) — never silent NaN distances.
fn graph_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let adjacency_param = vec![("adj".to_string(), ParamShape::Matrix)];
    let traversal_params = vec![
        ("adj".to_string(), ParamShape::Matrix),
        ("source".to_string(), ParamShape::Scalar),
    ];
    let finite_adjacency = || vec![ArgGuard::AllFinite(0)];
    let cell = |name: &'static str, operator: &str, arity: usize| {
        let variable_names: &[&str] = if arity == 1 {
            &["adj"]
        } else {
            &["adj", "source"]
        };
        let arguments: Vec<Term> = variable_names
            .iter()
            .map(|variable| Term::Variable(VariableId((*variable).into())))
            .collect();
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), arity)
            .expect("graph cell signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments,
            },
            signature,
            if arity == 1 {
                adjacency_param.clone()
            } else {
                traversal_params.clone()
            },
            finite_adjacency(),
        )
    };
    vec![
        cell("std.graph.reachability", "reachability", 2),
        cell("std.graph.bfs_order", "bfs_order", 2),
        cell("std.graph.shortest_distances", "shortest_distances", 2),
        cell("std.graph.out_degrees", "out_degrees", 1),
        // Spectral basics (slice 3): the Laplacian as registry DATA;
        // the spectrum composes through the existing symmetric eigen
        // op.
        cell("std.graph.laplacian", "graph_laplacian", 1),
        // Directed → spectral path (slice 4): symmetrization as
        // registry DATA (weight-preserving (A+Aᵀ)/2 convention).
        cell("std.graph.symmetrize", "graph_symmetrize", 1),
        // Negative-edge methods (slice 5): Bellman-Ford as registry
        // DATA; negative weights ADMITTED, reachable negative cycles
        // refuse E-GRAPH-005 at the kernel/wrapper layer.
        cell("std.graph.bellman_ford", "bellman_ford", 2),
        // Sparse storage (slice 6): COO extraction/build as registry
        // DATA (duplicates SUM; malformed streams refuse E-GRAPH-006).
        // The build cell has MIXED param shapes (scalar n, vector
        // triplets) and guards the triplet stream (index 1), so it
        // bypasses the adjacency-cell helper.
        cell("std.graph.sparse_triplets", "sparse_triplets", 1),
        {
            let mut signature = Signature::default();
            signature
                .insert(SymbolId("sparse_from_triplets".into()), 2)
                .expect("graph cell signature is conflict-free");
            (
                "std.graph.sparse_from_triplets",
                Term::Apply {
                    operator: SymbolId("sparse_from_triplets".into()),
                    arguments: vec![
                        Term::Variable(VariableId("n".into())),
                        Term::Variable(VariableId("triplets".into())),
                    ],
                },
                signature,
                vec![
                    ("n".to_string(), ParamShape::Scalar),
                    ("triplets".to_string(), ParamShape::Vector),
                ],
                vec![ArgGuard::AllFinite(1)],
            )
        },
    ]
    .into_iter()
    .chain(optimization_cells())
    .chain(polynomial_cells())
    .chain(control_cells())
    .chain(category_cells())
    .chain(pde_cells())
    .chain(probability_cells())
    .collect()
}

/// The PDE cells (emath-xx0x.4 thin nucleus): registry DATA over the
/// spectral Poisson op — zero per-op VM code (the fjxh.14 anti-LOC
/// law). The all-finite guard on the load keeps NaN out of the
/// transform at the cell seam (the kernel's own E-PDE-002 guards the
/// bare-op path).
fn pde_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let sine = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poisson_sine".into()), 1)
            .expect("poisson_sine signature is conflict-free");
        (
            "std.pde.poisson_sine",
            Term::Apply {
                operator: SymbolId("poisson_sine".into()),
                arguments: vec![Term::Variable(VariableId("load".into()))],
            },
            signature,
            vec![("load".to_string(), ParamShape::Vector)],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![sine]
}

/// The probability cells (emath-xx0x.5 thin nucleus): registry DATA
/// over the sampling/density EMIR ops — zero per-op VM code (the
/// fjxh.14 anti-LOC law). The all-finite guard on the params keeps
/// NaN out of the generators at the cell seam (the kernel's own
/// E-PROB-001/002 codes guard the bare-op path). The seed→stream
/// mapping (f64 bits → SplitMix64 state) is PROVISIONAL: the vnqo
/// stream contract owns the seed/stream semantics above this layer.
fn probability_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let sample_cell = |name: &'static str, operator: &'static str, kind: ProbKind| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), 3)
            .expect("sample signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments: vec![
                    Term::Variable(VariableId("params".into())),
                    Term::Variable(VariableId("seed".into())),
                    Term::Variable(VariableId("draws".into())),
                ],
            },
            signature,
            vec![
                ("params".to_string(), ParamShape::Vector),
                ("seed".to_string(), ParamShape::Scalar),
                ("draws".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    let density_cell = |name: &'static str, operator: &'static str| {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId(operator.into()), 2)
            .expect("density signature is conflict-free");
        (
            name,
            Term::Apply {
                operator: SymbolId(operator.into()),
                arguments: vec![
                    Term::Variable(VariableId("params".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("params".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![
        sample_cell("std.prob.normal_sample", "normal_sample", ProbKind::Normal),
        sample_cell(
            "std.prob.uniform_sample",
            "uniform_sample",
            ProbKind::Uniform,
        ),
        sample_cell(
            "std.prob.bernoulli_sample",
            "bernoulli_sample",
            ProbKind::Bernoulli,
        ),
        density_cell("std.prob.normal_density", "normal_density"),
        density_cell("std.prob.uniform_density", "uniform_density"),
        density_cell("std.prob.bernoulli_pmf", "bernoulli_pmf"),
    ]
}

/// The optimization cells (r3-lp-milp-wlif slice 1): registry DATA over
/// the LP/Pareto EMIR ops — zero per-op VM code (the fjxh.14 anti-LOC
/// law). Both declare the all-finite guard on every numeric argument
/// (the strict-f64 finite policy; `E-CELL-006` at the seam, the
/// kernel's own E-LP/E-PARETO codes guard the bare-op path).
fn optimization_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let lp_params = vec![
        ("A".to_string(), ParamShape::Matrix),
        ("b".to_string(), ParamShape::Vector),
        ("c".to_string(), ParamShape::Vector),
    ];
    let pareto_params = vec![("points".to_string(), ParamShape::Matrix)];
    let lp = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("lp_minimize".into()), 3)
            .expect("lp signature is conflict-free");
        (
            "std.optimize.lp",
            Term::Apply {
                operator: SymbolId("lp_minimize".into()),
                arguments: vec![
                    Term::Variable(VariableId("A".into())),
                    Term::Variable(VariableId("b".into())),
                    Term::Variable(VariableId("c".into())),
                ],
            },
            signature,
            lp_params,
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let pareto = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("pareto_front".into()), 1)
            .expect("pareto signature is conflict-free");
        (
            "std.optimize.pareto_front",
            Term::Apply {
                operator: SymbolId("pareto_front".into()),
                arguments: vec![Term::Variable(VariableId("points".into()))],
            },
            signature,
            pareto_params,
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![lp, pareto]
}

/// The polynomial cells (r3-funcspaces-poly-hjor slice 1): registry
/// DATA over the poly EMIR ops — zero per-op VM code (the fjxh.14
/// anti-LOC law). Addition needs no cell (it binds the generic vector
/// add at call level).
fn polynomial_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let two_vectors = || {
        vec![
            ("a".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
        ]
    };
    let mul = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poly_mul".into()), 2)
            .expect("poly_mul signature is conflict-free");
        (
            "std.poly.mul",
            Term::Apply {
                operator: SymbolId("poly_mul".into()),
                arguments: vec![
                    Term::Variable(VariableId("a".into())),
                    Term::Variable(VariableId("b".into())),
                ],
            },
            signature,
            two_vectors(),
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
        )
    };
    let eval = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poly_eval".into()), 2)
            .expect("poly_eval signature is conflict-free");
        (
            "std.poly.eval",
            Term::Apply {
                operator: SymbolId("poly_eval".into()),
                arguments: vec![
                    Term::Variable(VariableId("p".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("p".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![mul, eval]
}

/// The control cells (emath-r3-sde-control-zxkl thin B43): registry
/// DATA over the control EMIR ops — zero per-op VM code (the fjxh.14
/// anti-LOC law). The all-finite guards keep NaN out of the cell seam
/// (the kernels' own E-CONTROL-001..005 codes guard the bare-op path).
fn control_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let transfer = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("transfer_eval".into()), 3)
            .expect("transfer_eval signature is conflict-free");
        (
            "std.control.transfer_eval",
            Term::Apply {
                operator: SymbolId("transfer_eval".into()),
                arguments: vec![
                    Term::Variable(VariableId("num".into())),
                    Term::Variable(VariableId("den".into())),
                    Term::Variable(VariableId("x".into())),
                ],
            },
            signature,
            vec![
                ("num".to_string(), ParamShape::Vector),
                ("den".to_string(), ParamShape::Vector),
                ("x".to_string(), ParamShape::Scalar),
            ],
            vec![ArgGuard::AllFinite(0), ArgGuard::AllFinite(1)],
        )
    };
    let dc_gain = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("dc_gain".into()), 3)
            .expect("dc_gain signature is conflict-free");
        (
            "std.control.dc_gain",
            Term::Apply {
                operator: SymbolId("dc_gain".into()),
                arguments: vec![
                    Term::Variable(VariableId("A".into())),
                    Term::Variable(VariableId("b".into())),
                    Term::Variable(VariableId("c".into())),
                ],
            },
            signature,
            vec![
                ("A".to_string(), ParamShape::Matrix),
                ("b".to_string(), ParamShape::Vector),
                ("c".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let poles_stable = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("poles_stable".into()), 1)
            .expect("poles_stable signature is conflict-free");
        (
            "std.control.poles_stable",
            Term::Apply {
                operator: SymbolId("poles_stable".into()),
                arguments: vec![Term::Variable(VariableId("den".into()))],
            },
            signature,
            vec![("den".to_string(), ParamShape::Vector)],
            vec![ArgGuard::AllFinite(0)],
        )
    };
    vec![transfer, dc_gain, poles_stable]
}

/// The category cells (emath-r3-abstract-algebra-88wo thin B39):
/// registry DATA over the category EMIR ops — zero per-op VM code (the
/// fjxh.14 anti-LOC law). The all-finite guards keep NaN out of the
/// cell seam (the kernels' own E-CAT-001..007 codes guard the bare-op
/// path; the law certification still runs inside the kernel).
fn category_cells() -> Vec<(
    &'static str,
    Term,
    Signature,
    Vec<(String, ParamShape)>,
    Vec<ArgGuard>,
)> {
    let check = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("category_check".into()), 3)
            .expect("category_check signature is conflict-free");
        (
            "std.category.check",
            Term::Apply {
                operator: SymbolId("category_check".into()),
                arguments: vec![
                    Term::Variable(VariableId("dom".into())),
                    Term::Variable(VariableId("cod".into())),
                    Term::Variable(VariableId("comp".into())),
                ],
            },
            signature,
            vec![
                ("dom".to_string(), ParamShape::Vector),
                ("cod".to_string(), ParamShape::Vector),
                ("comp".to_string(), ParamShape::Matrix),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
            ],
        )
    };
    let commutative = {
        let mut signature = Signature::default();
        signature
            .insert(SymbolId("diagram_commutative".into()), 4)
            .expect("diagram_commutative signature is conflict-free");
        (
            "std.category.commutative",
            Term::Apply {
                operator: SymbolId("diagram_commutative".into()),
                arguments: vec![
                    Term::Variable(VariableId("dom".into())),
                    Term::Variable(VariableId("cod".into())),
                    Term::Variable(VariableId("comp".into())),
                    Term::Variable(VariableId("faces".into())),
                ],
            },
            signature,
            vec![
                ("dom".to_string(), ParamShape::Vector),
                ("cod".to_string(), ParamShape::Vector),
                ("comp".to_string(), ParamShape::Matrix),
                ("faces".to_string(), ParamShape::Vector),
            ],
            vec![
                ArgGuard::AllFinite(0),
                ArgGuard::AllFinite(1),
                ArgGuard::AllFinite(2),
                ArgGuard::AllFinite(3),
            ],
        )
    };
    vec![check, commutative]
}
