//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! Phase 1 lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

pub mod builtin;
mod emitter;
pub mod growth;
pub mod image;
pub mod install;
pub mod interp;
pub mod lazy;
pub mod native_kernel;
pub mod optimize;
pub mod runner;
pub mod shake;
pub mod specialize;
pub mod term_compile;

pub use builtin::BuiltinId;
pub use runner::{
    Continuation, DAEDisposition, DAEIndex, InitializationVerdict, SimulateOptions, StepMethod,
    Trajectory, TrajectorySample, definition_order, simulate_continuous,
    simulate_continuous_dispositioned, simulate_continuous_with, step_continuous,
    step_continuous_values,
};

use emath_core::{Span, special::SpecialFn};
pub use emath_ir::CellClass;
use emath_ir::SemanticPackage;

/// Evaluation resource budget. Resource exhaustion is a typed refusal
/// (`EvalFault::BudgetExhausted`) — never partial authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalBudget {
    /// Maximum interpreted op steps.
    pub max_steps: u32,
    /// Maximum capability applications.
    pub max_capability_applications: u32,
}

impl Default for EvalBudget {
    fn default() -> Self {
        Self {
            max_steps: u32::MAX,
            max_capability_applications: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmirValue(pub u32);

/// One axis of [`EmirOp::TensorSlice`]: a scalar point or a half-open range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmirSliceAxis {
    Point(EmirValue),
    Range { start: EmirValue, end: EmirValue },
}

/// Accumulation strategy for [`EmirOp::Fold`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldCombine {
    Add,
    Mul,
    And,
    Or,
}

/// How out-of-range stencil indices resolve: `Clamp` (replicate the edge
/// cell), `Neumann` (mirror the next interior cell), `OneSided` (linear
/// extrapolation; first-order one-sided first differences), or `Dirichlet`
/// (fixed boundary values). 2D and 3D admit
/// `Clamp`/`Neumann`/`OneSided`; fixed Dirichlet faces remain unsupported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgePolicy {
    Clamp,
    Neumann,
    OneSided,
    Dirichlet { left: f64, right: f64 },
}

/// The admitted distribution families of the probability nucleus
/// (xx0x.5). The `u8` code is the stable kernel encoding (codegen
/// renders it; the rt wrappers decode it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbKind {
    /// Normal(μ, σ) — Box–Muller sampling, exact density.
    Normal,
    /// Uniform(a, b) — affine map of [0, 1), exact density.
    Uniform,
    /// Bernoulli(p) — threshold sampling (p ∈ {0, 1} exact), PMF.
    Bernoulli,
}

impl ProbKind {
    /// The rt kernel's `u8` encoding.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Uniform => 1,
            Self::Bernoulli => 2,
        }
    }

    /// SSA/cell-surface name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Uniform => "uniform",
            Self::Bernoulli => "bernoulli",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmirOp {
    ConstF64(u64),
    ConstI64(i64),
    /// UTF-8 text constant. Identity is the exact normalized source value.
    ConstText(String),
    /// Pure template substitution. The parser has already validated the
    /// template and the arguments follow its holes in source order.
    FormatText {
        template: String,
        arguments: Vec<EmirValue>,
    },
    /// Unicode scalar count of normalized Text.
    TextLength(EmirValue),
    /// Explicit Unicode NFC normalization.
    TextNfc(EmirValue),
    /// Strict-f64 special-function evaluation. `error_bound` selects
    /// the declared bound instead of the central value.
    SpecialFunction {
        function: SpecialFn,
        arguments: Vec<EmirValue>,
        error_bound: bool,
    },
    /// Pure report section constructor.
    ReportSection {
        heading: EmirValue,
        body: EmirValue,
    },
    /// Pure single-section document constructor.
    ReportDocument {
        title: EmirValue,
        section: EmirValue,
    },
    /// Deterministic Markdown rendering.
    ReportMarkdown(EmirValue),
    /// Deterministic LaTeX rendering.
    ReportLatex(EmirValue),
    /// Immutable time-series data with identity-bearing interpretation policy.
    SeriesCreate {
        points: Vec<(f64, f64)>,
        interpolation: String,
        extrapolation: String,
    },
    /// Evaluate a series at one time coordinate.
    SeriesSample {
        series: EmirValue,
        time: EmirValue,
    },
    /// Finite set creation; guarded elements come from comprehensions.
    SetCreate {
        elements: Vec<EmirValue>,
        guards: Vec<Option<EmirValue>>,
    },
    /// Extensional finite-set membership.
    SetContains {
        element: EmirValue,
        set: EmirValue,
    },
    /// Inline nominal record construction.
    RecordCreate {
        type_name: String,
        fields: Vec<(String, EmirValue)>,
    },
    /// Complex constant (real, imaginary). B14.
    ConstComplex(f64, f64),
    /// Exact-rational construction (emath-rat-real-types-p5cj): build
    /// `num/den` from two integer registers. Exact i128 math — gcd
    /// reduced, denominator forced positive, never f64. A zero
    /// denominator is a typed refusal (never a panic, never a silent
    /// zero).
    RatConstruct { num: EmirValue, den: EmirValue },
    /// Exact rational addition: common denominator, then gcd-reduced.
    /// Intermediate overflow is a typed refusal, never a silent wrap.
    RatAdd(EmirValue, EmirValue),
    /// Canonicalize an exact rational: gcd-reduce and force the
    /// denominator positive.
    RatNorm(EmirValue),
    /// Certified interval constructor `[lo, hi]` (8pjn). Faults at run
    /// when the bounds are non-finite or `lo > hi` — an ill-formed
    /// interval is a refusal, never a silently swapped pair.
    IntervalCreate(EmirValue, EmirValue),
    /// Interval intersection; empty result is a typed refusal.
    IntervalIntersect(EmirValue, EmirValue),
    /// Boolean constant; produced by optimizer folding.
    ConstBool(bool),
    LoadInput(u16),
    LoadState(u16),
    F64Add(EmirValue, EmirValue),
    F64Sub(EmirValue, EmirValue),
    F64Mul(EmirValue, EmirValue),
    F64Div(EmirValue, EmirValue),
    F64Pow(EmirValue, EmirValue),
    Neg(EmirValue),
    /// Generic unary/binary math builtin via the `BuiltinId` registry.
    UnaryBuiltin(BuiltinId, EmirValue),
    BinaryBuiltin(BuiltinId, EmirValue, EmirValue),
    Lt(EmirValue, EmirValue),
    Le(EmirValue, EmirValue),
    Gt(EmirValue, EmirValue),
    Ge(EmirValue, EmirValue),
    Eq(EmirValue, EmirValue),
    Ne(EmirValue, EmirValue),
    And(EmirValue, EmirValue),
    Or(EmirValue, EmirValue),
    /// `==>` — `!a || b`
    Imply(EmirValue, EmirValue),
    /// `<==>` — `a == b` for Bool
    Iff(EmirValue, EmirValue),
    Not(EmirValue),
    IsFinite(EmirValue),
    Select {
        condition: EmirValue,
        then_value: EmirValue,
        else_value: EmirValue,
    },
    VectorCreate(Vec<EmirValue>),
    /// Materialize coefficients of a finite-budget linear recurrence.
    /// Recurrence coefficient `j` multiplies the term at offset `j + 1`.
    SequenceGenerate {
        initial: EmirValue,
        recurrence: EmirValue,
        budget: EmirValue,
    },
    /// Truncated Cauchy product of two materialized generating functions.
    SequenceConvolve {
        left: EmirValue,
        right: EmirValue,
        count: EmirValue,
    },
    MatrixCreate {
        rows: usize,
        cols: usize,
        elements: Vec<EmirValue>,
    },
    VectorIndex {
        vector: EmirValue,
        index: EmirValue,
    },
    MatrixIndex {
        matrix: EmirValue,
        row: EmirValue,
        col: EmirValue,
    },
    VectorAdd(EmirValue, EmirValue),
    VectorSub(EmirValue, EmirValue),
    VectorScale(EmirValue, EmirValue),
    VectorDot(EmirValue, EmirValue),
    VectorNorm(EmirValue),
    VectorLength(EmirValue),
    /// 1D convolution with fixed weights and an edge policy; output length
    /// equals input length.
    Stencil1d {
        input: EmirValue,
        weights: Vec<f64>,
        center: usize,
        edge: EdgePolicy,
    },
    /// 2D 3x3 stencil convolution (row-major weights, length 9); output
    /// shape equals input shape. `Dirichlet` is not admitted in Phase 1.
    Stencil2d {
        input: EmirValue,
        weights: Vec<f64>,
        center: (usize, usize),
        edge: EdgePolicy,
    },
    /// 3D 3x3x3 stencil convolution (axis-major weights, length 27);
    /// output shape equals the rank-3 Tensor input shape.
    Stencil3d {
        input: EmirValue,
        weights: Vec<f64>,
        center: (usize, usize, usize),
        edge: EdgePolicy,
    },
    MatrixAdd(EmirValue, EmirValue),
    MatrixSub(EmirValue, EmirValue),
    MatrixScale(EmirValue, EmirValue),
    MatrixMulVector(EmirValue, EmirValue),
    MatrixMulMatrix(EmirValue, EmirValue),
    MatrixTranspose(EmirValue),
    /// Spectral decomposition of a real SYMMETRIC square matrix
    /// (xx0x.2): eigenvalues ASCENDING (deterministic cyclic Jacobi).
    /// Non-square/not-symmetric input refuses typed (`E-LINALG-001/2`).
    EigenSymmetric(EmirValue),
    /// Eigenvector MATRIX of a real symmetric square matrix: column j
    /// is the unit eigenvector for eigenvalue j (ascending, canonical
    /// signs).
    EigenVectorsSymmetric(EmirValue),
    /// Singular values of a rectangular matrix, DESCENDING (thin rank
    /// via the symmetric AᵀA eigenproblem).
    SvdSingularValues(EmirValue),
    /// Thin SVD factors packed row-major as `[U; s; Vᵀ]`: rows 0..m are
    /// U (m×r, zero columns for rank-deficient directions), row m is s
    /// (r values), rows m+1..m+1+r are Vᵀ (r×cols); r = min(m, cols).
    /// Width is max(cols, r) with zero padding.
    SvdFactors(EmirValue),
    /// Conjugate-gradient solve of `A x = b` over A's dense storage
    /// (SPD-convergence-checked; non-convergence refuses typed
    /// `E-LINALG-003`, never a silently wrong x).
    CgSolve(EmirValue, EmirValue),
    /// Dense partial-pivot solve of `A x = b`.
    LinearSolve(EmirValue, EmirValue),
    /// Packed partial-pivot factors `[p; L; U]`.
    LuFactors(EmirValue),
    /// Packed thin factors `[Q; R]`.
    QrFactors(EmirValue),
    /// Matrix outer product of two vectors.
    OuterProduct(EmirValue, EmirValue),
    /// BFS reachability mask over a DENSE adjacency carrier
    /// (r2-graphs-masa slice 1): element `v` is 1.0 when `v` is
    /// reachable from `source` (the source reaches itself). Non-square
    /// carrier / out-of-range source refuses typed
    /// (`E-GRAPH-001/003`).
    GraphReachable(EmirValue, EmirValue),
    /// BFS visit order over a dense adjacency carrier: source first,
    /// neighbors discovered in ASCENDING index (breadth-first, never
    /// depth-first, never insertion-order). Same typed refusals.
    GraphBfsOrder(EmirValue, EmirValue),
    /// O(n²) selection Dijkstra over the carrier's nonnegative weights:
    /// unreachable vertices are +Inf; a NEGATIVE weight refuses typed
    /// `E-GRAPH-002` (Dijkstra's precondition, never a silently wrong
    /// distance).
    GraphDijkstra(EmirValue, EmirValue),
    /// Out-degree per vertex: count of NONZERO entries per row (0.0 is
    /// no edge even in a weighted carrier; a self-loop counts).
    /// In-degree is this op over the transposed carrier.
    GraphDegreeOut(EmirValue),
    /// Unnormalized graph Laplacian `L = D − A` over the dense
    /// adjacency carrier (r2-graphs-masa slice 3): D the out-degree
    /// diagonal. The spectrum composes through the EXISTING
    /// `EigenSymmetric` op (an undirected graph's Laplacian is
    /// symmetric; a directed carrier's spectrum refuses the symmetric
    /// gate — the documented class fence).
    GraphLaplacian(EmirValue),
    /// Standard-form linear program (r3-lp-milp-wlif slice 1):
    /// minimize `cᵀx` s.t. `A x ≤ b`, `x ≥ 0`, `b ≥ 0` — Bland's-rule
    /// simplex (smallest-index rules, provably terminating). Unbounded
    /// objective refuses typed `E-LP-001`; a negative right side
    /// `E-LP-002` (normalization is a named deferral); mismatched
    /// operand dimensions `E-LP-003`; non-finite coefficients
    /// `E-LP-004`. MILP (integer constraints) is the named next slice.
    LpMinimize(EmirValue, EmirValue, EmirValue),
    /// Strict Pareto front over a finite objective carrier (all
    /// MINIMIZED; maximize by negating): returns the non-dominated
    /// mask in point-index order — the portfolio artifact's
    /// deterministic data. Identical points do not dominate each
    /// other. Non-finite entry refuses `E-PARETO-001`.
    ParetoFront(EmirValue),
    /// Cauchy convolution of two coefficient vectors, ASCENDING order
    /// (r3-funcspaces-poly-hjor slice 1): the B28 compute layer. The
    /// EMPTY operand is the zero polynomial (additive identity).
    /// Non-finite coefficient refuses `E-POLY-001`.
    PolyMul(EmirValue, EmirValue),
    /// Horner evaluation of a coefficient vector (ascending order) at
    /// a point; empty coefficients evaluate to 0.0. Non-finite
    /// coefficient/point refuses `E-POLY-001/002`.
    PolyEval(EmirValue, EmirValue),
    /// One implicit (backward) Euler step for a scalar ODE with an
    /// ASCENDING polynomial rate law (xx0x.3 thin nucleus): Newton on
    /// the residual `y1 − h·f(y1) − y0` to machine tolerance; a
    /// non-converged or non-finite solve refuses typed `E-ODE-001`,
    /// non-positive `dt` refuses `E-ODE-003`.
    OdeBackwardEuler(EmirValue, EmirValue, EmirValue),
    /// One velocity-Verlet step for the separable system `q' = v`,
    /// `v' = a(q)` (acceleration as an ascending polynomial of
    /// position; xx0x.3): kick-drift-kick, time-reversible (`h` may be
    /// negative). Non-finite carriers refuse `E-ODE-004`.
    OdeVelocityVerlet(EmirValue, EmirValue, EmirValue, EmirValue),
    /// Symmetrized adjacency `(A + Aᵀ)/2` (masa slice 4): the
    /// weight-preserving convention — the output is a symmetric
    /// carrier for the existing laplacian/symmetric-eigen path.
    /// Symmetrization is a USER choice; the laplacian/eigen directed
    /// fences stay. Refusals reuse the closed graph set
    /// (`E-GRAPH-001/002/004`).
    GraphSymmetrize(EmirValue),
    /// Bellman-Ford shortest distances from `source` (masa slice 5):
    /// negative edge weights ADMITTED; a negative cycle reachable
    /// from the source refuses `E-GRAPH-005` (no answer exists);
    /// unreachable vertices are +Inf. Other refusals reuse the closed
    /// graph set (`E-GRAPH-001/003/004`).
    GraphBellmanFord(EmirValue, EmirValue),
    /// Sparse COO extraction (masa slice 6): the dense adjacency as a
    /// flat triplet stream `[u, v, w, ...]` ascending (u, v); explicit
    /// 0.0 entries are skipped. Carrier refusals reuse the closed set.
    GraphSparseTriplets(EmirValue),
    /// Sparse COO build (masa slice 6): the dense adjacency from a
    /// triplet stream; DUPLICATE (u, v) entries SUM (parallel edges
    /// add weights). Out-of-range indices refuse `E-GRAPH-003`,
    /// non-finite weights `E-GRAPH-004`, a length not a multiple of
    /// three `E-GRAPH-006`.
    GraphSparseFromTriplets(EmirValue, EmirValue),
    /// Exact INTEGER null vector of an integer matrix (rymw prime):
    /// the generic exact-integer primitive — rational Gauss-Jordan with
    /// gcd reduction, NO floating point. Requires a nullspace of
    /// dimension EXACTLY ONE: any other dimension refuses typed
    /// (`E-NULLSPACE-002`, never a guessed basis vector). Non-integral
    /// or overflowing inputs refuse `E-NULLSPACE-001`. The returned
    /// vector is primitive (entries coprime, first nonzero entry
    /// positive — a canonical generator, so results are permutation-
    /// and column-order deterministic). Chemistry balancing is a cell
    /// over this op; the op itself has no domain identity.
    IntNullspace(EmirValue),
    /// Exact integer product DIFFERENCE `∏a_i − ∏b_i` (thermo slice):
    /// the generic exact-rational equality primitive behind Wegscheider
    /// cycle consistency. Computed over u128/i128 intermediates with NO
    /// floating point; the result is the exact difference (f64-exact
    /// for small inputs). Non-integral entries refuse `E-EXACT-001`;
    /// overflow refuses `E-EXACT-002`. Zero-domain — used by the
    /// cycle-consistency cell, generic like IntNullspace.
    ExactProductDelta(EmirValue, EmirValue),
    // ── Option/Result value semantics (aj8d thin slice) ─────────────
    // TOTAL semantics: no panicking unwrap exists — the honesty gate
    // is UnwrapOr (defaults evaluate eagerly, register discipline).
    /// Wrap a value into `Option::Some`.
    OptionSome(EmirValue),
    /// The None option (no payload; a None carries NOTHING).
    OptionNone,
    /// Polarity of an option (reads the TAG, not the content:
    /// `Some(None)` is Some).
    OptionIsSome(EmirValue),
    /// Total unwrap: the inner value when Some, else the default.
    OptionUnwrapOr(EmirValue, EmirValue),
    /// Wrap a value into `Result::Ok`.
    ResultOk(EmirValue),
    /// Wrap an error payload into `Result::Err` (the payload is a
    /// real value, preserved — never swallowed).
    ResultErr(EmirValue),
    /// Polarity of a result.
    ResultIsOk(EmirValue),
    /// Total unwrap: the value when Ok, else the default.
    ResultUnwrapOr(EmirValue, EmirValue),
    /// The error as an OPTION: `None` when Ok, `Some(error)` when Err
    /// (Result errors compose with the Option ops).
    ResultErrorOf(EmirValue),
    /// Spectral Poisson solve `-u'' = f` on [0,1], Dirichlet class
    /// (xx0x.4 thin nucleus): discrete sine diagonalization of the 3-point
    /// Laplacian — forward DST-I of the interior load, division by the
    /// positive eigenvalues, inverse transform. Empty interior refuses
    /// `E-PDE-001`; non-finite loads refuse `E-PDE-002`.
    PoissonDirichletSine(EmirValue),
    /// Seeded sample from an admitted distribution (xx0x.5 thin
    /// nucleus): the explicit seed and optional declared split path enter
    /// the vnqo counter-based stream contract before seeding the local
    /// sampling kernel. Invalid parameters refuse
    /// `E-PROB-001`, non-finite `E-PROB-002`, wrong arity
    /// `E-PROB-003`. Same seed ⟹ bit-identical draws.
    ProbSample {
        kind: ProbKind,
        params: EmirValue,
        seed: EmirValue,
        draws: EmirValue,
        stream: Option<EmirValue>,
    },
    /// Exact density / PMF of an admitted distribution at a point
    /// (xx0x.5): closed forms, not estimates. Same refusal surface as
    /// [`EmirOp::ProbSample`].
    ProbDensity {
        kind: ProbKind,
        params: EmirValue,
        x: EmirValue,
    },
    /// Transfer-function evaluation (zxkl thin B43): `num(x)/den(x)`
    /// over ASCENDING carriers (the B28 representation). A denominator
    /// that vanishes at the point (a pole hit, or the zero polynomial)
    /// refuses `E-CONTROL-002`; non-finite carriers/points refuse
    /// `E-CONTROL-001`.
    ControlTransferEval(EmirValue, EmirValue, EmirValue),
    /// State-space DC gain `c·(−A)⁻¹·b` (zxkl thin B43; implicit D = 0
    /// — the feedthrough term is the named deferral). Stability is the
    /// Faddeev–LeVerrier characteristic polynomial under the
    /// Routh–Hurwitz sign test: an unstable carrier refuses
    /// `E-CONTROL-003`, a marginal (degenerate Routh) carrier
    /// `E-CONTROL-005`; shape mismatches refuse `E-CONTROL-004`.
    ControlDcGain(EmirValue, EmirValue, EmirValue),
    /// Routh–Hurwitz strict-stability predicate over an ASCENDING
    /// denominator (zxkl thin B43): TRUE = all roots strictly in the
    /// open left half plane, FALSE = provably unstable. A degenerate
    /// table (zero first-column entry) refuses `E-CONTROL-005`; the
    /// zero polynomial refuses `E-CONTROL-002`; non-finite carriers
    /// `E-CONTROL-001`.
    ControlPolesStable(EmirValue),
    /// Finite-category law gate (88wo thin B39): certifies the dense
    /// composition-table carrier — composition totality/alignment,
    /// identity existence, associativity (morphisms ≤ 64, else
    /// `E-CAT-007`). Typed refusals `E-CAT-001..007` name the FIRST
    /// violated law in the documented pass order.
    CategoryCheck(EmirValue, EmirValue, EmirValue),
    /// Diagram commutativity over face path-pairs (88wo thin B39):
    /// each face record is `[start, end, len_l, len_r, left…, right…]`
    /// (paths ≥ 1 morphism); the carrier must certify first, then each
    /// face is commutative iff both path composites are the SAME
    /// morphism index. Returns the per-face mask in face order.
    CategoryDiagramCommutative(EmirValue, EmirValue, EmirValue, EmirValue),
    TensorCreate {
        shape: Vec<usize>,
        elements: Vec<EmirValue>,
    },
    TensorIndex {
        tensor: EmirValue,
        indices: Vec<EmirValue>,
    },
    TensorSlice {
        tensor: EmirValue,
        axes: Vec<EmirSliceAxis>,
    },
    TensorAdd(EmirValue, EmirValue),
    TensorSub(EmirValue, EmirValue),
    TensorScale(EmirValue, EmirValue),
    /// Einstein summation over the given subscripts (e.g. `"ik,kj->ij"`).
    Einsum {
        subscripts: String,
        inputs: Vec<EmirValue>,
    },
    /// Exact i64 factorial / modular inverse / congruence check.
    Factorial(EmirValue),
    ModInv(EmirValue, EmirValue),
    /// Universal exact-Euclidean integer remainder `a.rem_euclid(m)` on
    /// i64; typed Arithmetic fault when `m <= 0` (never a panic).
    IntRem(EmirValue, EmirValue),
    Congruence(EmirValue, EmirValue, EmirValue),
    /// Horner polynomial evaluation over GF(p) / Reed-Solomon encode /
    /// Hamming distance (RS proximity machinery).
    PolyEvalMod(EmirValue, EmirValue, EmirValue),
    RSEncode(EmirValue, EmirValue, EmirValue),
    HammingDistance(EmirValue, EmirValue),
    /// Fold sum/product/forall/exists over an integer range; `body` runs
    /// once per iteration with the loop variable as an extra input.
    Fold {
        start: EmirValue,
        end: EmirValue,
        init: EmirValue,
        combine: FoldCombine,
        loop_var_index: u16,
        body: EmirProgram,
    },
    /// Composite Simpson integration; `integrand` runs per sample point,
    /// `steps` must be even.
    Integral {
        start: EmirValue,
        end: EmirValue,
        steps: u32,
        loop_var_index: u16,
        integrand: EmirProgram,
    },
    /// Dual-number forward-mode derivative of `body` w.r.t. `var_index`.
    Differentiate {
        body: EmirProgram,
        var_index: u16,
    },
    /// Newton root-finding on `body` (the residual) w.r.t. `var_index`.
    Solve {
        body: EmirProgram,
        var_index: u16,
        tolerance: f64,
        max_iter: u32,
    },
    /// Newton on ∇f = 0 over `body` w.r.t. `var_indices`.
    Optimize {
        body: EmirProgram,
        var_indices: Vec<u16>,
        maximize: bool,
        learning_rate: f64,
        tolerance: f64,
        max_iter: u32,
    },
    /// Numerical limit: sample `body` approaching `target` from
    /// `direction` (0 = two-sided, +1 = above, -1 = below).
    SampleLimit {
        body: EmirProgram,
        var_index: u16,
        target: EmirValue,
        direction: EmirValue,
    },
    /// Adjoint-method reverse AD: one forward + one backward pass gives
    /// gradients w.r.t. all `var_indices` at O(cost).
    ReverseMode {
        body: EmirProgram,
        var_indices: Vec<u16>,
    },
    /// Capability-cell application — data, not a domain op: the named
    /// admitted cell dispatches from the capability layer (local
    /// reference semantics in the interp world; otherwise an outstanding
    /// provider call). Zero core delta: no `emath-ir` op enum grows.
    ApplyCapability {
        capability: String,
        class: CellClass,
        args: Vec<EmirValue>,
    },
    /// Elementwise unary builtin over a Float64 vector (generic broadcast;
    /// the reference-term compiler's map vocabulary, fjxh.5).
    VectorMap {
        builtin: BuiltinId,
        source: EmirValue,
    },
    /// Elementwise strict-f64 vector-scalar arithmetic with a broadcast
    /// scalar (canonical order: vector, scalar). Closed op set.
    VectorMapScalar {
        op: VectorScalarOp,
        vector: EmirValue,
        scalar: EmirValue,
    },
    /// Closed reduce (sum/max/min) over a Float64 vector; an empty input
    /// is a typed fault, never a silent identity.
    VectorReduce {
        reduce: ReduceId,
        source: EmirValue,
    },
    /// Whether every element of a Float64 vector is finite (the
    /// strict-f64 policy guard op).
    VectorAllFinite(EmirValue),
}

/// Closed elementwise vector-scalar arithmetic set for
/// [`EmirOp::VectorMapScalar`]: strict f64, IEEE semantics, single
/// rounding per element (bit-exact against the reference oracle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorScalarOp {
    /// `v_i + s`
    Add,
    /// `v_i - s`
    Sub,
    /// `v_i * s`
    Mul,
    /// `v_i / s` (IEEE `/0` stays Inf/NaN, matching generated Rust).
    Div,
}

impl VectorScalarOp {
    /// Stable token for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
        }
    }

    /// Evaluate one element against the broadcast scalar.
    #[must_use]
    pub fn eval(self, x: f64, scalar: f64) -> f64 {
        match self {
            Self::Add => x + scalar,
            Self::Sub => x - scalar,
            Self::Mul => x * scalar,
            Self::Div => x / scalar,
        }
    }
}

/// Closed reduce set for generic vector aggregation
/// ([`EmirOp::VectorReduce`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceId {
    /// Left-to-right strict f64 sum (same order as `Iterator::sum`, so
    /// compiled cells match the reference oracle bit-for-bit).
    Sum,
    /// Maximum; NaN propagates (`f64::max` silently drops NaN — never
    /// silent here).
    Max,
    /// Minimum; NaN propagates.
    Min,
}

impl ReduceId {
    /// Stable token for diagnostics and canonical output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Max => "max",
            Self::Min => "min",
        }
    }

    /// Strict left-to-right reduce over a non-empty vector. The caller
    /// (interpreter) faults on an empty vector before reaching this.
    #[must_use]
    pub fn eval(self, values: &[f64]) -> f64 {
        match self {
            Self::Sum => values.iter().fold(0.0_f64, |acc, &x| acc + x),
            Self::Max => values.iter().fold(f64::NEG_INFINITY, |acc, &x| {
                if x.is_nan() { f64::NAN } else { acc.max(x) }
            }),
            Self::Min => values.iter().fold(f64::INFINITY, |acc, &x| {
                if x.is_nan() { f64::NAN } else { acc.min(x) }
            }),
        }
    }
}

impl EmirOp {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ConstF64(_) => "const-f64",
            Self::ConstI64(_) => "const-i64",
            Self::ConstText(_) => "const-text",
            Self::FormatText { .. } => "format-text",
            Self::TextLength(_) => "text-length",
            Self::TextNfc(_) => "text-nfc",
            Self::ReportSection { .. } => "report-section",
            Self::ReportDocument { .. } => "report-document",
            Self::ReportMarkdown(_) => "report-markdown",
            Self::ReportLatex(_) => "report-latex",
            Self::SpecialFunction { .. } => "special-function",
            Self::SeriesCreate { .. } => "series-create",
            Self::SeriesSample { .. } => "series-sample",
            Self::SetCreate { .. } => "set-create",
            Self::SetContains { .. } => "set-contains",
            Self::RecordCreate { .. } => "record-create",
            Self::ConstComplex(..) => "const-complex",
            Self::RatConstruct { .. } => "rat-construct",
            Self::RatAdd(..) => "rat-add",
            Self::RatNorm(_) => "rat-norm",
            Self::IntervalCreate(..) => "interval-create",
            Self::IntervalIntersect(..) => "interval-intersect",
            Self::ConstBool(_) => "const-bool",
            Self::LoadInput(_) => "load-input",
            Self::LoadState(_) => "load-state",
            Self::F64Add(..) => "f64-add",
            Self::F64Sub(..) => "f64-sub",
            Self::F64Mul(..) => "f64-mul",
            Self::F64Div(..) => "f64-div",
            Self::F64Pow(..) => "f64-pow",
            Self::Neg(_) => "neg",
            Self::UnaryBuiltin(id, _) => id.name(),
            Self::BinaryBuiltin(id, _, _) => id.name(),
            Self::Lt(..) => "lt",
            Self::Le(..) => "le",
            Self::Gt(..) => "gt",
            Self::Ge(..) => "ge",
            Self::Eq(..) => "eq",
            Self::Ne(..) => "ne",
            Self::And(..) => "and",
            Self::Or(..) => "or",
            Self::Imply(..) => "imply",
            Self::Iff(..) => "iff",
            Self::Not(_) => "not",
            Self::IsFinite(_) => "is-finite",
            Self::Select { .. } => "select",
            Self::VectorCreate(_) => "vec-create",
            Self::MatrixCreate { .. } => "mat-create",
            Self::VectorIndex { .. } => "vec-index",
            Self::MatrixIndex { .. } => "mat-index",
            Self::VectorAdd(..) => "vec-add",
            Self::VectorSub(..) => "vec-sub",
            Self::VectorScale(..) => "vec-scale",
            Self::VectorDot(..) => "vec-dot",
            Self::VectorNorm(_) => "vec-norm",
            Self::VectorLength(_) => "vec-len",
            Self::Stencil1d { .. } => "stencil-1d",
            Self::Stencil2d { .. } => "stencil-2d",
            Self::Stencil3d { .. } => "stencil-3d",
            Self::MatrixAdd(..) => "mat-add",
            Self::MatrixSub(..) => "mat-sub",
            Self::MatrixScale(..) => "mat-scale",
            Self::MatrixMulVector(..) => "mat-mul-vec",
            Self::MatrixMulMatrix(..) => "mat-mul-mat",
            Self::MatrixTranspose(_) => "mat-transpose",
            Self::EigenSymmetric(_) => "eig-sym",
            Self::EigenVectorsSymmetric(_) => "eig-vecs",
            Self::SvdSingularValues(_) => "svd-values",
            Self::SvdFactors(_) => "svd-factors",
            Self::CgSolve(..) => "cg-solve",
            Self::LinearSolve(..) => "linear-solve",
            Self::LuFactors(_) => "lu-factors",
            Self::QrFactors(_) => "qr-factors",
            Self::OuterProduct(..) => "outer-product",
            Self::GraphReachable(..) => "graph-reachable",
            Self::GraphBfsOrder(..) => "graph-bfs-order",
            Self::GraphDijkstra(..) => "graph-dijkstra",
            Self::GraphDegreeOut(_) => "graph-degree-out",
            Self::GraphLaplacian(_) => "graph-laplacian",
            Self::GraphSymmetrize(_) => "graph-symmetrize",
            Self::GraphBellmanFord(..) => "graph-bellman-ford",
            Self::GraphSparseTriplets(_) => "graph-sparse-triplets",
            Self::GraphSparseFromTriplets(..) => "graph-sparse-from-triplets",
            Self::IntNullspace(_) => "int-nullspace",
            Self::ExactProductDelta(..) => "exact-product-delta",
            Self::OptionSome(_) => "option-some",
            Self::OptionNone => "option-none",
            Self::OptionIsSome(_) => "option-is-some",
            Self::OptionUnwrapOr(..) => "option-unwrap-or",
            Self::ResultOk(_) => "result-ok",
            Self::ResultErr(_) => "result-err",
            Self::ResultIsOk(_) => "result-is-ok",
            Self::ResultUnwrapOr(..) => "result-unwrap-or",
            Self::ResultErrorOf(_) => "result-error-of",
            Self::LpMinimize(..) => "lp-minimize",
            Self::ParetoFront(_) => "pareto-front",
            Self::PolyMul(..) => "poly-mul",
            Self::PolyEval(..) => "poly-eval",
            Self::SequenceGenerate { .. } => "sequence-generate",
            Self::SequenceConvolve { .. } => "sequence-convolve",
            Self::OdeBackwardEuler(..) => "ode-backward-euler",
            Self::OdeVelocityVerlet(..) => "ode-velocity-verlet",
            Self::PoissonDirichletSine(_) => "poisson-dirichlet-sine",
            Self::ProbSample { kind, .. } => match kind {
                ProbKind::Normal => "normal-sample",
                ProbKind::Uniform => "uniform-sample",
                ProbKind::Bernoulli => "bernoulli-sample",
            },
            Self::ProbDensity { kind, .. } => match kind {
                ProbKind::Normal => "normal-density",
                ProbKind::Uniform => "uniform-density",
                ProbKind::Bernoulli => "bernoulli-pmf",
            },
            Self::ControlTransferEval(..) => "control-transfer-eval",
            Self::ControlDcGain(..) => "control-dc-gain",
            Self::ControlPolesStable(_) => "control-poles-stable",
            Self::CategoryCheck(..) => "category-check",
            Self::CategoryDiagramCommutative(..) => "category-diagram-commutative",
            Self::TensorCreate { .. } => "tensor-create",
            Self::TensorIndex { .. } => "tensor-index",
            Self::TensorSlice { .. } => "tensor-slice",
            Self::TensorAdd(..) => "tensor-add",
            Self::TensorSub(..) => "tensor-sub",
            Self::TensorScale(..) => "tensor-scale",
            Self::Einsum { .. } => "einsum",
            Self::Factorial(..) => "factorial",
            Self::ModInv(..) => "mod-inv",
            Self::IntRem(..) => "int-rem",
            Self::Congruence(..) => "congruence",
            Self::PolyEvalMod(..) => "poly-eval-mod",
            Self::RSEncode(..) => "rs-encode",
            Self::HammingDistance(..) => "hamming-distance",
            Self::Fold { .. } => "fold",
            Self::Integral { .. } => "integral",
            Self::Differentiate { .. } => "differentiate",
            Self::Solve { .. } => "solve",
            Self::Optimize { .. } => "optimize",
            Self::SampleLimit { .. } => "sample-limit",
            Self::ReverseMode { .. } => "reverse-mode",
            Self::ApplyCapability { .. } => "apply-capability",
            Self::VectorMap { .. } => "vector-map",
            Self::VectorMapScalar { .. } => "vector-map-scalar",
            Self::VectorReduce { .. } => "vector-reduce",
            Self::VectorAllFinite(_) => "vector-all-finite",
        }
    }

    /// SSA dump of this op: name, register operands, and non-register payloads.
    /// Nested sub-programs are omitted here; [`EmirProgram::print`] dumps them.
    #[must_use]
    pub fn format_ssa(&self) -> String {
        match self {
            Self::ConstF64(bits) => format!("const-f64 {bits:016x}"),
            Self::ConstI64(value) => format!("const-i64 {value}"),
            Self::ConstText(value) => format!("const-text {value:?}"),
            Self::FormatText {
                template,
                arguments,
            } => format!("format-text {template:?} {}", format_regs(arguments)),
            Self::SeriesCreate {
                points,
                interpolation,
                extrapolation,
            } => format!(
                "series-create interpolation={interpolation} extrapolation={extrapolation} points={}",
                points
                    .iter()
                    .map(|(time, value)| format!(
                        "{:016x}:{:016x}",
                        time.to_bits(),
                        value.to_bits()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::ConstBool(value) => format!("const-bool {value}"),
            Self::ConstComplex(re, im) => {
                format!("const-complex {:016x} {:016x}", re.to_bits(), im.to_bits())
            }
            Self::LoadInput(index) => format!("load-input {index}"),
            Self::LoadState(index) => format!("load-state {index}"),
            Self::Select {
                condition,
                then_value,
                else_value,
            } => format!(
                "select %{} %{} %{}",
                condition.0, then_value.0, else_value.0
            ),
            Self::MatrixCreate {
                rows,
                cols,
                elements,
            } => format!("mat-create {rows} {cols} {}", format_regs(elements)),
            Self::TensorCreate { shape, elements } => format!(
                "tensor-create {} {}",
                format_shape(shape),
                format_regs(elements)
            ),
            Self::TensorSlice { tensor, axes } => {
                let mut out = format!("tensor-slice %{}", tensor.0);
                for axis in axes {
                    match axis {
                        EmirSliceAxis::Point(v) => out.push_str(&format!(" point %{}", v.0)),
                        EmirSliceAxis::Range { start, end } => {
                            out.push_str(&format!(" range %{} %{}", start.0, end.0));
                        }
                    }
                }
                out
            }
            Self::Stencil1d {
                input,
                weights,
                center,
                edge,
            } => format!(
                "stencil-1d %{} center={center} {} {}",
                input.0,
                format_edge(edge),
                format_f64_bits(weights)
            ),
            Self::Stencil2d {
                input,
                weights,
                center,
                edge,
            } => format!(
                "stencil-2d %{} center={},{} {} {}",
                input.0,
                center.0,
                center.1,
                format_edge(edge),
                format_f64_bits(weights)
            ),
            Self::Stencil3d {
                input,
                weights,
                center,
                edge,
            } => format!(
                "stencil-3d %{} center={},{},{} {} {}",
                input.0,
                center.0,
                center.1,
                center.2,
                format_edge(edge),
                format_f64_bits(weights)
            ),
            Self::Einsum { subscripts, inputs } => {
                format!("einsum {subscripts} {}", format_regs(inputs))
            }
            Self::Fold {
                start,
                end,
                init,
                combine,
                loop_var_index,
                ..
            } => format!(
                "fold {} start=%{} end=%{} init=%{} loop={loop_var_index}",
                fold_combine_name(*combine),
                start.0,
                end.0,
                init.0
            ),
            Self::Integral {
                start,
                end,
                steps,
                loop_var_index,
                ..
            } => format!(
                "integral steps={steps} start=%{} end=%{} loop={loop_var_index}",
                start.0, end.0
            ),
            Self::Differentiate { var_index, .. } => {
                format!("differentiate var={var_index}")
            }
            Self::Solve {
                var_index,
                tolerance,
                max_iter,
                ..
            } => format!(
                "solve var={var_index} tol={:016x} max={max_iter}",
                tolerance.to_bits()
            ),
            Self::Optimize {
                var_indices,
                maximize,
                learning_rate,
                tolerance,
                max_iter,
                ..
            } => format!(
                "optimize maximize={maximize} lr={:016x} tol={:016x} max={max_iter} vars={}",
                learning_rate.to_bits(),
                tolerance.to_bits(),
                format_u16s(var_indices)
            ),
            Self::SampleLimit {
                var_index,
                target,
                direction,
                ..
            } => format!(
                "sample-limit var={var_index} target=%{} direction=%{}",
                target.0, direction.0
            ),
            Self::ReverseMode { var_indices, .. } => {
                format!("reverse-mode vars={}", format_u16s(var_indices))
            }
            Self::ApplyCapability {
                capability,
                class,
                args,
            } => format!(
                "apply-capability name={capability} class={} args={}",
                class.as_str(),
                format_regs(args)
            ),
            other => {
                let mut operands = Vec::new();
                optimize::operand_registers(other, &mut operands);
                let mut out = other.name().to_string();
                if !operands.is_empty() {
                    out.push(' ');
                    out.push_str(&format_regs(&operands));
                }
                out
            }
        }
    }
}

/// Domain obligations recorded during lowering. Phase 1 semantics: the
/// obligation is emitted as an assumption (strict-f64 IEEE behavior); no
/// silent erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainObligation {
    DivisionNonZero,
    SqrtNonNegative,
    LogPositive,
    PowFiniteResult,
}

impl DomainObligation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DivisionNonZero => "division requires a non-zero denominator",
            Self::SqrtNonNegative => "sqrt requires a non-negative argument",
            Self::LogPositive => "ln requires a strictly positive argument",
            Self::PowFiniteResult => "pow result must be finite under strict-f64 policy",
        }
    }
}

/// One lowered definition: a linear op list computing the output.
#[derive(Clone, Debug, PartialEq)]
pub struct EmirProgram {
    pub ops: Vec<(EmirOp, Span)>,
    pub result: EmirValue,
    pub input_count: u16,
    pub state_count: u16,
    pub domain_obligations: Vec<DomainObligation>,
}

impl EmirProgram {
    /// Deterministic SSA dump. Distinct register operands, constant
    /// payloads, nested bodies, counts, and obligations produce distinct
    /// bytes; `op.name()`-only dumps used to collide on those.
    #[must_use]
    pub fn print(&self) -> String {
        let mut out = String::new();
        self.write_print(&mut out, 0);
        out
    }

    fn write_print(&self, out: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        out.push_str(&pad);
        out.push_str(&format!("inputs: {}\n", self.input_count));
        out.push_str(&pad);
        out.push_str(&format!("states: {}\n", self.state_count));
        for (index, (op, _)) in self.ops.iter().enumerate() {
            out.push_str(&pad);
            out.push_str(&format!("%{index}: {}\n", op.format_ssa()));
            write_nested_programs(out, op, indent + 1);
        }
        out.push_str(&pad);
        out.push_str(&format!("result: %{}\n", self.result.0));
        for obligation in &self.domain_obligations {
            out.push_str(&pad);
            out.push_str("obligation: ");
            out.push_str(obligation.as_str());
            out.push('\n');
        }
    }
}

fn format_regs(values: &[EmirValue]) -> String {
    let mut out = String::new();
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push('%');
        out.push_str(&value.0.to_string());
    }
    out
}

fn format_shape(shape: &[usize]) -> String {
    shape
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("x")
}

fn format_u16s(values: &[u16]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_f64_bits(values: &[f64]) -> String {
    values
        .iter()
        .map(|value| format!("{:016x}", value.to_bits()))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_edge(edge: &EdgePolicy) -> String {
    match edge {
        EdgePolicy::Clamp => "clamp".to_string(),
        EdgePolicy::Neumann => "neumann".to_string(),
        EdgePolicy::OneSided => "onesided".to_string(),
        EdgePolicy::Dirichlet { left, right } => {
            format!("dirichlet {:016x} {:016x}", left.to_bits(), right.to_bits())
        }
    }
}

fn fold_combine_name(combine: FoldCombine) -> &'static str {
    match combine {
        FoldCombine::Add => "add",
        FoldCombine::Mul => "mul",
        FoldCombine::And => "and",
        FoldCombine::Or => "or",
    }
}

fn write_nested_programs(out: &mut String, op: &EmirOp, indent: usize) {
    let pad = "  ".repeat(indent);
    match op {
        EmirOp::Fold { body, .. }
        | EmirOp::Differentiate { body, .. }
        | EmirOp::Solve { body, .. }
        | EmirOp::Optimize { body, .. }
        | EmirOp::SampleLimit { body, .. }
        | EmirOp::ReverseMode { body, .. } => {
            out.push_str(&pad);
            out.push_str("body:\n");
            body.write_print(out, indent + 1);
        }
        EmirOp::Integral { integrand, .. } => {
            out.push_str(&pad);
            out.push_str("integrand:\n");
            integrand.write_print(out, indent + 1);
        }
        _ => {}
    }
}

/// Lower a Boolean requirement expression (constructor precondition).
pub fn lower_requirement(
    package: &SemanticPackage,
    expr: EmirExprRef,
    param_names: &[String],
) -> Result<EmirProgram, String> {
    let mut program = emitter::lower(package, expr, param_names, &[])?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

/// Lower a definition expression. `inputs` are declaration inputs; `states`
/// are declaration state field names (referenced as `state.<name>`).
pub fn lower_definition(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    let mut program = emitter::lower(package, expr, inputs, states)?;
    optimize::optimize_program(&mut program);
    Ok(program)
}

pub type EmirExprRef = emath_ir::ExprId;
