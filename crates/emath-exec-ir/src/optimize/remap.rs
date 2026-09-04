//! Operand remapping and dead-code elimination.

use super::*;

/// Rebuild `op` with each register operand mapped through `f`. Nested
/// sub-programs (fold bodies, integrands, solver bodies) are returned
/// unchanged.
pub(super) fn remap_operands(op: &EmirOp, f: &mut impl FnMut(EmirValue) -> EmirValue) -> EmirOp {
    let mut g = |v: EmirValue| f(v);
    match *op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBigInt(_)
        | EmirOp::ConstText(_)
        | EmirOp::SeriesCreate { .. }
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => op.clone(),
        EmirOp::LoadInput(_) | EmirOp::LoadState(_) => op.clone(),
        EmirOp::SeriesSample { series, time } => EmirOp::SeriesSample {
            series: g(series),
            time: g(time),
        },
        EmirOp::RatConstruct { num, den } => EmirOp::RatConstruct {
            num: g(num),
            den: g(den),
        },
        EmirOp::RatAdd(a, b) => EmirOp::RatAdd(g(a), g(b)),
        EmirOp::RatNorm(a) => EmirOp::RatNorm(g(a)),
        EmirOp::TextLength(text) => EmirOp::TextLength(g(text)),
        EmirOp::TextNfc(text) => EmirOp::TextNfc(g(text)),
        EmirOp::ReportSection { heading, body } => EmirOp::ReportSection {
            heading: g(heading),
            body: g(body),
        },
        EmirOp::ReportDocument { title, section } => EmirOp::ReportDocument {
            title: g(title),
            section: g(section),
        },
        EmirOp::ReportMarkdown(document) => EmirOp::ReportMarkdown(g(document)),
        EmirOp::ReportLatex(document) => EmirOp::ReportLatex(g(document)),
        EmirOp::F64Add(a, b) => EmirOp::F64Add(g(a), g(b)),
        EmirOp::F64Sub(a, b) => EmirOp::F64Sub(g(a), g(b)),
        EmirOp::F64Mul(a, b) => EmirOp::F64Mul(g(a), g(b)),
        EmirOp::F64Div(a, b) => EmirOp::F64Div(g(a), g(b)),
        EmirOp::F64Pow(a, b) => EmirOp::F64Pow(g(a), g(b)),
        EmirOp::Neg(a) => EmirOp::Neg(g(a)),
        EmirOp::UnaryBuiltin(id, a) => EmirOp::UnaryBuiltin(id, g(a)),
        EmirOp::BinaryBuiltin(id, a, b) => EmirOp::BinaryBuiltin(id, g(a), g(b)),
        EmirOp::Lt(a, b) => EmirOp::Lt(g(a), g(b)),
        EmirOp::Le(a, b) => EmirOp::Le(g(a), g(b)),
        EmirOp::Gt(a, b) => EmirOp::Gt(g(a), g(b)),
        EmirOp::Ge(a, b) => EmirOp::Ge(g(a), g(b)),
        EmirOp::Eq(a, b) => EmirOp::Eq(g(a), g(b)),
        EmirOp::Ne(a, b) => EmirOp::Ne(g(a), g(b)),
        EmirOp::And(a, b) => EmirOp::And(g(a), g(b)),
        EmirOp::Or(a, b) => EmirOp::Or(g(a), g(b)),
        EmirOp::Imply(a, b) => EmirOp::Imply(g(a), g(b)),
        EmirOp::Iff(a, b) => EmirOp::Iff(g(a), g(b)),
        EmirOp::Not(a) => EmirOp::Not(g(a)),
        EmirOp::IsFinite(a) => EmirOp::IsFinite(g(a)),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => EmirOp::Select {
            condition: g(condition),
            then_value: g(then_value),
            else_value: g(else_value),
        },
        EmirOp::VectorCreate(ref elements) => {
            EmirOp::VectorCreate(elements.iter().copied().map(g).collect())
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            ref elements,
        } => EmirOp::MatrixCreate {
            rows,
            cols,
            elements: elements.iter().map(|value| g(*value)).collect(),
        },
        EmirOp::VectorIndex { vector, index } => EmirOp::VectorIndex {
            vector: g(vector),
            index: g(index),
        },
        EmirOp::MatrixIndex { matrix, row, col } => EmirOp::MatrixIndex {
            matrix: g(matrix),
            row: g(row),
            col: g(col),
        },
        EmirOp::VectorAdd(a, b) => EmirOp::VectorAdd(g(a), g(b)),
        EmirOp::VectorSub(a, b) => EmirOp::VectorSub(g(a), g(b)),
        EmirOp::VectorScale(a, b) => EmirOp::VectorScale(g(a), g(b)),
        EmirOp::VectorDot(a, b) => EmirOp::VectorDot(g(a), g(b)),
        EmirOp::VectorNorm(a) => EmirOp::VectorNorm(g(a)),
        EmirOp::VectorLength(a) => EmirOp::VectorLength(g(a)),
        EmirOp::Stencil1d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil1d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil2d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::Stencil3d {
            input,
            ref weights,
            center,
            edge,
        } => EmirOp::Stencil3d {
            input: g(input),
            weights: weights.clone(),
            center,
            edge,
        },
        EmirOp::MatrixAdd(a, b) => EmirOp::MatrixAdd(g(a), g(b)),
        EmirOp::MatrixSub(a, b) => EmirOp::MatrixSub(g(a), g(b)),
        EmirOp::MatrixScale(a, b) => EmirOp::MatrixScale(g(a), g(b)),
        EmirOp::MatrixMulVector(a, b) => EmirOp::MatrixMulVector(g(a), g(b)),
        EmirOp::MatrixMulMatrix(a, b) => EmirOp::MatrixMulMatrix(g(a), g(b)),
        EmirOp::MatrixTranspose(a) => EmirOp::MatrixTranspose(g(a)),
        EmirOp::EigenSymmetric(a) => EmirOp::EigenSymmetric(g(a)),
        EmirOp::EigenVectorsSymmetric(a) => EmirOp::EigenVectorsSymmetric(g(a)),
        EmirOp::SvdSingularValues(a) => EmirOp::SvdSingularValues(g(a)),
        EmirOp::SvdFactors(a) => EmirOp::SvdFactors(g(a)),
        EmirOp::CgSolve(a, b) => EmirOp::CgSolve(g(a), g(b)),
        EmirOp::LinearSolve(a, b) => EmirOp::LinearSolve(g(a), g(b)),
        EmirOp::LuFactors(a) => EmirOp::LuFactors(g(a)),
        EmirOp::QrFactors(a) => EmirOp::QrFactors(g(a)),
        EmirOp::OuterProduct(a, b) => EmirOp::OuterProduct(g(a), g(b)),
        EmirOp::GraphReachable(a, b) => EmirOp::GraphReachable(g(a), g(b)),
        EmirOp::GraphBfsOrder(a, b) => EmirOp::GraphBfsOrder(g(a), g(b)),
        EmirOp::GraphDijkstra(a, b) => EmirOp::GraphDijkstra(g(a), g(b)),
        EmirOp::GraphBellmanFord(a, b) => EmirOp::GraphBellmanFord(g(a), g(b)),
        EmirOp::GraphDegreeOut(a) => EmirOp::GraphDegreeOut(g(a)),
        EmirOp::GraphLaplacian(a) => EmirOp::GraphLaplacian(g(a)),
        EmirOp::GraphSymmetrize(a) => EmirOp::GraphSymmetrize(g(a)),
        EmirOp::GraphSparseTriplets(a) => EmirOp::GraphSparseTriplets(g(a)),
        EmirOp::GraphSparseFromTriplets(a, b) => EmirOp::GraphSparseFromTriplets(g(a), g(b)),
        EmirOp::IntNullspace(a) => EmirOp::IntNullspace(g(a)),
        EmirOp::ExactProductDelta(a, b) => EmirOp::ExactProductDelta(g(a), g(b)),
        EmirOp::LpMinimize(a, b, c) => EmirOp::LpMinimize(g(a), g(b), g(c)),
        EmirOp::ParetoFront(a) => EmirOp::ParetoFront(g(a)),
        EmirOp::ControlTransferEval(a, b, c) => EmirOp::ControlTransferEval(g(a), g(b), g(c)),
        EmirOp::ControlDcGain(a, b, c) => EmirOp::ControlDcGain(g(a), g(b), g(c)),
        EmirOp::ControlPolesStable(a) => EmirOp::ControlPolesStable(g(a)),
        EmirOp::CategoryCheck(a, b, c) => EmirOp::CategoryCheck(g(a), g(b), g(c)),
        EmirOp::CategoryDiagramCommutative(a, b, c, d) => {
            EmirOp::CategoryDiagramCommutative(g(a), g(b), g(c), g(d))
        }
        EmirOp::PolyMul(a, b) => EmirOp::PolyMul(g(a), g(b)),
        EmirOp::PolyEval(a, b) => EmirOp::PolyEval(g(a), g(b)),
        EmirOp::SequenceGenerate {
            initial,
            recurrence,
            budget,
        } => EmirOp::SequenceGenerate {
            initial: g(initial),
            recurrence: g(recurrence),
            budget: g(budget),
        },
        EmirOp::SequenceConvolve { left, right, count } => EmirOp::SequenceConvolve {
            left: g(left),
            right: g(right),
            count: g(count),
        },
        EmirOp::OdeBackwardEuler(a, b, c) => EmirOp::OdeBackwardEuler(g(a), g(b), g(c)),
        EmirOp::OdeVelocityVerlet(a, b, c, d) => EmirOp::OdeVelocityVerlet(g(a), g(b), g(c), g(d)),
        EmirOp::PoissonDirichletSine(a) => EmirOp::PoissonDirichletSine(g(a)),
        EmirOp::ProbSample {
            kind,
            params,
            seed,
            draws,
            stream,
        } => EmirOp::ProbSample {
            kind,
            params: g(params),
            seed: g(seed),
            draws: g(draws),
            stream: stream.map(|value| g(value)),
        },
        EmirOp::ProbDensity { kind, params, x } => EmirOp::ProbDensity {
            kind,
            params: g(params),
            x: g(x),
        },
        EmirOp::OptionSome(a) => EmirOp::OptionSome(g(a)),
        EmirOp::OptionNone => EmirOp::OptionNone,
        EmirOp::OptionIsSome(a) => EmirOp::OptionIsSome(g(a)),
        EmirOp::OptionUnwrapOr(a, b) => EmirOp::OptionUnwrapOr(g(a), g(b)),
        EmirOp::ResultOk(a) => EmirOp::ResultOk(g(a)),
        EmirOp::ResultErr(a) => EmirOp::ResultErr(g(a)),
        EmirOp::ResultIsOk(a) => EmirOp::ResultIsOk(g(a)),
        EmirOp::ResultUnwrapOr(a, b) => EmirOp::ResultUnwrapOr(g(a), g(b)),
        EmirOp::ResultErrorOf(a) => EmirOp::ResultErrorOf(g(a)),
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => EmirOp::TensorCreate {
            shape: shape.clone(),
            elements: elements.iter().map(|value| g(*value)).collect(),
        },
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => EmirOp::TensorIndex {
            tensor: g(tensor),
            indices: indices.iter().copied().map(g).collect(),
        },
        EmirOp::TensorSlice { tensor, ref axes } => EmirOp::TensorSlice {
            tensor: g(tensor),
            axes: axes
                .iter()
                .map(|axis| match *axis {
                    crate::EmirSliceAxis::Point(v) => crate::EmirSliceAxis::Point(g(v)),
                    crate::EmirSliceAxis::Range { start, end } => crate::EmirSliceAxis::Range {
                        start: g(start),
                        end: g(end),
                    },
                })
                .collect(),
        },
        EmirOp::TensorAdd(a, b) => EmirOp::TensorAdd(g(a), g(b)),
        EmirOp::TensorSub(a, b) => EmirOp::TensorSub(g(a), g(b)),
        EmirOp::TensorScale(a, b) => EmirOp::TensorScale(g(a), g(b)),
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => EmirOp::Einsum {
            subscripts: subscripts.clone(),
            inputs: inputs.iter().copied().map(g).collect(),
        },
        EmirOp::Factorial(a) => EmirOp::Factorial(g(a)),
        EmirOp::ModInv(a, b) => EmirOp::ModInv(g(a), g(b)),
        EmirOp::SqrtMod(a, p) => EmirOp::SqrtMod(g(a), g(p)),
        EmirOp::PowMod(b, e, m) => EmirOp::PowMod(g(b), g(e), g(m)),
        EmirOp::IntRem(a, b) => EmirOp::IntRem(g(a), g(b)),
        EmirOp::Congruence(a, b, m) => EmirOp::Congruence(g(a), g(b), g(m)),
        EmirOp::PolyEvalMod(c, x, p) => EmirOp::PolyEvalMod(g(c), g(x), g(p)),
        EmirOp::RSEncode(c, n, p) => EmirOp::RSEncode(g(c), g(n), g(p)),
        EmirOp::HammingDistance(a, b) => EmirOp::HammingDistance(g(a), g(b)),
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => EmirOp::Fold {
            start: g(start),
            end: g(end),
            init: g(init),
            combine,
            loop_var_index,
            body: body.clone(),
        },
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            ref integrand,
        } => EmirOp::Integral {
            start: g(start),
            end: g(end),
            steps,
            loop_var_index,
            integrand: integrand.clone(),
        },
        EmirOp::Differentiate {
            ref body,
            var_index,
        } => EmirOp::Differentiate {
            body: body.clone(),
            var_index,
        },
        EmirOp::Solve {
            ref body,
            var_index,
            tolerance,
            max_iter,
        } => EmirOp::Solve {
            body: body.clone(),
            var_index,
            tolerance,
            max_iter,
        },
        EmirOp::Optimize {
            ref body,
            ref var_indices,
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        } => EmirOp::Optimize {
            body: body.clone(),
            var_indices: var_indices.clone(),
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        },
        EmirOp::SampleLimit {
            ref body,
            var_index,
            target,
            direction,
        } => EmirOp::SampleLimit {
            body: body.clone(),
            var_index,
            target: g(target),
            direction: g(direction),
        },
        EmirOp::ReverseMode {
            ref body,
            ref var_indices,
        } => EmirOp::ReverseMode {
            body: body.clone(),
            var_indices: var_indices.clone(),
        },
        // The nested program keeps its own register namespace; outer
        // remapping never reaches inside (same discipline as body ops).
        EmirOp::ProgramLiteral(ref program) => EmirOp::ProgramLiteral(program.clone()),
        EmirOp::ApplyCapability {
            ref capability,
            ref class,
            ref args,
        } => EmirOp::ApplyCapability {
            capability: capability.clone(),
            class: class.clone(),
            args: args.iter().copied().map(g).collect(),
        },
        EmirOp::FormatText {
            ref template,
            ref arguments,
        } => EmirOp::FormatText {
            template: template.clone(),
            arguments: arguments.iter().copied().map(g).collect(),
        },
        EmirOp::SpecialFunction {
            function,
            ref arguments,
            error_bound,
        } => EmirOp::SpecialFunction {
            function,
            arguments: arguments.iter().copied().map(g).collect(),
            error_bound,
        },
        EmirOp::SetCreate {
            ref elements,
            ref guards,
        } => EmirOp::SetCreate {
            elements: elements.iter().map(|value| g(*value)).collect(),
            guards: guards
                .iter()
                .map(|guard| guard.as_ref().map(|value| g(*value)))
                .collect(),
        },
        EmirOp::SetContains { element, set } => EmirOp::SetContains {
            element: g(element),
            set: g(set),
        },
        EmirOp::RecordCreate {
            ref type_name,
            ref fields,
        } => EmirOp::RecordCreate {
            type_name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| (name.clone(), g(*value)))
                .collect(),
        },
        EmirOp::VectorMap { builtin, source } => EmirOp::VectorMap {
            builtin,
            source: g(source),
        },
        EmirOp::VectorMapScalar { op, vector, scalar } => EmirOp::VectorMapScalar {
            op,
            vector: g(vector),
            scalar: g(scalar),
        },
        EmirOp::VectorReduce { reduce, source } => EmirOp::VectorReduce {
            reduce,
            source: g(source),
        },
        EmirOp::VectorAllFinite(source) => EmirOp::VectorAllFinite(g(source)),
        EmirOp::IntervalCreate(lo, hi) => EmirOp::IntervalCreate(g(lo), g(hi)),
        EmirOp::IntervalIntersect(a, b) => EmirOp::IntervalIntersect(g(a), g(b)),
    }
}

/// Mark registers reachable from `result` as needed (backward over the
/// linear SSA list; operands always precede their user), then compact the
/// op list and renumber every live register.
pub(super) fn dead_code_eliminate(program: &mut EmirProgram) {
    let n = program.ops.len();
    let mut needed = vec![false; n];
    if (program.result.0 as usize) < n {
        needed[program.result.0 as usize] = true;
    }
    let mut operands = Vec::new();
    for i in (0..n).rev() {
        if needed[i] || !is_total(&program.ops[i].0, program) {
            needed[i] = true;
            operands.clear();
            operand_registers(&program.ops[i].0, &mut operands);
            for v in &operands {
                if (v.0 as usize) < n {
                    needed[v.0 as usize] = true;
                }
            }
        }
    }
    // Compact: old index -> new index. Operands precede their users, so
    // the remap table is complete when each op is rebuilt in order.
    let mut remap = vec![0u32; n];
    let mut kept: Vec<(EmirOp, Span)> = Vec::with_capacity(n);
    for i in 0..n {
        if needed[i] {
            remap[i] = kept.len() as u32;
            let (op, span) = &program.ops[i];
            let mut map = |v: EmirValue| EmirValue(remap[v.0 as usize]);
            kept.push((remap_operands(op, &mut map), span.clone()));
        }
    }
    program.ops = kept;
    if (program.result.0 as usize) < n {
        program.result = EmirValue(remap[program.result.0 as usize]);
    }
}
