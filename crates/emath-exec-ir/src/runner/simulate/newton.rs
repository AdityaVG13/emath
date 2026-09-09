//! Causalized implicit-DAE Newton orchestration.
//!
//! Newton mathematics (forward-difference Jacobian, convergence budget,
//! elimination) executes from the authored reference cells
//! `std.capability.calculus.residual-newton-solve` and
//! `std.capability.calculus.residual-gaussian-solve`. Residual Euler/RK4 steps execute
//! from ONE static per-model step program shared by the VM and the
//! generated Rust backend: the program lowers every residual and
//! explicit rate definition, composes typed rate/algebraic frame
//! programs, applies the authored Newton cell at each runtime stage
//! state, and delegates the stage formulas (shifts and weights) to the
//! authored `std.capability.dynamics.model-explicit-step` capsule.
//! This module retains packing/dispatch only; no host Newton, Gaussian,
//! or stage arithmetic remains.

use crate::interp::{ProgramValue, Value, evaluate};
use crate::{EmirOp, EmirProgram, EmirValue, lower_definition};
use emath_ir::{Declaration, ModelResidual, SemanticPackage, TypeNode};
use std::collections::BTreeMap;

/// The authored residual Newton cell this module dispatches into.
pub const RESIDUAL_NEWTON_SOLVE: &str = "std.capability.calculus.residual-newton-solve";
/// The authored explicit model-step capsule (stage formulas).
pub const MODEL_EXPLICIT_STEP: &str = "std.capability.dynamics.model-explicit-step";

// ---------------------------------------------------------------------------
// Solve layouts
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SolveLayout {
    input_names: Vec<String>,
    algebraic_names: Vec<String>,
    rate_names: Vec<String>,
    state_names: Vec<String>,
    algebraic_widths: Vec<usize>,
    rate_widths: Vec<usize>,
    state_widths: Vec<usize>,
    algebraic_is_vector: Vec<bool>,
    rate_is_vector: Vec<bool>,
    state_is_vector: Vec<bool>,
}

impl SolveLayout {
    fn algebraic_total(&self) -> usize {
        self.algebraic_widths.iter().sum()
    }
    fn rate_total(&self) -> usize {
        self.rate_widths.iter().sum()
    }
    fn unknown_total(&self) -> usize {
        self.algebraic_total() + self.rate_total()
    }
    fn state_total(&self) -> usize {
        self.state_widths.iter().sum()
    }
    /// Composite vector layout: [algebraic.., rates.., state-at-point..].
    fn composite_len(&self) -> usize {
        self.unknown_total() + self.state_total()
    }
}

/// Static per-field widths from declaration types. Admission requires
/// scalar or fixed-extent-vector carriers for every Newton unknown,
/// so these offsets are compile-time constants in generated steps.
fn field_width(node: &TypeNode) -> Result<usize, String> {
    match node {
        TypeNode::Float64 | TypeNode::Nat | TypeNode::Int => Ok(1),
        TypeNode::Tensor { shape, .. } if shape.is_empty() => Ok(1),
        TypeNode::Refinement { base, .. } => field_width(base),
        TypeNode::Vector {
            extent: Some(emath_ir::Extent::Fixed(n)),
            ..
        } => Ok(*n),
        other => Err(format!(
            "model carrier must be a scalar or fixed-length vector, found {}",
            other.display_name()
        )),
    }
}

fn is_vector_carrier(node: &TypeNode) -> bool {
    match node {
        TypeNode::Float64 | TypeNode::Nat | TypeNode::Int => false,
        TypeNode::Tensor { shape, .. } if shape.is_empty() => false,
        TypeNode::Refinement { base, .. } => is_vector_carrier(base),
        TypeNode::Vector {
            extent: Some(emath_ir::Extent::Fixed(_)),
            ..
        } => true,
        _ => false,
    }
}

fn is_vector_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Vector(_) | Value::Matrix { .. } | Value::Tensor { .. }
    )
}

fn build_layout(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    algebraic_names: &[String],
    rate_names: &[String],
) -> Result<SolveLayout, String> {
    let input_names: Vec<String> = declaration.inputs.iter().map(|f| f.name.clone()).collect();
    let state_names: Vec<String> = declaration.state.iter().map(|f| f.name.clone()).collect();
    let mut algebraic_widths = Vec::new();
    let mut algebraic_is_vector = Vec::new();
    for name in algebraic_names {
        let field = declaration
            .algebraic
            .iter()
            .find(|f| &f.name == name)
            .ok_or_else(|| format!("algebraic field `{name}` is not declared"))?;
        let node = package
            .ty(field.ty)
            .ok_or_else(|| format!("unknown type for algebraic field `{name}`"))?;
        algebraic_widths.push(field_width(node)?);
        algebraic_is_vector.push(is_vector_carrier(node));
    }
    let mut rate_widths = Vec::new();
    let mut rate_is_vector = Vec::new();
    for rate in rate_names {
        let field = declaration
            .state
            .iter()
            .find(|f| &f.name == rate)
            .ok_or_else(|| format!("rate unknown `{rate}` has no state field"))?;
        let node = package
            .ty(field.ty)
            .ok_or_else(|| format!("unknown type for rate state `{rate}`"))?;
        rate_widths.push(field_width(node)?);
        rate_is_vector.push(is_vector_carrier(node));
    }
    let mut state_widths = Vec::new();
    let mut state_is_vector = Vec::new();
    for field in &declaration.state {
        let node = package
            .ty(field.ty)
            .ok_or_else(|| format!("unknown type for state `{}`", field.name))?;
        state_widths.push(field_width(node)?);
        state_is_vector.push(is_vector_carrier(node));
    }
    let _ = residuals;
    Ok(SolveLayout {
        input_names,
        algebraic_names: algebraic_names.to_vec(),
        rate_names: rate_names.to_vec(),
        state_names,
        algebraic_widths,
        rate_widths,
        state_widths,
        algebraic_is_vector,
        rate_is_vector,
        state_is_vector,
    })
}

/// Deduplicated implicit-rate names in residual scan order (the order
/// the native causal solver used for its solve vector).
fn rate_names_of(package: &SemanticPackage, declaration: &Declaration) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some(residuals) = package.residuals.get(&declaration.id) {
        for residual in residuals {
            for rate in &residual.rates {
                if !names.iter().any(|n| n == rate) {
                    names.push(rate.clone());
                }
            }
        }
    }
    names
}

fn algebraic_names_of(declaration: &Declaration) -> Vec<String> {
    declaration
        .algebraic
        .iter()
        .map(|f| f.name.clone())
        .collect()
}

/// Flat storage of one dense value (scalar/vector; matrix and tensor
/// state carriers are not admitted on the Newton path).
fn dense_storage(value: &Value) -> Result<Vec<f64>, String> {
    match value {
        Value::F64(v) => Ok(vec![*v]),
        Value::I64(v) => Ok(vec![*v as f64]),
        Value::Vector(items) => Ok(items.clone()),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => Ok(data.clone()),
        other => Err(format!("value {other:?} has no dense Float64 storage")),
    }
}

fn unflatten(data: &[f64], width: usize, is_vector: bool) -> Value {
    if width == 1 && !is_vector {
        Value::F64(data[0])
    } else {
        Value::Vector(data.to_vec())
    }
}

// ---------------------------------------------------------------------------
// Op helpers
// ---------------------------------------------------------------------------

fn push(ops: &mut Vec<(EmirOp, emath_core::Span)>, op: EmirOp) -> EmirValue {
    let value = EmirValue(ops.len() as u32);
    ops.push((op, emath_core::Span::default()));
    value
}

/// Composite vector [algebraic flat, rate zeros, state-at-point flat].
fn composite_segments(
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
    layout: &SolveLayout,
    alg_values: &[EmirValue],
    state_flat: EmirValue,
) -> Vec<EmirValue> {
    let mut segments = Vec::new();
    for (index, _) in layout.algebraic_names.iter().enumerate() {
        let width = layout.algebraic_widths[index];
        if width == 1 {
            let _ = width;
            segments.push(push(ops, EmirOp::DenseValues(alg_values[index])));
        } else {
            segments.push(push(ops, EmirOp::DenseValues(alg_values[index])));
        }
    }
    let zero = push(ops, EmirOp::ConstF64(0.0f64.to_bits()));
    for width in &layout.rate_widths {
        if *width == 1 {
            segments.push(zero);
        } else {
            let mut zeros = Vec::with_capacity(*width);
            for _ in 0..*width {
                zeros.push(zero);
            }
            segments.push(push(ops, EmirOp::VectorCreate(zeros)));
        }
    }
    segments.push(state_flat);
    segments
}

//// Flat vector segment (never a scalar): slice of `width` elements.
fn slice_segment(
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
    vector: EmirValue,
    offset: usize,
    width: usize,
) -> EmirValue {
    let start = push(ops, EmirOp::ConstI64(offset as i64));
    let count = push(ops, EmirOp::ConstI64(width as i64));
    push(
        ops,
        EmirOp::VectorSlice {
            vector,
            offset: start,
            count,
        },
    )
}
// Slice one dense field out of a flat vector at `offset`.
// Width-1 vector carriers stay vectors (VectorSlice); only declared
// scalars collapse to a scalar (VectorIndex), matching generated steps
// and the explicit unpack path.
fn field_slice(
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
    vector: EmirValue,
    offset: usize,
    width: usize,
    is_vector: bool,
) -> EmirValue {
    if width == 1 && !is_vector {
        let index = push(ops, EmirOp::ConstI64(offset as i64));
        push(ops, EmirOp::VectorIndex { vector, index })
    } else {
        let start = push(ops, EmirOp::ConstI64(offset as i64));
        let count = push(ops, EmirOp::ConstI64(width as i64));
        push(
            ops,
            EmirOp::VectorSlice {
                vector,
                offset: start,
                count,
            },
        )
    }
}

/// Residual subprogram bind list: declaration inputs, then algebraic
/// unknowns, then implicit rate unknowns (`__rate_<state>`), the frame
/// the native causal solver lowered residuals against.
fn residual_bind_names(layout: &SolveLayout) -> Vec<String> {
    let mut names = layout.input_names.clone();
    for name in &layout.algebraic_names {
        names.push(name.clone());
    }
    for rate in &layout.rate_names {
        names.push(format!("__rate_{rate}"));
    }
    names
}

/// Compose the residual evaluator body: input frame is the composite
/// vector (unknowns prefix + state tail) followed by the captured
/// declaration input values. Evaluates every residual program through
/// a CallFrame over per-slot value registers and concatenates the
/// residual components into one flat vector.
fn evaluator_ops(
    package: &SemanticPackage,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
    composite: EmirValue,
    input_regs: &[EmirValue],
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
) -> Result<EmirValue, String> {
    let bind_names = residual_bind_names(layout);
    let mut offset = 0usize;
    let mut bind_slots = Vec::with_capacity(bind_names.len());
    for slot in 0..layout.input_names.len() {
        bind_slots.push(input_regs[slot]);
    }
    for name in layout
        .algebraic_names
        .iter()
        .chain(layout.rate_names.iter())
    {
        let (width, is_vector) =
            if let Some(pos) = layout.algebraic_names.iter().position(|n| n == name) {
                (
                    layout.algebraic_widths[pos],
                    layout.algebraic_is_vector[pos],
                )
            } else {
                let pos = layout.rate_names.iter().position(|n| n == name).unwrap();
                (layout.rate_widths[pos], layout.rate_is_vector[pos])
            };
        bind_slots.push(field_slice(ops, composite, offset, width, is_vector));
        offset += width;
    }
    let mut state_slots = Vec::with_capacity(layout.state_names.len());
    let mut state_offset = layout.unknown_total();
    for (index, _) in layout.state_names.iter().enumerate() {
        let width = layout.state_widths[index];
        let is_vector = layout.state_is_vector[index];
        state_slots.push(field_slice(ops, composite, state_offset, width, is_vector));
        state_offset += width;
    }
    let mut segments = Vec::new();
    for residual in residuals {
        let body = lower_definition(package, residual.expr, &bind_names, &layout.state_names)
            .map_err(|detail| format!("residual lowering failed: {detail}"))?;
        let frame = push(
            ops,
            EmirOp::CallFrame {
                body,
                inputs: bind_slots.clone(),
                state: state_slots.clone(),
            },
        );
        // A single component may be a scalar or a one-element vector.
        segments.push(push(ops, EmirOp::DenseValues(frame)));
    }
    let result = push(ops, EmirOp::VectorConcat(segments));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Frame programs (typed residual/rate callbacks)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameOut {
    /// Flat rates in state-field order (implicit solved + explicit
    /// definition rates).
    Rates,
    /// Flat solved algebraic values in declaration order.
    Algebraic,
}

/// Append one typed frame body over a shared ops stream and return its
/// result register. Explicit input 0 is the point state (flat,
/// state-field order); captures are the residual evaluator value, the
/// declaration input values, and the algebraic guess values. The body
/// builds the composite unknown vector, applies the authored
/// residual-Newton cell, evaluates the explicit definition chain over
/// the solved algebraic values, and projects the requested output.
fn frame_tail(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
    mode: FrameOut,
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
) -> Result<EmirValue, String> {
    let point = push(ops, EmirOp::LoadInput(0));
    let evaluator = push(ops, EmirOp::LoadInput(1));
    let mut alg_guess_regs = Vec::with_capacity(layout.algebraic_names.len());
    for index in 0..layout.algebraic_names.len() {
        alg_guess_regs.push(push(
            ops,
            EmirOp::LoadInput(((2 + layout.input_names.len() + index) as u16)),
        ));
    }
    let segments = composite_segments(ops, layout, &alg_guess_regs, point);
    let composite = push(ops, EmirOp::VectorConcat(segments));
    let count = push(ops, EmirOp::ConstI64(layout.unknown_total() as i64));
    let solved = push(
        ops,
        EmirOp::ApplyCapability {
            capability: RESIDUAL_NEWTON_SOLVE.to_string(),
            class: crate::CellClass::Pure,
            args: vec![evaluator, composite, count],
        },
    );
    match mode {
        FrameOut::Algebraic => {
            let mut segments = Vec::new();
            let mut offset = 0usize;
            for width in &layout.algebraic_widths {
                segments.push(slice_segment(ops, solved, offset, *width));
                offset += width;
            }
            Ok(push(ops, EmirOp::VectorConcat(segments)))
        }
        FrameOut::Rates => {
            rate_segments(package, declaration, residuals, layout, ops, point, solved)
        }
    }
}

/// Per-state rate segments in declaration order (implicit solved rates
/// slice the solved composite; explicit definition rates run the
/// definition chain over solved algebraic values and the point state).
fn rate_segments(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
    ops: &mut Vec<(EmirOp, emath_core::Span)>,
    point: EmirValue,
    solved: EmirValue,
) -> Result<EmirValue, String> {
    let _ = residuals;
    let mut implicit_offsets: BTreeMap<usize, usize> = BTreeMap::new();
    let mut acc = layout.algebraic_total();
    for (index, name) in layout.rate_names.iter().enumerate() {
        let state_index = layout
            .state_names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("rate `{name}` has no state field"))?;
        implicit_offsets.insert(state_index, acc);
        acc += layout.rate_widths[index];
    }

    // Definition chain in source order (native definition semantics):
    // every definition evaluates over inputs, algebraic unknowns, and
    // earlier definitions, reading state fields at the point.
    let order = crate::definition_order(package, declaration);
    let mut def_names: Vec<String> = Vec::new();
    let mut def_regs: Vec<EmirValue> = Vec::new();
    let mut state_args = Vec::with_capacity(layout.state_names.len());
    let mut state_offset = 0usize;
    for (index, width) in layout.state_widths.iter().enumerate() {
        let is_vector = layout.state_is_vector[index];
        state_args.push(field_slice(ops, point, state_offset, *width, is_vector));
        state_offset += width;
    }
    for (name, expr) in &order {
        let name = (*name).clone();
        let expr = *expr;
        let mut bind_names = layout.input_names.clone();
        bind_names.extend(layout.algebraic_names.iter().cloned());
        bind_names.extend(def_names.iter().cloned());
        let body = lower_definition(package, expr, &bind_names, &layout.state_names)
            .map_err(|detail| format!("rate definition lowering failed: {detail}"))?;
        let mut inputs = Vec::with_capacity(bind_names.len());
        for (index, bind) in bind_names.iter().enumerate() {
            if index < layout.input_names.len() {
                inputs.push(push(ops, EmirOp::LoadInput((2 + index) as u16)));
            } else if let Some(pos) = layout.algebraic_names.iter().position(|n| n == bind) {
                let off = layout.algebraic_widths[..pos].iter().sum::<usize>();
                inputs.push(field_slice(
                    ops,
                    solved,
                    off,
                    layout.algebraic_widths[pos],
                    layout.algebraic_is_vector[pos],
                ));
            } else if let Some(pos) = def_names.iter().position(|n| n == bind) {
                inputs.push(def_regs[pos]);
            } else {
                return Err(format!(
                    "definition `{name}` binds an unknown name `{bind}`"
                ));
            }
        }
        let value = push(
            ops,
            EmirOp::CallFrame {
                body,
                inputs,
                state: state_args.clone(),
            },
        );
        def_names.push(name);
        def_regs.push(value);
    }

    let mut segments = Vec::with_capacity(layout.state_names.len());
    for (state_index, state) in layout.state_names.iter().enumerate() {
        if let Some(&offset) = implicit_offsets.get(&state_index) {
            let width = layout.state_widths[state_index];
            segments.push(slice_segment(ops, solved, offset, width));
            continue;
        }
        let def_name = format!("der_{state}");
        let def_index = def_names
            .iter()
            .position(|n| n == &def_name)
            .ok_or_else(|| format!("missing rate `der_{state}`"))?;
        let value = def_regs[def_index];
        let template = push(ops, EmirOp::DenseLayout(state_args[state_index]));
        let condition = push(ops, EmirOp::SameDenseShape(template, value));
        let then_body = EmirProgram {
            ops: vec![
                (EmirOp::LoadInput(0), Default::default()),
                (EmirOp::DenseValues(EmirValue(0)), Default::default()),
            ],
            result: EmirValue(1),
            input_count: 1,
            state_count: 0,
            domain_obligations: Vec::new(),
        };
        let else_body = EmirProgram {
            ops: vec![(
                EmirOp::Refuse(
                    "state and rate must have the same scalar/vector/matrix/tensor shape".into(),
                ),
                Default::default(),
            )],
            result: EmirValue(0),
            input_count: 1,
            state_count: 0,
            domain_obligations: Vec::new(),
        };
        segments.push(push(
            ops,
            EmirOp::Branch {
                condition,
                args: vec![value],
                then_body,
                else_body,
            },
        ));
    }
    Ok(push(ops, EmirOp::VectorConcat(segments)))
}

/// Build the typed frame program (rate or algebraic frame) over the
/// given capture registers of the outer program.
fn build_frame(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
    mode: FrameOut,
    captures: &[EmirValue],
) -> Result<EmirProgram, String> {
    let mut ops = Vec::new();
    let result = frame_tail(package, declaration, residuals, layout, mode, &mut ops)?;
    Ok(EmirProgram {
        ops,
        result,
        input_count: u16::try_from(1 + captures.len()).unwrap_or(u16::MAX),
        state_count: 0,
        domain_obligations: Vec::new(),
    })
}

/// Compose the residual evaluator program value for a solve at one
/// point: explicit input 0 is the composite unknown vector; captures
/// are the declaration input values.
fn evaluator_literal_body(
    package: &SemanticPackage,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
) -> Result<EmirProgram, String> {
    let mut ops = Vec::new();
    let composite = push(&mut ops, EmirOp::LoadInput(0));
    let mut input_regs = Vec::with_capacity(layout.input_names.len());
    for index in 0..layout.input_names.len() {
        input_regs.push(push(&mut ops, EmirOp::LoadInput((1 + index) as u16)));
    }
    let result = evaluator_ops(package, residuals, layout, composite, &input_regs, &mut ops)?;
    Ok(EmirProgram {
        ops,
        result,
        input_count: u16::try_from(1 + layout.input_names.len()).unwrap_or(u16::MAX),
        state_count: 0,
        domain_obligations: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Static residual step program (VM and generated share this)
// ---------------------------------------------------------------------------

/// Residual-model step program, shared by the interpreter and the
/// generated Rust steps. Input frame: declaration inputs then `dt`.
/// State frame: differential state values (declaration order) then
/// algebraic values (declaration order). Output: flat storage of the
/// accepted differential state followed by the solved algebraic values,
/// both in declaration order.

// ---------------------------------------------------------------------------
// Map-level causal solve (packing/dispatch for implicit.rs callers)
// ---------------------------------------------------------------------------

/// Solve a model residual system at the current state (map API used by
/// projection, backward-Euler, Verlet, and RK45 rate evaluation). The
/// authored residual-Newton cell performs the iteration; this wrapper
/// packs the unknown composite, applies the cell, and unpacks solved
/// algebraic values and `der_<state>` rates.
pub fn causal_newton(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    residuals: &[ModelResidual],
    algebraic_names: &[String],
    rate_names: &[String],
) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>), String> {
    let input_names: Vec<String> = declaration.inputs.iter().map(|f| f.name.clone()).collect();
    let state_names: Vec<String> = declaration.state.iter().map(|f| f.name.clone()).collect();
    for name in &input_names {
        if !inputs.contains_key(name) {
            return Err(format!("missing input `{name}`"));
        }
    }
    let mut unknown_widths = Vec::new();
    let mut unknown_is_vector = Vec::new();
    let mut composite: Vec<f64> = Vec::new();
    for name in algebraic_names {
        let value = state
            .get(name)
            .or_else(|| inputs.get(name))
            .ok_or_else(|| {
                format!("missing algebraic-variable guess `{name}` in the simulate inputs map")
            })?;
        let width = value_width(value, name)?;
        unknown_is_vector.push(is_vector_value(value));
        unknown_widths.push(width);
        composite.extend(dense_storage(value)?);
    }
    for rate in rate_names {
        let (width, is_vector) = match state.get(rate) {
            Some(Value::F64(_)) => (1, false),
            Some(Value::Vector(items)) => (items.len(), true),
            other => {
                return Err(format!(
                    "rate unknown `der({rate})` needs a scalar or vector state, found {other:?}"
                ));
            }
        };
        unknown_is_vector.push(is_vector);
        unknown_widths.push(width);
        composite.extend(std::iter::repeat(0.0).take(width));
    }
    let count: usize = unknown_widths.iter().sum();
    let mut state_widths = Vec::with_capacity(state_names.len());
    let mut state_is_vector = Vec::with_capacity(state_names.len());
    for name in &state_names {
        let value = state
            .get(name)
            .ok_or_else(|| format!("missing state `{name}`"))?;
        let data = dense_storage(value)?;
        state_widths.push(data.len());
        state_is_vector.push(is_vector_value(value));
        composite.extend(data);
    }
    let layout = SolveLayout {
        input_names,
        algebraic_names: algebraic_names.to_vec(),
        rate_names: rate_names.to_vec(),
        state_names,
        algebraic_widths: unknown_widths[..algebraic_names.len()].to_vec(),
        rate_widths: unknown_widths[algebraic_names.len()..].to_vec(),
        state_widths,
        algebraic_is_vector: unknown_is_vector[..algebraic_names.len()].to_vec(),
        rate_is_vector: unknown_is_vector[algebraic_names.len()..].to_vec(),
        state_is_vector,
    };
    let solved = run_map_solve(package, residuals, &layout, inputs, composite, count)?;
    let mut algebraic_solved = BTreeMap::new();
    let mut offset = 0usize;
    for (index, name) in algebraic_names.iter().enumerate() {
        let width = layout.algebraic_widths[index];
        let is_vector = layout.algebraic_is_vector[index];
        algebraic_solved.insert(
            name.clone(),
            slice_to_value(&solved, offset, width, is_vector),
        );
        offset += width;
    }
    let mut rate_solved = BTreeMap::new();
    for (index, rate) in rate_names.iter().enumerate() {
        let width = layout.rate_widths[index];
        let is_vector = layout.rate_is_vector[index];
        rate_solved.insert(
            format!("der_{rate}"),
            slice_to_value(&solved, offset, width, is_vector),
        );
        offset += width;
    }
    if offset > solved.len() {
        return Err("internal: Newton solve vector has the wrong width".to_string());
    }
    Ok((algebraic_solved, rate_solved))
}

fn run_map_solve(
    package: &SemanticPackage,
    residuals: &[ModelResidual],
    layout: &SolveLayout,
    inputs: &BTreeMap<String, Value>,
    composite: Vec<f64>,
    count: usize,
) -> Result<Vec<f64>, String> {
    use crate::interp::EvalFault;
    let evaluator_body = evaluator_literal_body(package, residuals, layout)?;
    let captures: Vec<Value> = layout
        .input_names
        .iter()
        .map(|name| {
            inputs
                .get(name)
                .cloned()
                .ok_or_else(|| format!("missing input `{name}`"))
        })
        .collect::<Result<_, _>>()?;
    let evaluator = Value::Program(ProgramValue {
        body: evaluator_body,
        captures,
        vector_input: true,
    });
    let count_value = Value::I64(count as i64);
    let mut ops = Vec::new();
    let mut load = |index: usize| {
        let value = EmirValue(ops.len() as u32);
        ops.push((EmirOp::LoadInput(index as u16), Default::default()));
        value
    };
    let x0 = load(0);
    let program = load(1);
    let cnt = load(2);
    let result = EmirValue(ops.len() as u32);
    ops.push((
        EmirOp::ApplyCapability {
            capability: RESIDUAL_NEWTON_SOLVE.to_string(),
            class: crate::CellClass::Pure,
            args: vec![program, x0, cnt],
        },
        Default::default(),
    ));
    let program = EmirProgram {
        ops,
        result,
        input_count: 3,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    match evaluate(
        &program,
        &[Value::Vector(composite), evaluator, count_value],
        &[],
    ) {
        Ok(Value::Vector(solved)) => Ok(solved),
        Ok(other) => Err(format!("residual Newton solve returned {other:?}")),
        Err(EvalFault::CarrierRefused { detail, .. }) => Err(detail),
        Err(fault) => Err(format!("residual Newton solve fault: {fault:?}")),
    }
}

fn value_width(value: &Value, name: &str) -> Result<usize, String> {
    match value {
        Value::F64(_) | Value::I64(_) => Ok(1),
        Value::Vector(items) => Ok(items.len()),
        _ => Err(format!(
            "algebraic variable `{name}` must be a scalar or vector, found {value:?}"
        )),
    }
}

fn slice_to_value(solved: &[f64], offset: usize, width: usize, is_vector: bool) -> Value {
    if width == 1 && !is_vector {
        Value::F64(solved[offset])
    } else {
        Value::Vector(solved[offset..offset + width].to_vec())
    }
}

// ---------------------------------------------------------------------------
// VM step entry (Main API routing)
// ---------------------------------------------------------------------------

/// Residual-model Euler/RK4 step for Main API routing. Evaluates the
/// shared static step program (authored stage formulas and authored
/// Newton solves) and unpacks the accepted differential state plus the
/// projected algebraic values. Guards and refusal texts match the
/// explicit-path contract.
pub fn authored_implicit_explicit_step(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    method: super::types::StepMethod,
) -> Result<BTreeMap<String, Value>, String> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(format!(
            "E-ODE-003: step size must be a positive finite Float64 (a non-advancing step must never return the input as an integrated value), got {dt}"
        ));
    }
    let residuals: Vec<ModelResidual> = package
        .residuals
        .get(&declaration.id)
        .cloned()
        .unwrap_or_default();
    let rk4 = matches!(method, super::types::StepMethod::Rk4);
    let program = residual_model_step_program(package, declaration, &residuals, rk4)?;
    let mut args = Vec::with_capacity(declaration.inputs.len() + 1);
    for field in &declaration.inputs {
        let value = inputs
            .get(&field.name)
            .cloned()
            .ok_or_else(|| format!("missing input `{}`", field.name))?;
        args.push(coerce_slot(package, field.ty, value));
    }
    args.push(Value::F64(dt));
    let mut frame = Vec::with_capacity(declaration.state.len() + declaration.algebraic.len());
    for field in declaration.state.iter().chain(declaration.algebraic.iter()) {
        let value = state
            .get(&field.name)
            .cloned()
            .or_else(|| inputs.get(&field.name).cloned())
            .ok_or_else(|| format!("missing state `{}`", field.name))?;
        frame.push(coerce_slot(package, field.ty, value));
    }
    let values = evaluate(&program, &args, &frame).map_err(|fault| match fault {
        crate::interp::EvalFault::CarrierRefused { detail, .. } => detail,
        fault => fault.to_string(),
    })?;
    unpack_step(package, declaration, values)
}

fn coerce_slot(package: &SemanticPackage, ty: emath_ir::TypeId, value: Value) -> Value {
    super::super::eval::coerce_to_slot(value, package.ty(ty))
}

/// Unpack the step-program output: accepted differential storage then
/// solved algebraic storage, both in declaration order, back to a
/// value map (type-derived static widths).
fn unpack_step(
    package: &SemanticPackage,
    declaration: &Declaration,
    result: Value,
) -> Result<BTreeMap<String, Value>, String> {
    let Value::Vector(data) = result else {
        return Err("residual step did not return Float64 storage".to_string());
    };
    let mut next = BTreeMap::new();
    let mut offset = 0usize;
    for field in declaration.state.iter().chain(declaration.algebraic.iter()) {
        let node = package
            .ty(field.ty)
            .ok_or_else(|| format!("unknown type for field `{}`", field.name))?;
        let width = field_width(node)?;
        let is_vector = is_vector_carrier(node);
        let end = offset
            .checked_add(width)
            .ok_or_else(|| "residual step storage length exceeds usize".to_string())?;
        let slice = data
            .get(offset..end)
            .ok_or_else(|| "residual step storage does not match the declaration".to_string())?;
        next.insert(field.name.clone(), unflatten(slice, width, is_vector));
        offset = end;
    }
    if offset != data.len() {
        return Err("residual step storage has trailing data".to_string());
    }
    Ok(next)
}

// ---------------------------------------------------------------------------
// Static residual step program (VM and generated share this)
// ---------------------------------------------------------------------------

/// Residual-model step program, shared by the interpreter and the
/// generated Rust steps. Input frame: declaration inputs then `dt`.
/// State frame: differential state values (declaration order) then
/// algebraic values (declaration order). Output: flat storage of the
/// accepted differential state followed by the solved algebraic values,
/// both in declaration order.
pub fn residual_model_step_program(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    rk4: bool,
) -> Result<EmirProgram, String> {
    let layout = build_layout(
        package,
        declaration,
        residuals,
        &algebraic_names_of(declaration),
        &rate_names_of(package, declaration),
    )?;
    let input_count = layout.input_names.len() + 1;
    let state_count = layout.state_names.len() + layout.algebraic_names.len();
    let mut ops = Vec::new();
    let mut input_regs = Vec::with_capacity(layout.input_names.len());
    for index in 0..layout.input_names.len() {
        input_regs.push(push(&mut ops, EmirOp::LoadInput(index as u16)));
    }
    let dt = push(&mut ops, EmirOp::LoadInput(layout.input_names.len() as u16));
    let mut state_regs = Vec::with_capacity(state_count);
    for index in 0..state_count {
        state_regs.push(push(&mut ops, EmirOp::LoadState(index as u16)));
    }
    let mut segments = Vec::new();
    for reg in state_regs.iter().take(layout.state_names.len()) {
        segments.push(push(&mut ops, EmirOp::DenseValues(*reg)));
    }
    let state_flat = push(&mut ops, EmirOp::VectorConcat(segments));
    let evaluator_body = evaluator_literal_body(package, residuals, &layout)?;
    let evaluator = push(
        &mut ops,
        EmirOp::ProgramLiteral {
            body: evaluator_body,
            captures: input_regs.clone(),
            vector_input: true,
        },
    );
    let mut frame_captures = Vec::new();
    frame_captures.push(evaluator);
    frame_captures.extend(input_regs.iter().copied());
    frame_captures.extend(state_regs[layout.state_names.len()..].iter().copied());
    let rate_body = build_frame(
        package,
        declaration,
        residuals,
        &layout,
        FrameOut::Rates,
        &frame_captures,
    )?;
    let algebraic_body = build_frame(
        package,
        declaration,
        residuals,
        &layout,
        FrameOut::Algebraic,
        &frame_captures,
    )?;
    let rate_frame = push(
        &mut ops,
        EmirOp::ProgramLiteral {
            body: rate_body,
            captures: frame_captures.clone(),
            vector_input: true,
        },
    );
    let algebraic_frame = push(
        &mut ops,
        EmirOp::ProgramLiteral {
            body: algebraic_body,
            captures: frame_captures,
            vector_input: true,
        },
    );
    let k1 = push(
        &mut ops,
        EmirOp::CallProgram {
            program: rate_frame,
            inputs: state_flat,
        },
    );
    let method = push(&mut ops, EmirOp::ConstBool(rk4));
    let profile = push(&mut ops, EmirOp::ConstBool(false));
    let advanced = push(
        &mut ops,
        EmirOp::ApplyCapability {
            capability: MODEL_EXPLICIT_STEP.to_string(),
            class: crate::CellClass::Pure,
            args: vec![rate_frame, state_flat, k1, dt, method, profile],
        },
    );
    let algebraic = push(
        &mut ops,
        EmirOp::CallProgram {
            program: algebraic_frame,
            inputs: advanced,
        },
    );
    let result = push(&mut ops, EmirOp::VectorConcat(vec![advanced, algebraic]));
    Ok(EmirProgram {
        ops,
        result,
        input_count: input_count as u16,
        state_count: state_count as u16,
        domain_obligations: Vec::new(),
    })
}
