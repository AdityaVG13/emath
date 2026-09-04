//! Totality analysis: which ops cannot fault at runtime.

use super::*;

/// Whether evaluating this op can fault at runtime for a well-formed
/// program. Total ops are eligible for removal and for single-use
/// inlining; everything else is kept so strict eager fault semantics are
/// preserved.
pub fn is_total(op: &EmirOp, program: &EmirProgram) -> bool {
    match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => true,
        EmirOp::LoadInput(i) => usize::from(*i) < usize::from(program.input_count),
        EmirOp::LoadState(i) => usize::from(*i) < usize::from(program.state_count),
        EmirOp::F64Add(..)
        | EmirOp::F64Sub(..)
        | EmirOp::F64Mul(..)
        | EmirOp::F64Div(..)
        | EmirOp::F64Pow(..)
        | EmirOp::Neg(_)
        | EmirOp::UnaryBuiltin(..)
        | EmirOp::BinaryBuiltin(..) => true,
        EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(_)
        | EmirOp::IsFinite(_) => true,
        EmirOp::Select { .. }
        | EmirOp::FormatText { .. }
        | EmirOp::TextLength(_)
        | EmirOp::TextNfc(_)
        | EmirOp::ReportSection { .. }
        | EmirOp::ReportDocument { .. }
        | EmirOp::ReportMarkdown(_)
        | EmirOp::ReportLatex(_)
        | EmirOp::SetCreate { .. }
        | EmirOp::RecordCreate { .. } => true,
        EmirOp::VectorCreate(..)
        | EmirOp::MatrixCreate { .. }
        | EmirOp::TensorCreate { .. } => true,
        // Dynamic index bounds can fault even in well-formed programs.
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. }
        | EmirOp::SetContains { .. }
        | EmirOp::SeriesSample { .. } => false,
        // Static-shape aggregate ops: shape/type faults are excluded by the
        // typed front end.
        EmirOp::VectorAdd(..)
        | EmirOp::VectorSub(..)
        | EmirOp::VectorScale(..)
        | EmirOp::VectorDot(..)
        | EmirOp::VectorNorm(_)
        | EmirOp::VectorLength(_)
        | EmirOp::Stencil1d { .. }
        | EmirOp::Stencil2d { .. }
        | EmirOp::Stencil3d { .. }
        | EmirOp::MatrixAdd(..)
        | EmirOp::MatrixSub(..)
        | EmirOp::MatrixScale(..)
        | EmirOp::MatrixMulVector(..)
        | EmirOp::MatrixMulMatrix(..)
        | EmirOp::MatrixTranspose(_)
        // Spectral/iterative kernels refuse typed on bad input
        // (E-LINALG-001..003) — dynamic faults, like Factorial.
        | EmirOp::EigenSymmetric(_)
        | EmirOp::EigenVectorsSymmetric(_)
        | EmirOp::SvdSingularValues(_)
        | EmirOp::SvdFactors(_)
        | EmirOp::CgSolve(..)
        | EmirOp::LinearSolve(..)
        | EmirOp::LuFactors(_)
        | EmirOp::QrFactors(_)
        | EmirOp::OuterProduct(..)
        // Graph kernels refuse typed on bad input
        // (E-GRAPH-001..003) — dynamic faults, like the spectral set.
        | EmirOp::GraphReachable(..)
        | EmirOp::GraphBfsOrder(..)
        | EmirOp::GraphDijkstra(..)
        | EmirOp::GraphDegreeOut(_)
        | EmirOp::GraphLaplacian(_)
        | EmirOp::GraphSymmetrize(_)
        | EmirOp::GraphBellmanFord(..)
        | EmirOp::GraphSparseTriplets(_)
        | EmirOp::GraphSparseFromTriplets(..)
        // Exact integer nullspace: dynamic faults (E-NULLSPACE-001/002)
        // — never folded, matched only by children-preserving rewriting.
        | EmirOp::IntNullspace(_)
        // Exact integer product delta: dynamic faults
        // (E-EXACT-001/002) — never folded.
        | EmirOp::ExactProductDelta(..)
        // Optimization kernels refuse typed on bad
        // input (E-LP-001..004, E-PARETO-001) — dynamic faults, like
        // the spectral/graph sets.
        | EmirOp::LpMinimize(..)
        | EmirOp::ParetoFront(_)
        // Polynomial kernels refuse typed on
        // non-finite input (E-POLY-001/002) — dynamic faults.
        | EmirOp::PolyMul(..)
        | EmirOp::PolyEval(..)
        | EmirOp::SequenceGenerate { .. }
        | EmirOp::SequenceConvolve { .. }
        // ODE stepping refuses typed: Newton non-convergence
        // (E-ODE-001), non-advancing step (E-ODE-003), non-finite
        // carriers (E-ODE-004).
        | EmirOp::OdeBackwardEuler(..)
        | EmirOp::OdeVelocityVerlet(..)
        // Spectral Poisson refuses typed: empty interior
        // (E-PDE-001), non-finite load (E-PDE-002).
        | EmirOp::PoissonDirichletSine(_)
        // Probability ops refuse typed: invalid parameters
        // (E-PROB-001), non-finite carriers (E-PROB-002), wrong arity
        // (E-PROB-003).
        | EmirOp::ProbSample { .. }
        | EmirOp::ProbDensity { .. }
        // Control kernels (thin B43) refuse typed on bad input
        // (E-CONTROL-001..005) — dynamic faults, like the probability
        // set.
        | EmirOp::ControlTransferEval(..)
        | EmirOp::ControlDcGain(..)
        | EmirOp::ControlPolesStable(_)
        // Category kernels (thin B39) refuse typed on bad input
        // (E-CAT-001..007) — dynamic faults, like the control set.
        | EmirOp::CategoryCheck(..)
        | EmirOp::CategoryDiagramCommutative(..)
        // Option/Result semantics are TOTAL value ops — they
        // fault only via TypeConfusion on a wrong carrier shape, the
        // same dynamic-fault class as the typed-kernel sets above.
        | EmirOp::OptionSome(_)
        | EmirOp::OptionNone
        | EmirOp::OptionIsSome(_)
        | EmirOp::OptionUnwrapOr(..)
        | EmirOp::ResultOk(_)
        | EmirOp::ResultErr(_)
        | EmirOp::ResultIsOk(_)
        | EmirOp::ResultUnwrapOr(..)
        | EmirOp::ResultErrorOf(_)
        | EmirOp::TensorAdd(..)
        | EmirOp::TensorSub(..)
        | EmirOp::TensorScale(..)
        | EmirOp::Einsum { .. } => true,
        // Dynamic domain faults (factorial of a negative, non-invertible
        // modulus, congruence mod 0, ...) and runtime panics (solver
        // non-convergence).
        EmirOp::Factorial(..)
        | EmirOp::ModInv(..)
        | EmirOp::SqrtMod(..)
        | EmirOp::PowMod(..)
        | EmirOp::IntRem(..)
        | EmirOp::Congruence(..)
        | EmirOp::PolyEvalMod(..)
        | EmirOp::RSEncode(..)
        | EmirOp::HammingDistance(..) => false,
        // Higher-order drivers evaluate user bodies and can fault or
        // panic inside them.
        EmirOp::Fold { .. }
        | EmirOp::Integral { .. }
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::SampleLimit { .. }
        | EmirOp::ReverseMode { .. } => false,
        // Capability dispatch can refuse (E-CELL-006), demand a provider
        // run, or fault on arity/shape — never total.
        EmirOp::ApplyCapability { .. } => false,
        // Generic vector map/broadcast/finite-guard ops are total on any
        // (possibly empty) vector; reduce faults on an empty vector, so
        // it is kept eager like the index ops.
        EmirOp::VectorMap { .. } | EmirOp::VectorMapScalar { .. } => true,
        EmirOp::VectorReduce { .. } => false,
        EmirOp::VectorAllFinite(_) => true,
        // Certified intervals fault on ill-formed bounds and
        // intersection faults on an empty result — never total.
        EmirOp::IntervalCreate(..)
        | EmirOp::IntervalIntersect(..)
        | EmirOp::SpecialFunction { .. } => false,
        // Exact-rational cells fault
        // dynamically: zero denominator (E-RAT-001 class) and i128
        // overflow — never total, never folded here.
        EmirOp::RatConstruct { .. } | EmirOp::RatAdd(..) | EmirOp::RatNorm(_) => false,
        // A program literal is an immutable artifact value: it cannot
        // fault and always produces its carrier.
        EmirOp::ProgramLiteral(_) => true,
    }
}
