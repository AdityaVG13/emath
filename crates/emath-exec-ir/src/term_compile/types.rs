//! Cell parameter shapes, guards, errors, and the compiled-cell type.

use super::*;

/// Declared shape of one cell parameter. Closed set; matrix/tensor
/// parameter shapes are later spine work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamShape {
    /// IEEE-754 binary64 scalar.
    Scalar,
    /// Rank-1 Float64 vector.
    Vector,
    /// Dense row-major Float64 matrix (the graph/linear-algebra
    /// carrier; slice 2 opened the call surface).
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
pub(super) enum Shape {
    Scalar,
    Vector,
    /// Dense matrix carrier (`Matrix<Float64>` params and the graph
    /// call slots; slice 2).
    Matrix,
    /// Comparison results (`lt`/`le`/`gt`/`ge`/`eq`/`ne`): the closed
    /// vocabulary composes booleans NOWHERE (every other arm matches
    /// Scalar/Vector only).
    Bool,
    /// An Option carrier (`option_some`/`option_none`). Carriers are
    /// OPAQUE at the shape level: the inner payload shape is not
    /// tracked — only the polarity/unwrap/error_of arms admit them
    /// (call surface).
    OptionCarrier,
    /// A Result carrier (`result_ok`/`result_err`). Same opacity law as
    /// the Option carrier; the error payload shares the Result slot
    /// (`Value::Result { ok, payload }`).
    ResultCarrier,
}

impl Shape {
    /// The concrete payload/default shapes the Option/Result call
    /// surface admits: Scalar, Vector, Matrix. Carriers refuse (they
    /// are opaque) and Bool refuses (the closed vocabulary composes
    /// booleans NOWHERE).
    pub(super) const fn is_concrete_payload(self) -> bool {
        matches!(self, Self::Scalar | Self::Vector | Self::Matrix)
    }

    /// True for a (nested) carrier shape. Nested payloads are the
    /// type-honest rule lifted in
    /// `Option<T>` and `Result<T,E>` are themselves payloads, so
    /// a carrier is an acceptable payload for the three CONSTRUCTORS
    /// (`option_some`/`result_ok`/`result_err`) and an acceptable
    /// unwrap_or DEFAULT when the retrieved payload is a carrier. Bool
    /// still composes nowhere (not a carrier, not concrete).
    pub(super) const fn is_payload_candidate(self) -> bool {
        self.is_concrete_payload() || matches!(self, Self::OptionCarrier | Self::ResultCarrier)
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
    /// function is minted on the fly — that is the law).
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
            // Matrix carriers: the guard
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

/// Post-body certificate predicate (cell DATA). A
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
///
/// `PartialEq` compares the compiled program field-wise; `Eq` is
/// deliberately absent because `EmirProgram` carries f64 payloads where
/// bit equality is not a law.
#[derive(Clone, Debug, PartialEq)]
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
