//! Result carrier checks and op-site lookup.

use super::{BackendError, EmirOp, EmirProgram, EmirValue, ValueKind, kind_at};

/// Index of `op` inside `program.ops` (pointer identity; `op` is always
/// borrowed from that slice during rendering).
pub(super) fn op_self_index(program: &EmirProgram, op: &EmirOp) -> Option<u32> {
    program
        .ops
        .iter()
        .position(|(produced, _)| std::ptr::eq(produced, op))
        .map(|i| i as u32)
}

/// Static carrier-shape check (interp `TypeConfusion` parity): a Result
/// consumer's operand must carry a Result kind, otherwise the strict
/// backend refuses typed: a `BackendError`, never a Rust panic, never a
/// silent scalar shadow.
pub(super) fn expect_result_carrier(
    program: &EmirProgram,
    value: EmirValue,
    consumer: &str,
    kinds: &[ValueKind],
) -> Result<(), BackendError> {
    let Some(producer) = program.ops.get(value.0 as usize).map(|(op, _)| op) else {
        return Err(BackendError::Lowering(format!(
            "op `{consumer}` carrier operand register {} out of range",
            value.0
        )));
    };
    if matches!(kind_at(kinds, value), ValueKind::Result(..)) {
        Ok(())
    } else {
        Err(BackendError::Lowering(format!(
            "op `{consumer}` requires a Result carrier, got register {} produced by `{}` (interp TypeConfusion parity)",
            value.0,
            producer.name()
        )))
    }
}
