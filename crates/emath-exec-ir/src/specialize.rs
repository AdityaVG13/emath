//! Static specializer with VM parity (fjxh.12).
//!
//! A fixed genome + program is a [`CompiledCell`] with some parameters
//! bound to known constants. [`specialize_cell`] is a partial evaluator
//! over that cell: bound scalar parameters become embedded constants at
//! their load sites, residual parameters renumber onto the smaller input
//! contract, and the EXISTING bit-exact folding pass (`optimize`) then
//! collapses every constant chain — the residual is static EMIR, not a
//! per-op backend (Wave 12 C8: never duplicate backends per op).
//!
//! Parity law: the specialized path executes under the same declared
//! numeric policy as the generic VM seam — the shared guard runner runs
//! first (same `E-CELL-006` refusal class), then the residual bytecode
//! under the default budget. The differential tests pin value parity and
//! refusal parity bit-for-bit; the seeded-mutant test proves the
//! differential actually discriminates.
//!
//! Zero core delta: no new op variants, no domain-named branches; the
//! specializer is a rewrite over cell data. Refusals are typed
//! ([`SpecializeError`], codes `E-SPEC-001..004`), never silent
//! specializations past the declared contract.

use std::collections::BTreeMap;
use std::fmt;

use crate::interp::{EvalFault, Value, evaluate_with_budget};
use crate::optimize;
use crate::term_compile::{ArgGuard, CompiledCell, ParamShape, run_guards};
use crate::{EmirOp, EmirProgram, EvalBudget};

/// Specialization refusal. Closed set: every variant names what was
/// wrong with the binding against the declared cell contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecializeError {
    /// A binding names a parameter outside the cell contract (the
    /// negative seed's silent-success scenario: specializing past the
    /// contract would diverge from the VM while looking successful).
    UnknownParam {
        /// The undeclared parameter name.
        name: String,
    },
    /// A bound constant is not finite (the strict-f64 policy holds at
    /// the specialization seam too).
    NonFiniteConstant {
        /// The parameter bound to a non-finite value.
        name: String,
    },
    /// A vector-shaped parameter was bound: vectors are residual inputs,
    /// not partial-evaluation constants in the closed vocabulary.
    UnsupportedShape {
        /// The parameter name.
        name: String,
        /// The declared shape token (`vector`).
        shape: &'static str,
    },
    /// A declared guard points at a parameter the specializer would
    /// bind; the guard could no longer run at the seam, so the
    /// specialization refuses instead of silently dropping an
    /// obligation.
    GuardOnConstantParam {
        /// The bound parameter index the guard targets.
        index: usize,
    },
}

impl SpecializeError {
    /// Stable diagnostic code for the refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownParam { .. } => "E-SPEC-001",
            Self::NonFiniteConstant { .. } => "E-SPEC-002",
            Self::UnsupportedShape { .. } => "E-SPEC-003",
            Self::GuardOnConstantParam { .. } => "E-SPEC-004",
        }
    }
}

impl fmt::Display for SpecializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParam { name } => write!(
                formatter,
                "{code}: binding names `{name}`, which is outside the cell's \
                 declared parameter contract",
                code = self.code()
            ),
            Self::NonFiniteConstant { name } => write!(
                formatter,
                "{code}: `{name}` is bound to a non-finite constant; the \
                 strict-f64 policy refuses at the specialization seam",
                code = self.code()
            ),
            Self::UnsupportedShape { name, shape } => write!(
                formatter,
                "{code}: `{name}` is {shape}-shaped; only scalar parameters \
                 can be bound to specialization constants",
                code = self.code()
            ),
            Self::GuardOnConstantParam { index } => write!(
                formatter,
                "{code}: a declared guard targets parameter {index}, which is \
                 bound to a constant; the guard could no longer run at the seam",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for SpecializeError {}

/// A specialized pure cell: the residual parameter contract (bound
/// scalars dropped), the guards renumbered onto the residual arguments,
/// and the static residual bytecode (constants embedded, constant chains
/// folded by the shared bit-exact pass).
#[derive(Clone, Debug)]
pub struct SpecializedCell {
    /// Capability identity carried from the source cell.
    pub capability: String,
    /// Parameters that remain runtime inputs, in argument order.
    pub residual_params: Vec<(String, ParamShape)>,
    /// Contract guards over the residual arguments, in declared order.
    pub guards: Vec<ArgGuard>,
    /// Static residual bytecode (folded, dead-register eliminated).
    pub program: EmirProgram,
}

impl SpecializedCell {
    /// Execute the specialized path with VM-seam parity: declared guards
    /// run first (the same `E-CELL-006` refusal class as the generic
    /// seam), then the residual bytecode under the default budget.
    pub fn evaluate(&self, inputs: &[Value]) -> Result<Value, EvalFault> {
        run_guards(&self.capability, &self.guards, inputs)?;
        evaluate_with_budget(&self.program, inputs, &[], EvalBudget::default())
    }
}

/// Partial-evaluate a compiled cell against known scalar constants.
///
/// Bound parameters become embedded constants; the residual program is
/// folded with the existing optimizer and re-declared over the smaller
/// input contract. Nothing else changes: the residual executes the same
/// generic vocabulary the VM seam dispatches, so a parity differential
/// against the generic path is the acceptance test.
pub fn specialize_cell(
    cell: &CompiledCell,
    bindings: &BTreeMap<String, f64>,
) -> Result<SpecializedCell, SpecializeError> {
    // 1. Validate every binding against the declared contract.
    let mut bound_at: Vec<Option<f64>> = vec![None; cell.params.len()];
    for (name, value) in bindings {
        let position = cell
            .params
            .iter()
            .position(|(param, _)| param == name)
            .ok_or_else(|| SpecializeError::UnknownParam { name: name.clone() })?;
        let (_, shape) = cell.params[position];
        if shape != ParamShape::Scalar {
            return Err(SpecializeError::UnsupportedShape {
                name: name.clone(),
                shape: shape.as_str(),
            });
        }
        if !value.is_finite() {
            return Err(SpecializeError::NonFiniteConstant { name: name.clone() });
        }
        bound_at[position] = Some(*value);
    }

    // 2. A guard over a bound parameter could no longer run at the seam:
    // refuse instead of silently dropping a declared obligation.
    for guard in &cell.guards {
        let index = match guard {
            ArgGuard::NonEmpty(index) | ArgGuard::AllFinite(index) => *index,
        };
        if bound_at.get(index).copied().unwrap_or(None).is_some() {
            return Err(SpecializeError::GuardOnConstantParam { index });
        }
    }

    // 3. Residual contract: unbound parameters keep their relative order;
    // guards renumber onto the residual argument slots.
    let mut residual_params: Vec<(String, ParamShape)> = Vec::new();
    let mut residual_index = vec![0usize; cell.params.len()];
    for (position, (name, shape)) in cell.params.iter().enumerate() {
        if bound_at[position].is_some() {
            continue;
        }
        residual_index[position] = residual_params.len();
        residual_params.push((name.clone(), *shape));
    }
    let guards = cell
        .guards
        .iter()
        .map(|guard| match guard {
            ArgGuard::NonEmpty(index) => ArgGuard::NonEmpty(residual_index[*index]),
            ArgGuard::AllFinite(index) => ArgGuard::AllFinite(residual_index[*index]),
        })
        .collect();

    // 4. Rewrite the bytecode in place: bound loads become constants at
    // their own register positions (register identity is preserved —
    // SSA ids are op positions), residual loads renumber onto the
    // residual contract.
    let mut ops = Vec::with_capacity(cell.program.ops.len());
    for (op, span) in &cell.program.ops {
        let rewritten = match op {
            EmirOp::LoadInput(index) => {
                let index = usize::from(*index);
                match bound_at.get(index).copied().flatten() {
                    Some(constant) => EmirOp::ConstF64(constant.to_bits()),
                    None => EmirOp::LoadInput(residual_index[index] as u16),
                }
            }
            other => other.clone(),
        };
        ops.push((rewritten, *span));
    }

    // 5. Fold: the shared bit-exact constant folder + dead-register
    // elimination produce the static residual (a fully bound cell
    // collapses to a single constant; a partially bound cell keeps its
    // dynamic vocabulary).
    let mut program = EmirProgram {
        ops,
        result: cell.program.result,
        input_count: residual_params.len() as u16,
        state_count: cell.program.state_count,
        domain_obligations: cell.program.domain_obligations.clone(),
    };
    optimize::optimize_program(&mut program);

    Ok(SpecializedCell {
        capability: cell.capability.clone(),
        residual_params,
        guards,
        program,
    })
}
