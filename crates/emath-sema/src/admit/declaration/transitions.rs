//! Structural admission + SIR lowering for the `transitions:` section
//! (moved verbatim from declaration.rs).

use emath_core::tree::ExprKind;

use super::*;

/// Structural admission + SIR lowering for the `transitions:` section
/// (ch7, transitions slice — ). A transition
/// rule maps a declared event to re-assignments of input/state slots:
///
/// ```emath
/// transitions:
///     on EventName:
///         state.x = v
///         voltage = 0
/// ```
///
/// Each rule body is one or more single Assigned statements; the target
/// is a `state.<name>` path or a bare declared input/state name; the
/// value is any numeric `.emath` expression (which may reference the
/// trigger's own event parameters, in scope only inside that rule).
/// Each action lowers to a kept `ExprId` (`TransitionAction`) with
/// definitions inlined, and the rule becomes a `TransitionDecl` on the
/// admitter (the runner — owns execution). Mirrors
/// `admit_event_payloads`' kind policy: no kind gate.
///
/// Refusals are typed:
///
/// - `E-TRANS-001` — `on <Event>:` names an event not declared in
///   `events:` (or there is no `events:` section).
/// - `E-TRANS-002` — the action target is not a declared input/state
///   slot (bare unknown name, dotted `state.<missing>`, deep path).
/// - `E-TRANS-003` — a rule body is not an assignment (nested section),
///   `on` lacks an event name, or the action value is non-numeric.
/// - `E-TRANS-004` — `on <Event>:` has an empty body.
/// - `E-TRANS-005` — the action targets an `algebraic:` unknown (the
///   Newton projection owns those).
/// - `E-TRANS-006` — an event parameter name matches NO declared
///   input/state/algebraic variable, so no payload value can be
///   captured at firing (binding is undefined). Names the param and the
///   event.
///
/// Events are declared and their parameters type-checked even when no
/// `transitions:` rules exist (the events block runs before the gate).
pub(super) fn admit_transitions(
    admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    algebraic_fields: &[Field],
) {
    // Events are Phase-1 sections in their own right: declare them (and
    // type-check their parameters) whether or not `transitions:` rules
    // exist — event parameter types pass the same gate as every other
    // type site (bare `Real` refuses here too, never silently f64).
    // Both `event <Name>` FnDecl heads and no-arg `event <Name>` commands
    // count; the events block already refuses duplicates (E-NAME-022).
    let mut event_params: BTreeMap<String, Vec<(String, Infer)>> = BTreeMap::new();
    if let Some(events) = by_name.get("events") {
        for stmt in &events.suite.statements {
            match &stmt.kind {
                StmtKind::FnDecl {
                    head, name, params, ..
                } if head == "event" => {
                    let typed: Vec<(String, Infer)> = params
                        .iter()
                        .map(|param| {
                            let infer = map_type(
                                &param.ty,
                                &mut admitter.diagnostics,
                                &admitter.host_types,
                            )
                            .map(|node| infer_from_node(&node))
                            .unwrap_or(Infer::F64);
                            (param.name.clone(), infer)
                        })
                        .collect();
                    // E-TRANS-006: a captured event parameter must have a
                    // source value. It binds to the live value of the
                    // SAME-NAMED input/state/algebraic variable at the
                    // firing sample (admitter.inputs already merges
                    // algebraic unknowns); otherwise no payload value can
                    // be captured and binding is undefined.
                    for (param_name, _) in &typed {
                        if !admitter.inputs.contains_key(param_name)
                            && !admitter.states.contains_key(param_name)
                        {
                            admitter.error(
                                "E-TRANS-006",
                                format!(
                                    "event parameter `{param_name}` of event `{name}` is not a \
                                     declared model variable (input, state, or algebraic), so no \
                                     payload value can be captured at firing — declare it as an \
                                     input/state/algebraic variable or drop the parameter"
                                ),
                                stmt.source,
                            );
                        }
                    }
                    event_params.insert(name.clone(), typed);
                }
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("event") && head.len() == 2 =>
                {
                    event_params.insert(head[1].clone(), Vec::new());
                }
                _ => {}
            }
        }
    }

    let Some(section) = by_name.get("transitions") else {
        return;
    };
    let algebraic_names: Vec<String> = algebraic_fields
        .iter()
        .map(|field| field.name.clone())
        .collect();

    for rule_stmt in &section.suite.statements {
        let StmtKind::Section(rule) = &rule_stmt.kind else {
            admitter.error(
                "E-TRANS-003",
                "a `transitions:` rule must be `on <Event>:` with a name; bare statements refuse",
                rule_stmt.source,
            );
            continue;
        };
        if rule.name != "on" {
            admitter.error(
                "E-TRANS-003",
                format!(
                    "a `transitions:` rule must be `on <Event>:`, found `{}:`",
                    rule.name
                ),
                rule.head_source,
            );
            continue;
        }
        let Some(trigger) = &rule.generic else {
            admitter.error(
                "E-TRANS-003",
                "`on` in `transitions:` requires an event name after the colon (`on <Event>:`)",
                rule.head_source,
            );
            continue;
        };
        if !event_params.contains_key(trigger) {
            admitter.error(
                "E-TRANS-001",
                format!(
                    "`on {trigger}:` names event `{trigger}`, which is not declared in this \
                     declaration's `events:` section"
                ),
                rule.head_source,
            );
            continue;
        }
        let actions = &rule.suite.statements;
        if actions.is_empty() {
            admitter.error(
                "E-TRANS-004",
                format!("`on {trigger}:` requires at least one assignment action"),
                rule.source,
            );
            continue;
        }
        // Event parameters are in scope only for this rule's actions; the
        // admitter's own params map holds the pre-existing definition
        // environment, so save/restore around the rule.
        let saved_params = admitter.params.clone();
        for (name, infer) in &event_params[trigger] {
            admitter.params.insert(name.clone(), infer.clone());
        }
        // Lower each action to a kept `TransitionAction`; definitions
        // are inlined exactly like residuals/events. A referenced event
        // parameter stays a plain `Variable` node (a runtime capture name
        // the runner binds at firing).
        let mut rule_actions: Vec<TransitionAction> = Vec::new();
        for action in actions {
            // An action is an assignment in either parse shape: a bare
            // `name = <expr>` row is StmtKind::Assign; a dotted
            // `state.<name> = <expr>` row is StmtKind::Equation (the
            // ident-led expression path). Both are one target + one
            // numeric value.
            let (target_name, is_state_path, value) = match &action.kind {
                StmtKind::Assign { target, value } => {
                    if target.segments.len() == 1 {
                        (&target.segments[0], false, value)
                    } else if target.segments.len() == 2 && target.segments[0] == "state" {
                        (&target.segments[1], true, value)
                    } else {
                        admitter.error(
                            "E-TRANS-002",
                            format!(
                                "`on {trigger}:` action target `{}` is not a declared input \
                                 or state slot",
                                target.segments.join(".")
                            ),
                            target.source,
                        );
                        continue;
                    }
                }
                StmtKind::Equation { left, right } => {
                    let ExprKind::Path { segments, .. } = &left.kind else {
                        admitter.error(
                            "E-TRANS-002",
                            format!(
                                "`on {trigger}:` action target must be a plain `state.<name>` \
                                 or `<name>` slot, not an expression"
                            ),
                            left.source,
                        );
                        continue;
                    };
                    if segments.len() == 2 && segments[0] == "state" {
                        (&segments[1], true, right)
                    } else if segments.len() == 1 {
                        (&segments[0], false, right)
                    } else {
                        admitter.error(
                            "E-TRANS-002",
                            format!(
                                "`on {trigger}:` action target `{}` is not a declared input \
                                 or state slot",
                                segments.join(".")
                            ),
                            left.source,
                        );
                        continue;
                    }
                }
                _ => {
                    admitter.error(
                        "E-TRANS-003",
                        format!(
                            "`on {trigger}:` actions must be assignments \
                             (`state.<name> = <expr>` or `<name> = <expr>`), not another statement shape"
                        ),
                        action.source,
                    );
                    continue;
                }
            };
            if let StmtKind::Assign { target, .. } = &action.kind
                && !target.indices.is_empty()
            {
                admitter.error(
                    "E-TRANS-002",
                    format!(
                        "`on {trigger}:` action target must be a plain `state.<name>` or \
                         `<name>` slot (indexed targets refuse)"
                    ),
                    target.source,
                );
                continue;
            }
            if algebraic_names.iter().any(|name| name == target_name) {
                admitter.error(
                    "E-TRANS-005",
                    format!(
                        "`on {trigger}:` action cannot target algebraic unknown `{target_name}` \
                         (the Newton projection owns it)"
                    ),
                    action.source,
                );
                continue;
            }
            let declared = if is_state_path {
                admitter.states.get(target_name)
            } else {
                admitter
                    .inputs
                    .get(target_name)
                    .or_else(|| admitter.states.get(target_name))
            };
            if declared.is_none() {
                let display = if is_state_path {
                    format!("state.{target_name}")
                } else {
                    target_name.clone()
                };
                admitter.error(
                    "E-TRANS-002",
                    format!(
                        "`on {trigger}:` action target `{display}` is not a declared input or state slot"
                    ),
                    action.source,
                );
                continue;
            }
            // Values are numeric `.emath` expressions; SIR-lower and keep
            // the ExprId so the runner can execute the action.
            let Some((val_id, val_infer)) = admitter.lower_expr(value) else {
                continue;
            };
            match val_infer {
                Infer::F64 | Infer::Int | Infer::Nat => {}
                _ => {
                    admitter.error(
                        "E-TRANS-003",
                        format!(
                            "`on {trigger}:` action value must be numeric, inferred {val_infer}"
                        ),
                        value.source,
                    );
                    continue;
                }
            }
            let val_id = admitter.inline_defs(val_id);
            rule_actions.push(TransitionAction {
                target: target_name.clone(),
                is_state: is_state_path,
                expr: val_id,
            });
            admitter.record(
                "sema",
                format!("transition on `{trigger}` action `{target_name}=<expr>` SIR-lowered"),
                action.source,
            );
        }
        if !rule_actions.is_empty() {
            admitter.transitions.push(TransitionDecl {
                trigger: trigger.clone(),
                actions: rule_actions,
            });
        }
        admitter.params = saved_params;
    }
}
