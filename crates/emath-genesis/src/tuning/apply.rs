//! Apply-or-revert engine for world deltas.

use super::*;

pub(super) fn apply_or_revert(
    delta: &WorldDelta,
    world: &WorldIr,
    reverse: bool,
) -> Result<WorldIr, DeltaError> {
    if !reverse {
        let actual = world.identity();
        if actual != delta.base_world {
            return Err(DeltaError::BaseMismatch {
                expected: delta.base_world,
                actual,
            });
        }
    }

    let mut next = world.clone();
    if reverse {
        for change in delta.changes.iter().rev() {
            apply_change(&mut next, change, true)?;
        }
    } else {
        for change in &delta.changes {
            apply_change(&mut next, change, false)?;
        }
    }

    let identity = next.identity();
    if reverse {
        if identity != delta.base_world {
            return Err(DeltaError::DidNotRestore {
                expected: delta.base_world,
                actual: identity,
            });
        }
    } else if !delta.changes.is_empty() && identity == delta.base_world {
        return Err(DeltaError::IdentityUnchanged);
    }
    Ok(next)
}

pub(super) fn apply_change(
    world: &mut WorldIr,
    change: &SemanticChange,
    reverse: bool,
) -> Result<(), DeltaError> {
    let (expected, write) = directed_payloads(change, reverse)?;
    let target = change
        .symbol
        .as_ref()
        .ok_or_else(|| DeltaError::MissingTarget {
            kind: change.kind,
            target: String::new(),
        })?;
    match change.kind {
        SemanticVariableKind::Carrier => {
            apply_carrier(world, change.kind, &target.0, expected, write)
        }
        SemanticVariableKind::Symbol => apply_symbol(world, change.kind, target, expected, write),
        SemanticVariableKind::Signature => {
            apply_signature(world, change.kind, target, expected, write)
        }
        SemanticVariableKind::Operator => {
            apply_operator(world, change.kind, target, expected, write, false)
        }
        SemanticVariableKind::Constant => {
            apply_operator(world, change.kind, target, expected, write, true)
        }
        SemanticVariableKind::Constructor => {
            replace_list_item(&mut world.constructors, change.kind, expected, write)
        }
        SemanticVariableKind::Law => {
            replace_list_item(&mut world.laws, change.kind, expected, write)
        }
        SemanticVariableKind::Effect => {
            replace_list_item(&mut world.effects, change.kind, expected, write)
        }
    }
}

pub(super) fn directed_payloads(
    change: &SemanticChange,
    reverse: bool,
) -> Result<(Option<&str>, &str), DeltaError> {
    let (prior, next) = operational_payloads(change)?;
    if reverse {
        let prior = prior.ok_or(DeltaError::NotReversible { kind: change.kind })?;
        Ok((Some(next), prior))
    } else {
        Ok((prior, next))
    }
}
