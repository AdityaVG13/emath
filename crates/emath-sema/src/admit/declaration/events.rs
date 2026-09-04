//! Payload admission for hybrid events (moved verbatim from declaration.rs).

use super::*;

/// Payload admission for hybrid events (ch7,
/// event-execution slice). A payload suite is exactly one
/// `if <condition>:` arm whose body is exactly one assignment to a
/// declared `inputs:` / `state:` Float64 slot:
///
/// ```emath
/// events:
///     event ThresholdCrossed(voltage: Float64):
///         if charge >= capacitance * threshold_voltage:
///             voltage = 0
/// ```
///
/// Runs after definitions/equations so the condition and action may
/// reference declared definitions (inlined below, exactly like
/// residuals). Refusals are typed:
///
/// - `E-EVENT-001` — malformed payload: not a single if/assign pair,
///   indexed/dotted target, or a target that is not a declared input
///   or state slot (algebraic unknowns are refused: the Newton
///   projection owns them).
/// - `E-EVENT-002` — the condition does not infer `Bool`.
/// - `E-EVENT-003` — `else` arms: the deterministic contract is one
///   arm, one action.
/// - `E-EVENT-004` — the action value is not a numeric scalar.
/// - `E-EVENT-005` — the target slot is not a `Float64` scalar.
///
/// Bare `event Name` / `event Name(field: Type)` declarations have no
/// suite and stay surface-only (never scheduled, never refused).
pub(super) fn admit_event_payloads(
    admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    algebraic_fields: &[Field],
) {
    let Some(section) = by_name.get("events") else {
        return;
    };
    let algebraic_names: Vec<String> = algebraic_fields
        .iter()
        .map(|field| field.name.clone())
        .collect();
    for stmt in &section.suite.statements {
        let StmtKind::FnDecl {
            head,
            name,
            params,
            suite: Some(suite),
            source,
            ..
        } = &stmt.kind
        else {
            continue;
        };
        if head != "event" {
            continue;
        }
        if suite.statements.len() != 1 {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` payload must be exactly one `if <condition>:` arm with one \
                     assignment action (got {} statements)",
                    suite.statements.len()
                ),
                *source,
            );
            continue;
        }
        let StmtKind::If {
            condition,
            then,
            else_branches,
            else_tail,
        } = &suite.statements[0].kind
        else {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` payload must be `if <condition>:` (a Boolean condition \
                     and one action), not a bare statement"
                ),
                *source,
            );
            continue;
        };
        if !else_branches.is_empty() || else_tail.is_some() {
            admitter.error(
                "E-EVENT-003",
                format!(
                    "event `{name}` payload must be a SINGLE deterministic arm — no `else` \
                     branches (deterministic event execution is one condition, one action)"
                ),
                *source,
            );
            continue;
        }
        let Some((cond_id, cond_infer)) = admitter.lower_expr(condition) else {
            continue;
        };
        if !matches!(cond_infer, Infer::Bool) {
            admitter.error(
                "E-EVENT-002",
                format!("event `{name}` condition must be Boolean, inferred {cond_infer}"),
                condition.source,
            );
            continue;
        }
        if then.statements.len() != 1 {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` action must be exactly one assignment (`target = expr`), \
                     got {} statements",
                    then.statements.len()
                ),
                *source,
            );
            continue;
        }
        let StmtKind::Assign { target, value } = &then.statements[0].kind else {
            admitter.error(
                "E-EVENT-001",
                format!("event `{name}` action must be an assignment (`target = expr`)"),
                *source,
            );
            continue;
        };
        if !target.indices.is_empty() || target.segments.len() != 1 {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` action target must be a single unindexed name; indexed \
                     and dotted targets refuse (deterministic slot semantics)"
                ),
                target.source,
            );
            continue;
        }
        let target_name = &target.segments[0];
        let slot_infer = admitter
            .inputs
            .get(target_name)
            .or_else(|| admitter.states.get(target_name))
            .cloned();
        let Some(slot_infer) = slot_infer else {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` action target `{target_name}` is not a declared \
                     `inputs:` or `state:` slot"
                ),
                target.source,
            );
            continue;
        };
        if algebraic_names.iter().any(|name| name == target_name) {
            admitter.error(
                "E-EVENT-001",
                format!(
                    "event `{name}` action cannot target algebraic unknown `{target_name}` \
                     (the Newton projection owns it)"
                ),
                target.source,
            );
            continue;
        }
        if !matches!(slot_infer, Infer::F64) {
            admitter.error(
                "E-EVENT-005",
                format!(
                    "event `{name}` action target `{target_name}` must be a Float64 scalar \
                     slot, inferred {slot_infer}"
                ),
                target.source,
            );
            continue;
        }
        let Some((val_id, val_infer)) = admitter.lower_expr(value) else {
            continue;
        };
        match val_infer {
            Infer::F64 | Infer::Int | Infer::Nat => {}
            _ => {
                admitter.error(
                    "E-EVENT-004",
                    format!(
                        "event `{name}` action value must be a numeric scalar, inferred \
                         {val_infer}"
                    ),
                    value.source,
                );
                continue;
            }
        }
        let cond_id = admitter.inline_defs(cond_id);
        let val_id = admitter.inline_defs(val_id);
        admitter.events.push(EventDecl {
            name: name.clone(),
            // Runtime-captured payloads: parameter names in declaration
            // order. The runner binds each to the live value of the
            // SAME-NAMED input/state/algebraic variable at firing.
            params: params.iter().map(|param| param.name.clone()).collect(),
            condition: cond_id,
            action: EventAction {
                target: target_name.clone(),
                expr: val_id,
            },
        });
        admitter.record(
            "sema",
            format!("event `{name}` payload admitted (condition + action on `{target_name}`)"),
            *source,
        );
    }
}
