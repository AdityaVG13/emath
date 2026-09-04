//! Operand-register extraction per op.

use super::*;

/// Collect the register operands of an op (nested sub-programs excluded —
/// they number their own registers). Shared with the Rust backend for
/// single-use inlining decisions.
pub fn operand_registers(op: &EmirOp, out: &mut Vec<EmirValue>) {
    let mut push = |v: EmirValue| out.push(v);
    match *op {
        EmirOp::RatConstruct { num, den } => {
            push(num);
            push(den);
        }
        EmirOp::RatAdd(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::RatNorm(a) => push(a),
        EmirOp::F64Add(a, b)
        | EmirOp::F64Sub(a, b)
        | EmirOp::F64Mul(a, b)
        | EmirOp::F64Div(a, b)
        | EmirOp::F64Pow(a, b)
        | EmirOp::Lt(a, b)
        | EmirOp::Le(a, b)
        | EmirOp::Gt(a, b)
        | EmirOp::Ge(a, b)
        | EmirOp::Eq(a, b)
        | EmirOp::Ne(a, b)
        | EmirOp::And(a, b)
        | EmirOp::Or(a, b)
        | EmirOp::Imply(a, b)
        | EmirOp::Iff(a, b)
        | EmirOp::VectorAdd(a, b)
        | EmirOp::VectorSub(a, b)
        | EmirOp::VectorScale(a, b)
        | EmirOp::VectorDot(a, b)
        | EmirOp::MatrixAdd(a, b)
        | EmirOp::MatrixSub(a, b)
        | EmirOp::MatrixScale(a, b)
        | EmirOp::MatrixMulVector(a, b)
        | EmirOp::MatrixMulMatrix(a, b)
        | EmirOp::CgSolve(a, b)
        | EmirOp::LinearSolve(a, b)
        | EmirOp::OuterProduct(a, b)
        | EmirOp::GraphReachable(a, b)
        | EmirOp::GraphBfsOrder(a, b)
        | EmirOp::GraphDijkstra(a, b)
        | EmirOp::GraphBellmanFord(a, b)
        | EmirOp::GraphSparseFromTriplets(a, b)
        | EmirOp::PolyMul(a, b)
        | EmirOp::PolyEval(a, b)
        | EmirOp::TensorAdd(a, b)
        | EmirOp::TensorSub(a, b)
        | EmirOp::TensorScale(a, b)
        | EmirOp::ModInv(a, b)
        | EmirOp::SqrtMod(a, b)
        | EmirOp::IntRem(a, b)
        | EmirOp::HammingDistance(a, b)
        | EmirOp::BinaryBuiltin(_, a, b) => {
            push(a);
            push(b);
        }
        EmirOp::IntNullspace(a) => {
            push(a);
        }
        EmirOp::ExactProductDelta(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::SeriesSample { series, time } => {
            push(series);
            push(time);
        }
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => {
            push(initial);
            push(recurrence);
            push(budget);
        }
        EmirOp::SequenceConvolve { left, right, count } => {
            push(left);
            push(right);
            push(count);
        }
        // Three-operand ops (LP; backward Euler).
        EmirOp::LpMinimize(a, b, c) | EmirOp::OdeBackwardEuler(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Three-operand ops (control: transfer num/den/x,
        // state-space A/b/c).
        EmirOp::ControlTransferEval(a, b, c) | EmirOp::ControlDcGain(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Three-operand op (category law gate: dom, cod, comp).
        EmirOp::CategoryCheck(a, b, c) => {
            push(a);
            push(b);
            push(c);
        }
        // Four-operand op (category commutativity: dom, cod,
        // comp, faces).
        EmirOp::CategoryDiagramCommutative(a, b, c, d) => {
            push(a);
            push(b);
            push(c);
            push(d);
        }
        // Four-operand op (velocity Verlet: a, q, v, h).
        EmirOp::OdeVelocityVerlet(a, b, c, d) => {
            push(a);
            push(b);
            push(c);
            push(d);
        }
        // Three-operand op (seeded sampling: params, seed, draws).
        EmirOp::ProbSample {
            params: a,
            seed: b,
            draws: c,
            stream,
            ..
        } => {
            push(a);
            push(b);
            push(c);
            if let Some(stream) = stream {
                push(stream);
            }
        }
        // Two-operand op (density: params, x).
        EmirOp::ProbDensity {
            params: a, x: b, ..
        } => {
            push(a);
            push(b);
        }
        // Option/Result ops: constructors/polarity carry one
        // register, unwraps carry two, None carries none.
        EmirOp::OptionSome(a)
        | EmirOp::OptionIsSome(a)
        | EmirOp::ResultOk(a)
        | EmirOp::ResultErr(a)
        | EmirOp::ResultIsOk(a)
        | EmirOp::ResultErrorOf(a) => push(a),
        EmirOp::OptionUnwrapOr(a, b) | EmirOp::ResultUnwrapOr(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::OptionNone => {}
        // Single-operand op (spectral Poisson: the load).
        EmirOp::PoissonDirichletSine(a) => {
            push(a);
        }
        EmirOp::Neg(a)
        | EmirOp::UnaryBuiltin(_, a)
        | EmirOp::TextLength(a)
        | EmirOp::TextNfc(a)
        | EmirOp::ReportMarkdown(a)
        | EmirOp::ReportLatex(a)
        | EmirOp::Not(a)
        | EmirOp::IsFinite(a)
        | EmirOp::VectorNorm(a)
        | EmirOp::VectorLength(a)
        | EmirOp::MatrixTranspose(a)
        | EmirOp::EigenSymmetric(a)
        | EmirOp::EigenVectorsSymmetric(a)
        | EmirOp::SvdSingularValues(a)
        | EmirOp::SvdFactors(a)
        | EmirOp::LuFactors(a)
        | EmirOp::QrFactors(a)
        | EmirOp::GraphDegreeOut(a)
        | EmirOp::GraphLaplacian(a)
        | EmirOp::GraphSymmetrize(a)
        | EmirOp::GraphSparseTriplets(a)
        | EmirOp::ParetoFront(a)
        | EmirOp::ControlPolesStable(a)
        | EmirOp::Factorial(a) => push(a),
        EmirOp::ReportSection { heading, body } => {
            push(heading);
            push(body);
        }
        EmirOp::ReportDocument { title, section } => {
            push(title);
            push(section);
        }
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            push(condition);
            push(then_value);
            push(else_value);
        }
        EmirOp::VectorCreate(ref elements)
        | EmirOp::MatrixCreate { ref elements, .. }
        | EmirOp::TensorCreate { ref elements, .. } => {
            for &e in elements {
                push(e);
            }
        }
        EmirOp::ApplyCapability { ref args, .. } => {
            for &value in args {
                push(value);
            }
        }
        EmirOp::FormatText { ref arguments, .. } => {
            for &value in arguments {
                push(value);
            }
        }
        EmirOp::SpecialFunction { ref arguments, .. } => {
            for &value in arguments {
                push(value);
            }
        }
        EmirOp::SetCreate {
            ref elements,
            ref guards,
        } => {
            for &value in elements {
                push(value);
            }
            for &value in guards.iter().flatten() {
                push(value);
            }
        }
        EmirOp::SetContains { element, set } => {
            push(element);
            push(set);
        }
        EmirOp::RecordCreate { ref fields, .. } => {
            for (_, value) in fields {
                push(*value);
            }
        }
        EmirOp::VectorMap { source, .. } => push(source),
        EmirOp::VectorMapScalar { vector, scalar, .. } => {
            push(vector);
            push(scalar);
        }
        EmirOp::VectorReduce { source, .. } => push(source),
        EmirOp::VectorAllFinite(source) => push(source),
        EmirOp::VectorIndex { vector, index } => {
            push(vector);
            push(index);
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            push(matrix);
            push(row);
            push(col);
        }
        EmirOp::Stencil1d { input, .. }
        | EmirOp::Stencil2d { input, .. }
        | EmirOp::Stencil3d { input, .. } => push(input),
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            push(tensor);
            for &i in indices {
                push(i);
            }
        }
        EmirOp::TensorSlice { tensor, ref axes } => {
            push(tensor);
            for axis in axes {
                match *axis {
                    crate::EmirSliceAxis::Point(v) => push(v),
                    crate::EmirSliceAxis::Range { start, end } => {
                        push(start);
                        push(end);
                    }
                }
            }
        }
        EmirOp::Einsum { ref inputs, .. } => {
            for &i in inputs {
                push(i);
            }
        }
        EmirOp::Congruence(a, b, m) => {
            push(a);
            push(b);
            push(m);
        }
        EmirOp::IntervalCreate(lo, hi) => {
            push(lo);
            push(hi);
        }
        EmirOp::IntervalIntersect(a, b) => {
            push(a);
            push(b);
        }
        EmirOp::PolyEvalMod(c, x, p) | EmirOp::RSEncode(c, x, p) => {
            push(c);
            push(x);
            push(p);
        }
        EmirOp::PowMod(b, e, m) => {
            push(b);
            push(e);
            push(m);
        }
        EmirOp::Fold {
            start, end, init, ..
        } => {
            push(start);
            push(end);
            push(init);
        }
        EmirOp::Integral { start, end, .. } => {
            push(start);
            push(end);
        }
        EmirOp::SampleLimit {
            target, direction, ..
        } => {
            push(target);
            push(direction);
        }
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..)
        | EmirOp::LoadInput(_)
        | EmirOp::LoadState(_)
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::ReverseMode { .. }
        | EmirOp::ProgramLiteral(_) => {}
    }
}
