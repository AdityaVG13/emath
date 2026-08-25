//! EMIR peephole optimization: constant folding + dead-register elimination.
//!
//! Runs after lowering, before interpretation or codegen, so both consumers
//! see the same shrunk program.
//!
//! # Semantic contract
//!
//! The optimizer never changes the observable behavior of a program —
//! including its *strict eager evaluation*: the interpreter executes every
//! op in order and surfaces faults from any op, and generated Rust does the
//! same. DCE is therefore restricted to ops that are provably total for
//! well-formed programs (f64 arithmetic, builtins, static-shape aggregates).
//! Ops that can fault at runtime — number-theory ops, dynamic index
//! operations, higher-order drivers (fold/solve/optimize/...), and
//! out-of-range loads — are never removed, even when their register is
//! unused, so fault timing is preserved exactly.
//!
//! Constant folding only collapses scalar f64 chains (arithmetic + builtins
//! over constant operands), mirroring the interpreter's IEEE semantics
//! bit-exactly. Boolean registers have no constant form in EMIR, so
//! comparisons/`Select` are left alone.

use emath_core::Span;

use crate::{EmirOp, EmirProgram, EmirValue};

/// Optimize a program in place: constant-fold scalar arithmetic and
/// eliminate dead registers, recursing into nested sub-programs (fold
/// bodies, integrands, solver bodies, ...).
pub fn optimize_program(program: &mut EmirProgram) {
    // Nested bodies number their own registers; optimize them first so the
    // outer pass treats each as an opaque unit.
    for (op, _) in &mut program.ops {
        match op {
            EmirOp::Fold { body, .. }
            | EmirOp::Differentiate { body, .. }
            | EmirOp::Solve { body, .. }
            | EmirOp::Optimize { body, .. }
            | EmirOp::SampleLimit { body, .. }
            | EmirOp::ReverseMode { body, .. } => optimize_program(body),
            EmirOp::Integral { integrand, .. } => optimize_program(integrand),
            _ => {}
        }
    }
    constant_fold(program);
    dead_code_eliminate(program);
}

// ── Constant folding ─────────────────────────────────────────────────────

fn const_at(consts: &[Option<f64>], v: EmirValue) -> Option<f64> {
    consts.get(v.0 as usize).copied().flatten()
}

fn fold_binary(
    consts: &[Option<f64>],
    left: EmirValue,
    right: EmirValue,
    f: impl FnOnce(f64, f64) -> f64,
) -> Option<f64> {
    Some(f(const_at(consts, left)?, const_at(consts, right)?))
}

/// Replace provably-constant scalar ops with `ConstF64`, keeping every
/// register slot so downstream references stay valid. The interpreter's
/// IEEE behavior is matched exactly (division by zero folds to inf/NaN,
/// same as evaluating the op would).
fn constant_fold(program: &mut EmirProgram) {
    let mut consts: Vec<Option<f64>> = vec![None; program.ops.len()];
    for (i, (op, _)) in program.ops.iter_mut().enumerate() {
        let folded = match op {
            EmirOp::ConstF64(bits) => Some(f64::from_bits(*bits)),
            EmirOp::F64Add(a, b) => fold_binary(&consts, *a, *b, |x, y| x + y),
            EmirOp::F64Sub(a, b) => fold_binary(&consts, *a, *b, |x, y| x - y),
            EmirOp::F64Mul(a, b) => fold_binary(&consts, *a, *b, |x, y| x * y),
            EmirOp::F64Div(a, b) => fold_binary(&consts, *a, *b, |x, y| x / y),
            EmirOp::F64Pow(a, b) => fold_binary(&consts, *a, *b, |x, y| x.powf(y)),
            EmirOp::Neg(a) => const_at(&consts, *a).map(|x| -x),
            EmirOp::UnaryBuiltin(id, a) => const_at(&consts, *a).map(|x| id.eval_unary(x)),
            EmirOp::BinaryBuiltin(id, a, b) => {
                fold_binary(&consts, *a, *b, |x, y| id.eval_binary(x, y))
            }
            _ => None,
        };
        consts[i] = folded;
        if let Some(value) = folded {
            *op = EmirOp::ConstF64(value.to_bits());
        }
    }
}

// ── Dead-register elimination ───────────────────────────────────────────

/// Whether evaluating this op can fault at runtime for a well-formed
/// program. Total ops are eligible for removal; everything else is kept so
/// strict eager fault semantics are preserved.
fn removable(op: &EmirOp, program: &EmirProgram) -> bool {
    match op {
        EmirOp::ConstF64(_) | EmirOp::ConstI64(_) | EmirOp::ConstComplex(..) => true,
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
        EmirOp::Select { .. } => true,
        EmirOp::VectorCreate(..) | EmirOp::MatrixCreate { .. } | EmirOp::TensorCreate { .. } => {
            true
        }
        // Dynamic index bounds can fault even in well-formed programs.
        EmirOp::VectorIndex { .. }
        | EmirOp::MatrixIndex { .. }
        | EmirOp::TensorIndex { .. }
        | EmirOp::TensorSlice { .. } => false,
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
        | EmirOp::MatrixAdd(..)
        | EmirOp::MatrixSub(..)
        | EmirOp::MatrixScale(..)
        | EmirOp::MatrixMulVector(..)
        | EmirOp::MatrixMulMatrix(..)
        | EmirOp::MatrixTranspose(_)
        | EmirOp::TensorAdd(..)
        | EmirOp::TensorSub(..)
        | EmirOp::Einsum { .. } => true,
        // Dynamic domain faults (factorial of a negative, non-invertible
        // modulus, congruence mod 0, ...) and runtime panics (solver
        // non-convergence).
        EmirOp::Factorial(..)
        | EmirOp::ModInv(..)
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
    }
}

/// Collect the register operands of an op (nested sub-programs excluded —
/// they number their own registers).
fn collect_operands(op: &EmirOp, out: &mut Vec<EmirValue>) {
    let mut push = |v: EmirValue| out.push(v);
    match *op {
        EmirOp::F64Add(a, b) | EmirOp::F64Sub(a, b) | EmirOp::F64Mul(a, b)
        | EmirOp::F64Div(a, b) | EmirOp::F64Pow(a, b) | EmirOp::Lt(a, b)
        | EmirOp::Le(a, b) | EmirOp::Gt(a, b) | EmirOp::Ge(a, b) | EmirOp::Eq(a, b)
        | EmirOp::Ne(a, b) | EmirOp::And(a, b) | EmirOp::Or(a, b) | EmirOp::Imply(a, b)
        | EmirOp::Iff(a, b) | EmirOp::VectorAdd(a, b) | EmirOp::VectorSub(a, b)
        | EmirOp::VectorScale(a, b) | EmirOp::VectorDot(a, b) | EmirOp::MatrixAdd(a, b)
        | EmirOp::MatrixSub(a, b) | EmirOp::MatrixScale(a, b) | EmirOp::MatrixMulVector(a, b)
        | EmirOp::MatrixMulMatrix(a, b) | EmirOp::TensorAdd(a, b) | EmirOp::TensorSub(a, b)
        | EmirOp::ModInv(a, b) | EmirOp::HammingDistance(a, b) | EmirOp::BinaryBuiltin(_, a, b) => {
            push(a);
            push(b);
        }
        EmirOp::Neg(a) | EmirOp::UnaryBuiltin(_, a) | EmirOp::Not(a) | EmirOp::IsFinite(a)
        | EmirOp::VectorNorm(a) | EmirOp::VectorLength(a) | EmirOp::MatrixTranspose(a)
        | EmirOp::Factorial(a) => push(a),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            push(condition);
            push(then_value);
            push(else_value);
        }
        EmirOp::VectorCreate(ref elements) | EmirOp::MatrixCreate { ref elements, .. }
        | EmirOp::TensorCreate { ref elements, .. } => {
            for &e in elements {
                push(e);
            }
        }
        EmirOp::VectorIndex { vector, index } => {
            push(vector);
            push(index);
        }
        EmirOp::MatrixIndex {
            matrix,
            row,
            col,
        } => {
            push(matrix);
            push(row);
            push(col);
        }
        EmirOp::Stencil1d { input, .. } | EmirOp::Stencil2d { input, .. } => push(input),
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            push(tensor);
            for &i in indices {
                push(i);
            }
        }
        EmirOp::TensorSlice {
            tensor,
            ref axes,
        } => {
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
        EmirOp::PolyEvalMod(c, x, p) | EmirOp::RSEncode(c, x, p) => {
            push(c);
            push(x);
            push(p);
        }
        EmirOp::Fold {
            start,
            end,
            init,
            ..
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
        | EmirOp::ConstComplex(..)
        | EmirOp::LoadInput(_)
        | EmirOp::LoadState(_)
        | EmirOp::Differentiate { .. }
        | EmirOp::Solve { .. }
        | EmirOp::Optimize { .. }
        | EmirOp::ReverseMode { .. } => {}
    }
}

/// Rebuild `op` with each register operand mapped through `f`. Nested
/// sub-programs (fold bodies, integrands, solver bodies) are returned
/// unchanged.
fn remap_operands(op: &EmirOp, f: &mut impl FnMut(EmirValue) -> EmirValue) -> EmirOp {
    let mut g = |v: EmirValue| f(v);
    match *op {
        EmirOp::ConstF64(_) | EmirOp::ConstI64(_) | EmirOp::ConstComplex(..) => op.clone(),
        EmirOp::LoadInput(_) | EmirOp::LoadState(_) => op.clone(),
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
            elements: elements.iter().copied().map(g).collect(),
        },
        EmirOp::VectorIndex { vector, index } => EmirOp::VectorIndex {
            vector: g(vector),
            index: g(index),
        },
        EmirOp::MatrixIndex {
            matrix,
            row,
            col,
        } => EmirOp::MatrixIndex {
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
        EmirOp::MatrixAdd(a, b) => EmirOp::MatrixAdd(g(a), g(b)),
        EmirOp::MatrixSub(a, b) => EmirOp::MatrixSub(g(a), g(b)),
        EmirOp::MatrixScale(a, b) => EmirOp::MatrixScale(g(a), g(b)),
        EmirOp::MatrixMulVector(a, b) => EmirOp::MatrixMulVector(g(a), g(b)),
        EmirOp::MatrixMulMatrix(a, b) => EmirOp::MatrixMulMatrix(g(a), g(b)),
        EmirOp::MatrixTranspose(a) => EmirOp::MatrixTranspose(g(a)),
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => EmirOp::TensorCreate {
            shape: shape.clone(),
            elements: elements.iter().copied().map(g).collect(),
        },
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => EmirOp::TensorIndex {
            tensor: g(tensor),
            indices: indices.iter().copied().map(g).collect(),
        },
        EmirOp::TensorSlice {
            tensor,
            ref axes,
        } => EmirOp::TensorSlice {
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
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => EmirOp::Einsum {
            subscripts: subscripts.clone(),
            inputs: inputs.iter().copied().map(g).collect(),
        },
        EmirOp::Factorial(a) => EmirOp::Factorial(g(a)),
        EmirOp::ModInv(a, b) => EmirOp::ModInv(g(a), g(b)),
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
    }
}

/// Mark registers reachable from `result` as needed (backward over the
/// linear SSA list; operands always precede their user), then compact the
/// op list and renumber every live register.
fn dead_code_eliminate(program: &mut EmirProgram) {
    let n = program.ops.len();
    let mut needed = vec![false; n];
    if (program.result.0 as usize) < n {
        needed[program.result.0 as usize] = true;
    }
    let mut operands = Vec::new();
    for i in (0..n).rev() {
        if needed[i] || !removable(&program.ops[i].0, program) {
            needed[i] = true;
            operands.clear();
            collect_operands(&program.ops[i].0, &mut operands);
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
