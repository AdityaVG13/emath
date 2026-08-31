//! Declaration admission: the `admit_declaration` entry point and its
//! `AdmitResult` type, extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind, Section, StmtKind, UnaryOp,
};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::{
    BinaryOp, Declaration, EventAction, EventDecl, ExprId, ExprNode, Extent, KindSchema,
    LawMetadata, Literal, ModelResidual, Provenance, RepeatPolicy, TransitionAction,
    TransitionDecl, TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::{admit_equations, collect_node_names, residual_span};
use super::infer::{Infer, infer_conforms, infer_from_node};
use super::SiblingFunction;
use super::sections::{admit_compile_spec, admit_constructor, admit_named_field};
use super::sections_meta::{
    admit_about, admit_binding_provenance, admit_evidence, admit_host, admit_law_metadata,
};
use super::types::map_type;
use super::{
    Admitter, E_DUPLICATE_FIELD, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE, PHASE1_SECTIONS,
    TraceEntry,
};

/// Payload admission for hybrid events (r3-dynamical-03lh ch7,
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
fn admit_event_payloads(
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
                format!(
                    "event `{name}` condition must be Boolean, inferred {cond_infer}"
                ),
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
                format!(
                    "event `{name}` action must be an assignment (`target = expr`)"
                ),
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
            format!(
                "event `{name}` payload admitted (condition + action on `{target_name}`)"
            ),
            *source,
        );
    }
}

/// Structural admission + SIR lowering for the `transitions:` section
/// (r3-dynamical-03lh ch7, transitions slice — pass 3). A transition
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
/// admitter (the runner — pass 4 — owns execution). Mirrors
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
fn admit_transitions(
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
                StmtKind::FnDecl { head, name, params, .. } if head == "event" => {
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
            // the ExprId so the runner (pass 4) can execute the action.
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
                format!(
                    "transition on `{trigger}` action `{target_name}=<expr>` SIR-lowered"
                ),
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

fn numeric_literal(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(value) | ExprKind::Float(value) => value.replace('_', "").parse::<f64>().ok(),
        ExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => numeric_literal(value).map(|value| -value),
        _ => None,
    }
}

fn natural_literal(expr: &Expr) -> Option<usize> {
    let value = numeric_literal(expr)?;
    (value.is_finite() && value >= 0.0 && value.fract() == 0.0).then(|| value as usize)
}

fn path_is(expr: &Expr, expected: &str) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Path { segments, generics: None }
            if segments.len() == 1 && segments[0] == expected
    )
}

fn recurrence_offset(expr: &Expr, index: &str) -> Option<i64> {
    if path_is(expr, index) {
        return Some(0);
    }
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        return None;
    };
    if !path_is(left, index) {
        return None;
    }
    let amount = natural_literal(right)? as i64;
    match op {
        SynBinOp::Sub => Some(amount),
        SynBinOp::Add => Some(-amount),
        _ => None,
    }
}

fn recurrence_coefficients(
    expr: &Expr,
    sequence: &str,
    index: &str,
    scale: f64,
    out: &mut BTreeMap<usize, f64>,
) -> Result<(), &'static str> {
    match &expr.kind {
        ExprKind::Binary {
            op: SynBinOp::Add,
            left,
            right,
        } => {
            recurrence_coefficients(left, sequence, index, scale, out)?;
            recurrence_coefficients(right, sequence, index, scale, out)
        }
        ExprKind::Binary {
            op: SynBinOp::Sub,
            left,
            right,
        } => {
            recurrence_coefficients(left, sequence, index, scale, out)?;
            recurrence_coefficients(right, sequence, index, -scale, out)
        }
        ExprKind::Binary {
            op: SynBinOp::Mul,
            left,
            right,
        } => {
            if let Some(coefficient) = numeric_literal(left) {
                recurrence_coefficients(right, sequence, index, scale * coefficient, out)
            } else if let Some(coefficient) = numeric_literal(right) {
                recurrence_coefficients(left, sequence, index, scale * coefficient, out)
            } else {
                Err("E-SEQ-RECURRENCE")
            }
        }
        ExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => recurrence_coefficients(value, sequence, index, -scale, out),
        ExprKind::Index { value, indices } if path_is(value, sequence) && indices.len() == 1 => {
            let Some(offset) = recurrence_offset(&indices[0], index) else {
                return Err("E-SEQ-RECURRENCE");
            };
            if offset <= 0 {
                return Err("E-SEQ-TERMINATION");
            }
            *out.entry(offset as usize).or_default() += scale;
            Ok(())
        }
        _ => Err("E-SEQ-RECURRENCE"),
    }
}

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
pub(super) type AdmitResult = (
    Option<Declaration>,
    Vec<TestCase>,
    Vec<TypeNode>,
    Vec<(ExprNode, Span)>,
    Vec<TraceEntry>,
    Diagnostics,
    Vec<ModelResidual>,
    Vec<EventDecl>,
    Vec<TransitionDecl>,
    Option<LawMetadata>,
    BTreeMap<String, Provenance>,
);

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
pub(super) fn admit_declaration(
    decl: &emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
    capability_cells: &[(String, u32, Option<String>)],
    sibling_functions: &BTreeMap<String, SiblingFunction>,
) -> AdmitResult {
    let mut admitter = Admitter::new();
    admitter.host_types = host_types.clone();
    admitter.capability_cells = capability_cells.to_vec();
    admitter.sibling_functions = sibling_functions.clone();
    let kind_label = decl.as_kind.clone();
    let is_policy = kind_label == "policy";
    let is_model = kind_label == "model";
    let is_law = kind_label == "law";
    let schema = if is_policy {
        KindSchema::core_policy()
    } else if is_model {
        KindSchema::core_model()
    } else {
        KindSchema::core_function()
    };

    // Section collection with duplicate detection (E-SYN-103).
    let mut by_name: BTreeMap<&str, &Section> = BTreeMap::new();
    for section in decl.sections() {
        if let Some(previous) = by_name.get(section.name.as_str()) {
            admitter.error(
                "E-SYN-103",
                format!(
                    "duplicate section `{}` (first declared at bytes {}..{})",
                    section.name, previous.source.start, previous.source.end
                ),
                section.source,
            );
        } else {
            by_name.insert(&section.name, section);
        }
    }

    // Pass 5 (bead emath-l3-contracted-component-ceus7): L3 section rules.
    //
    // R5 (E-NAME-020): a name bound in BOTH `inputs:` and `outputs:` forks
    // the contract's identity for that slot. The generic duplicate-field
    // check already rejects it ("duplicate field ... declared in section
    // ..."), so no local rule is needed here; the LOCAL rule below covers
    // the case nothing else catches: a `definitions:` name shadowing an
    // `inputs:` name.
    // R6 (E-SEC-130): contract mode with `outputs:`/`goals:` but NO `inputs:`
    // section leaves the I/O surface unnamed — refuse.
    // R4 (E-SEC-133): contract mode without `goals:` is legal (every
    // definition defaults to evaluate) but the default is made visible.
    // Evidence (E-EV-140): only ASSERTION verbs (`prove`) claim truth
    // without computing it; Phase 1 goal verbs are operational and never
    // demand `evidence:`. The rule keys on the CLAIM_VERBS list below.
    if by_name.contains_key("inputs")
        || by_name.contains_key("outputs")
        || by_name.contains_key("definitions")
        || by_name.contains_key("goals")
        || by_name.contains_key("evidence")
    {
        let input_names: BTreeSet<String> = by_name
            .get("inputs")
            .map(|section| {
                section
                    .suite
                    .statements
                    .iter()
                    .filter_map(|stmt| match &stmt.kind {
                        StmtKind::FieldDecl { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // R5 (continued): a `definitions:` name that shadows an `inputs:`
        // name silently overwrites the declared input inside the component
        // body — same identity fork as inputs/outputs, same refusal.
        if let Some(definitions) = by_name.get("definitions") {
            for stmt in &definitions.suite.statements {
                if let StmtKind::Assign { target, .. } = &stmt.kind
                    && target.segments.len() == 1
                    && input_names.contains(&target.segments[0])
                {
                    let name = &target.segments[0];
                    admitter.error(
                        "E-NAME-020",
                        format!(
                            "definition `{name}` shadows the `inputs:` name `{name}` — \
                             a definition cannot overwrite a declared input"
                        ),
                        stmt.source,
                    );
                }
            }
        }
        let has_inputs = by_name.contains_key("inputs");
        // A declared hole (`name = ?`) IS the named unknown — the contract
        // may leave the I/O surface implicit when the hole names it.
        let declares_hole = decl.body.iter().chain(
            decl.sections().flat_map(|section| section.suite.statements.iter()),
        ).any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::Assign { value, .. }
                    if matches!(
                        &value.kind,
                        ExprKind::Path { segments, .. }
                            if segments.len() == 1 && segments[0] == "Hole"
                    )
            )
        });
        let has_outputs_or_goals =
            by_name.contains_key("outputs") || by_name.contains_key("goals");
        if has_outputs_or_goals && !has_inputs && !declares_hole {
            admitter.error(
                "E-SEC-130",
                "contract-mode declaration has `outputs:`/`goals:` but no `inputs:` \
                 section — add `inputs:` to name the I/O surface",
                decl.head_source,
            );
        }
        let goals_nonempty = by_name
            .get("goals")
            .is_some_and(|section| !section.suite.statements.is_empty());
        if !by_name.contains_key("goals") || !goals_nonempty {
            admitter.warning(
                "E-SEC-133",
                "no `goals:` section — every definition defaults to `evaluate`; \
                 declare `goals:` to pin intent",
                decl.head_source,
            );
        }
        // Evidence (E-EV-140): an ASSERTION verb states truth without
        // computing it; Phase 1 goal verbs (evaluate, differentiate,
        // benchmark, fit, simplify) are operational — they compute, they
        // do not claim, so they never demand `evidence:` (demanding it
        // broke the fit goals). `prove` is the first claim verb; when the
        // goals grammar accepts it, listing it in CLAIM_VERBS activates
        // the rule.
        const CLAIM_VERBS: &[&str] = &[];
        if let Some(goals) = by_name.get("goals") {
            let claim_bearing = goals
                .suite
                .statements
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    StmtKind::Section(nested) => Some(nested.name.as_str()),
                    _ => None,
                })
                .any(|verb| CLAIM_VERBS.contains(&verb));
            let evidence_present = by_name
                .get("evidence")
                .is_some_and(|section| !section.suite.statements.is_empty());
            if claim_bearing && !evidence_present {
                admitter.error(
                    "E-EV-140",
                    "claim-bearing goal verb requires an `evidence:` section with \
                     at least one row (a claim without evidence is a silent assertion)",
                    goals.head_source,
                );
            }
        }
    }
    // Kind schema is the required/optional source of truth (`E-KIND-011`).
    for (name, section_schema) in schema.sections() {
        if section_schema.repeat == RepeatPolicy::ExactlyOne && !by_name.contains_key(name) {
            admitter.error(
                "E-KIND-011",
                format!("kind `{}` requires section `{name}`", schema.name()),
                decl.head_source,
            );
        }
    }

    // Phase 1 whitelist: a section outside the subset is a typed refusal,
    // never a silent drop (AGENTS.md rule 6). `request:` / `requests:`
    // are the pre-`goals:` spellings; refuse with a migration hint.
    for section in decl.sections() {
        if matches!(section.name.as_str(), "request" | "requests") {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}:` was renamed to `goals:`; use `goals:`",
                    section.name
                ),
                section.head_source,
            );
            continue;
        }
        if matches!(
            section.name.as_str(),
            "assumptions" | "domain" | "citations"
        ) && !is_law
        {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}` is admitted only on `emath law` declarations",
                    section.name
                ),
                section.head_source,
            );
            continue;
        }
        if !PHASE1_SECTIONS.contains(&section.name.as_str()) {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}` is outside the Phase 1 subset (known: {})",
                    section.name,
                    PHASE1_SECTIONS.join(", ")
                ),
                section.head_source,
            );
        }
    }

    // Fields: inputs, outputs, state. Head-args lower into the same Field
    // IR as an `inputs:` section. `-> T` declares a single output named
    // after the declaration (the example `square = x * x` binds the
    // declaration name). Mixing the head spelling with the equivalent
    // section forks identity and is refused.
    let mut fields_infer: BTreeMap<String, Infer> = BTreeMap::new();
    let mut fields_by_section: BTreeMap<&str, Vec<Field>> = BTreeMap::new();
    let mut outputs_from_head = false;
    if let Some(signature) = &decl.signature {
        let stateful = by_name.contains_key("state") || by_name.contains_key("constructors");
        let refuse_head = !matches!(kind_label.as_str(), "function" | "law") || stateful;
        if refuse_head {
            admitter.error(
                "E-SYN-123",
                "declaration head arguments are only admitted on stateless `emath function` or `emath law` declarations (no `state:` or `constructors:`)",
                decl.head_source,
            );
        }
        if by_name.contains_key("inputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head arguments cannot be mixed with an `inputs:` section; use one spelling",
                decl.head_source,
            );
        }
        if signature.ret.is_some() && by_name.contains_key("outputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head `->` return type cannot be mixed with an `outputs:` section; use one spelling",
                decl.head_source,
            );
        }
        let mix_inputs = by_name.contains_key("inputs");
        let mix_outputs = signature.ret.is_some() && by_name.contains_key("outputs");
        if !refuse_head && !mix_inputs {
            for param in &signature.params {
                if param.by_ref {
                    admitter.error(
                        "E-SYN-101",
                        "by-ref declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                if param.default.is_some() {
                    admitter.error(
                        "E-SYN-101",
                        "default values on declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "inputs",
                    &param.name,
                    &param.ty,
                    param.source,
                    true,
                );
            }
        }
        if !refuse_head && !mix_outputs {
            if let Some(ret) = &signature.ret {
                outputs_from_head = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "outputs",
                    &decl.name,
                    ret,
                    ret.source,
                    false,
                );
            }
        }
    }

    for section_name in ["inputs", "outputs", "state"] {
        if let Some(section) = by_name.get(section_name) {
            for stmt in &section.suite.statements {
                let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                    admitter.error(
                        "E-SYN-101",
                        format!("only `name: Type` declarations are allowed in `{section_name}`"),
                        stmt.source,
                    );
                    continue;
                };
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    section_name,
                    name,
                    ty,
                    stmt.source,
                    section_name == "inputs",
                );
            }
        }
    }

    let inputs = fields_by_section.get("inputs").cloned().unwrap_or_default();
    let outputs_omitted = !by_name.contains_key("outputs") && !outputs_from_head;
    let mut outputs_raw = fields_by_section
        .get("outputs")
        .cloned()
        .unwrap_or_default();
    let state = fields_by_section.get("state").cloned().unwrap_or_default();
    // `algebraic:` variables are the unknowns of the implicit residual
    // system (causalized DAEs); initial guesses are supplied at simulate
    // time in the same map as `inputs:` values.
    if let Some(section) = by_name.get("algebraic") {
        if !is_model {
            admitter.error(
                "E-KIND-010",
                format!(
                    "`algebraic:` (implicit unknowns solved alongside the ODE states) is only admitted on `emath model` declarations; you used `emath {kind_label}` — did you mean `emath model`?"
                ),
                section.source,
            );
        } else {
            for stmt in &section.suite.statements {
                let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                    admitter.error(
                        "E-SYN-101",
                        "only `name: Type` declarations are allowed in `algebraic:`",
                        stmt.source,
                    );
                    continue;
                };
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "algebraic",
                    name,
                    ty,
                    stmt.source,
                    false,
                );
            }
        }
    }
    let algebraic_fields = fields_by_section
        .get("algebraic")
        .cloned()
        .unwrap_or_default();
    admitter.inputs = inputs
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();
    for field in &algebraic_fields {
        // Algebraic variables resolve like inputs inside definitions and
        // residuals; the runner binds their guesses from the same value
        // map. They stay out of `Declaration.inputs` (I/O contract).
        admitter
            .inputs
            .entry(field.name.clone())
            .or_insert_with(|| fields_infer.get(&field.name).cloned().unwrap_or(Infer::F64));
    }
    admitter.states = state
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();

    // A law may pair prose assumptions with machine-checkable `require`
    // expressions over its inputs. They reuse invariant IR so execution can
    // refuse before evaluating a partial formula.
    if is_law && let Some(section) = by_name.get("assumptions") {
        for stmt in &section.suite.statements {
            if let StmtKind::Require(expr) = &stmt.kind
                && let Some(id) = admitter.lower_requirement(expr)
            {
                admitter.constraints.push(id);
            }
        }
    }

    // Hybrid events (r3-dynamical-03lh, ch7): `events:` declares the
    // discrete event surface — `event Name(field: Type)` declarations
    // (FnDecl head `event`) or no-arg `event Name` commands. Events are
    // named surface: the same event name twice refuses through the
    // duplicate lane (E-NAME-022), and anything that is not an event
    // declaration refuses typed. Payload suites (`if <condition>:`
    // actions) are admitted later by `admit_event_payloads`, after
    // definitions and equations lower, so their expressions may name
    // declared definitions.
    if let Some(section) = by_name.get("events") {
        let mut seen_events: BTreeSet<String> = BTreeSet::new();
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::FnDecl { head, name, .. } if head == "event" => {
                    if !seen_events.insert(name.clone()) {
                        admitter.error(
                            "E-NAME-022",
                            format!("duplicate event name `{name}`"),
                            stmt.source,
                        );
                    }
                }
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("event") && head.len() == 2 =>
                {
                    let name = head[1].clone();
                    if !seen_events.insert(name.clone()) {
                        admitter.error(
                            "E-NAME-022",
                            format!("duplicate event name `{name}`"),
                            stmt.source,
                        );
                    }
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "only `event <Name>(field: Type)` declarations are allowed in `events:`",
                        stmt.source,
                    );
                }
            }
        }
    }

    // Measured evidence (04 §5.2, emath-r3-observations-9ffu): each row
    // is `obs <name>[: type] = <data>` (the parser stores it losslessly
    // as a FieldDecl with the value as its default). An observation is a
    // datum an instrument reported, not a model binding: it is
    // type-checked once here and named for provenance, but NEVER entered
    // into the definition environment — the E-OBS-WRITE refusal below
    // keeps model output from silently overwriting data. Reading
    // observations in comparisons (§5.3) is the named next slice.
    let mut observation_names: BTreeSet<String> = BTreeSet::new();
    if let Some(section) = by_name.get("observations") {
        for stmt in &section.suite.statements {
            let StmtKind::FieldDecl {
                name, ty, default, ..
            } = &stmt.kind
            else {
                admitter.error(
                    "E-SYN-101",
                    "observations rows are `obs <name>[: type] = <data>`",
                    stmt.source,
                );
                continue;
            };
            if !observation_names.insert(name.clone()) {
                admitter.error(
                    "E-NAME-022",
                    format!("duplicate observation name `{name}`"),
                    stmt.source,
                );
                continue;
            }
            let Some(value) = default else {
                admitter.error(
                    "E-SYN-101",
                    format!("observation `{name}` needs a value (`obs {name}[: type] = data`)"),
                    stmt.source,
                );
                continue;
            };
            let value_infer = admitter.lower_expr(value).map(|(_, infer)| infer);
            let declared = if let emath_core::tree::TypeKind::Path {
                segments,
                generic_args,
            } = &ty.kind
                && generic_args.is_empty()
                && segments.last().map(String::as_str) == Some("Infer")
            {
                None
            } else {
                map_type(ty, &mut admitter.diagnostics, &admitter.host_types)
                    .map(|node| infer_from_node(&node))
            };
            if let (Some(value_infer), Some(declared)) = (&value_infer, &declared)
                && !infer_conforms(value_infer, declared)
            {
                admitter.error(
                    "E-TYPE-012",
                    format!("observation `{name}` has type {value_infer}, expected {declared}"),
                    value.source,
                );
            }
        }
    }

    // Constraints section: process before definitions so the optimizer
    // can access them during definition lowering.  Each statement is an
    // expression that must infer as Bool.
    if let Some(section) = by_name.get("constraints") {
        for stmt in &section.suite.statements {
            let StmtKind::Expr(expr) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only expressions are allowed in `constraints:`",
                    stmt.source,
                );
                continue;
            };
            match admitter.lower_expr(expr) {
                Some((id, Infer::Bool)) => admitter.constraints.push(id),
                Some((_, infer)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("constraint must be Bool, got {infer}"),
                        expr.source,
                    );
                }
                None => {}
            }
        }
    }

    // Invariant section: each statement is a claim (Bool) that must hold.
    // Uses lower_requirement so claim expressions (limit, series, asymp)
    // are admitted as Bool(true) rather than erroring.
    if let Some(section) = by_name.get("invariant") {
        for stmt in &section.suite.statements {
            let expr = match &stmt.kind {
                StmtKind::Expr(e) => e,
                StmtKind::Require(e) | StmtKind::Ensure(e) | StmtKind::Invariant(e) => e,
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "only expressions are allowed in `invariant:`",
                        stmt.source,
                    );
                    continue;
                }
            };
            if let Some(id) = admitter.lower_requirement(expr) {
                admitter.constraints.push(id);
            }
        }
    }

    // Proof outlines (B13 + 05 §7.2, emath-r3-proofs-0qua): obligation
    // kinds as DATA. Proofs are additive authority, never admission
    // tickets — nothing here gates artifact production, and outline
    // claims are never lowered as definitions or constraints
    // (justification stays structurally separate from meaning).
    // Completeness is checked (an outline ends with its qed); NO
    // ProofChecker runs in this slice — `check` steps are data
    // obligations, and machine-record lowering is the named follow-up.
    if let Some(section) = by_name.get("proofs") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(outline) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only `outline <Name>:` proof outlines are allowed in `proofs:`",
                    stmt.source,
                );
                continue;
            };
            let outline_name = outline.generic.as_deref().unwrap_or("_");
            let steps = &outline.suite.statements;
            if steps.is_empty() {
                admitter.error(
                    "E-SYN-101",
                    format!("proof outline `{outline_name}` is empty"),
                    stmt.source,
                );
                continue;
            }
            let mut declared: Vec<String> = Vec::new();
            let mut ends_with_qed = false;
            for (i, step) in steps.iter().enumerate() {
                ends_with_qed = false;
                match &step.kind {
                    StmtKind::Section(s)
                        if matches!(s.name.as_str(), "assumption" | "lemma") =>
                    {
                        let target = s.generic.clone().unwrap_or_else(|| "_".into());
                        if s.name == "lemma" && s.suite.statements.is_empty() {
                            admitter.error(
                                "E-SYN-101",
                                format!(
                                    "lemma `{target}` in outline `{outline_name}` needs a claim after `:`"
                                ),
                                step.source,
                            );
                        }
                        declared.push(target);
                    }
                    StmtKind::Command { head, .. }
                        if head.first().map(String::as_str) == Some("check") =>
                    {
                        match head.get(1) {
                            Some(target) if declared.iter().any(|d| d == target) => {}
                            Some(target) => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`check {target}` in outline `{outline_name}` names an obligation not declared earlier in the outline"
                                ),
                                step.source,
                            ),
                            None => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`check` in outline `{outline_name}` must name an obligation: `check <name>`"
                                ),
                                step.source,
                            ),
                        }
                    }
                    StmtKind::Command { head, .. }
                        if head.first().map(String::as_str) == Some("qed") =>
                    {
                        ends_with_qed = i + 1 == steps.len();
                        match head.get(1) {
                            Some(target) if declared.iter().any(|d| d == target) => {}
                            Some(target) => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed {target}` in outline `{outline_name}` names an obligation not declared earlier in the outline"
                                ),
                                step.source,
                            ),
                            None => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed` in outline `{outline_name}` must name the concluding obligation: `qed <name>`"
                                ),
                                step.source,
                            ),
                        }
                        if i + 1 != steps.len() {
                            admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed` must be the final step of outline `{outline_name}`"
                                ),
                                step.source,
                            );
                        }
                    }
                    _ => {
                        admitter.error(
                            "E-SYN-101",
                            format!(
                                "unknown proof step in outline `{outline_name}`; obligation kinds are data: `assumption <name>: <claim>`, `lemma <name>: <claim>`, `check <name>`, `qed <name>`"
                            ),
                            step.source,
                        );
                    }
                }
            }
            if !ends_with_qed {
                admitter.error(
                    "E-SYN-101",
                    format!(
                        "proof outline `{outline_name}` is incomplete: it must end with `qed <obligation>` (an outline without its concluding qed is not an obligation record); ProofChecker integration is the named follow-up"
                    ),
                    stmt.source,
                );
            }
            // Obligation data lands in the semantic trace; the
            // `emath.proof-obligation v1` machine-record lowering and the
            // ProofChecker contract are the named follow-ups.
            admitter.record(
                "proofs",
                format!(
                    "outline `{outline_name}`: {} obligation step(s) recorded as data (assumption/lemma/check/qed); no ProofChecker runs in this slice",
                    steps.len()
                ),
                stmt.source,
            );
        }
    }

    // Declarative figures (05 §7.4, emath-r3-figures-b1xn): the
    // section NAME + payload grammar slot is reserved — `figures:` is
    // out of the generic E-SEC-101 roster error and every payload row
    // refuses naming the design forks. The payload grammar is the
    // named follow-up; nothing draws in this seed.
    if let Some(section) = by_name.get("figures") {
        for stmt in &section.suite.statements {
            admitter.error(
                "E-SYN-101",
                "`figures:` payload rows are outside the Phase 1 subset — the declarative-figures design follow-up (emath-r3-figures-b1xn) must first settle: sampling is tied to the budgets/continuation machinery from day one (unbounded adaptive sampling would be the first nondeterminism smuggled in through the front door), every figure artifact carries its sampling receipt (visual continuity is labeled OBSERVATIONAL, never proved smoothness), and the renderer is a provider contract (Renderer) — upstream never defines render semantics; the same spec must render in WASM, PNG, or paper",
                stmt.source,
            );
        }
    }

    // Definitions.
    let mut definitions: BTreeMap<String, ExprId> = BTreeMap::new();
    let mut sequence_bases: BTreeMap<String, BTreeMap<usize, f64>> = BTreeMap::new();
    if let Some(section) = by_name.get("definitions") {
        for stmt in &section.suite.statements {
            let StmtKind::Assign { target, value } = &stmt.kind else {
                // Bio dynamics (04 §4.3–4.5, emath-r3-bio-dynamics-ephb):
                // the propensity/schedule/seed vocabulary refuses with its
                // named field-pack follow-up instead of the generic
                // row-shape error, so the diagnostic tells the modeler
                // where the capability actually lands.
                let bio_fence = match &stmt.kind {
                    StmtKind::Section(section)
                        if matches!(
                            section.name.as_str(),
                            "propensity" | "dose" | "sample"
                        ) =>
                    {
                        Some(match section.name.as_str() {
                            "propensity" => format!(
                                "`propensity <Name>:` transition declarations are outside the Phase 1 subset — the propensity/transition rule suite ships with the bio dynamics field-pack follow-up (emath-r3-bio-dynamics-ephb); `events:` sections admit today and `on <trigger>:` rules are that lane's named next slice"
                            ),
                            _ => format!(
                                "`{}` schedule rows lower to ch7 clocks and ship with the bio dynamics field-pack follow-up (emath-r3-bio-dynamics-ephb); measured `obs` rows are the admitted evidence surface today",
                                section.name
                            ),
                        })
                    }
                    StmtKind::Command { head, .. }
                        if head.first().map(String::as_str) == Some("seed") =>
                    {
                        Some(
                            "`seed <hex>` seeds the stochastic goal's RNG stream (E-SIM-SEED: an unseeded stochastic goal refuses) — the seeded-randomness world ships with the bio dynamics field-pack follow-up (emath-r3-bio-dynamics-ephb)"
                                .to_string(),
                        )
                    }
                    _ => None,
                };
                if let Some(message) = bio_fence {
                    admitter.error("E-SYN-101", message, stmt.source);
                    continue;
                }
                // F6 (emath-r3-docs-fixes-3wtm): `=` vs `==` causalization.
                // A `==` statement here is a comparison, not a definition:
                // name both readings instead of a generic shape error.
                let is_eqeq = match &stmt.kind {
                    StmtKind::Expr(expr) => matches!(
                        &expr.kind,
                        ExprKind::Binary {
                            op: SynBinOp::Eq,
                            ..
                        }
                    ),
                    _ => false,
                };
                admitter.error(
                    "E-SYN-101",
                    if is_eqeq {
                        "`definitions:` binds with `=` (the left name takes the value); `==` is a comparison/equation and does not define a name — write `name = lhs == rhs` as a definition, or move the `==` row to `equations:`/`invariant:` where it constrains"
                    } else {
                        "only `name = expression` definitions are allowed in Phase 1"
                    },
                    stmt.source,
                );
                continue;
            };
            if target.segments.len() != 1 {
                admitter.error(
                    E_UNSUPPORTED_TYPE,
                    "nested definition targets are not supported",
                    target.source,
                );
                continue;
            }
            let name = &target.segments[0];
            if !target.indices.is_empty() {
                if target.indices.len() != 1 {
                    admitter.error(
                        "E-SEQ-RECURRENCE",
                        "sequence definition rows require exactly one index",
                        target.source,
                    );
                    continue;
                }
                if let Some(index) = natural_literal(&target.indices[0]) {
                    let Some(base) = numeric_literal(value).filter(|value| value.is_finite())
                    else {
                        admitter.error(
                            "E-SEQ-RECURRENCE",
                            "sequence base cases must be finite numeric literals",
                            value.source,
                        );
                        continue;
                    };
                    if sequence_bases
                        .entry(name.clone())
                        .or_default()
                        .insert(index, base)
                        .is_some()
                    {
                        admitter.error(
                            E_DUPLICATE_FIELD,
                            format!("duplicate sequence base case `{name}[{index}]`"),
                            target.source,
                        );
                    }
                    continue;
                }
                let ExprKind::Path {
                    segments,
                    generics: None,
                } = &target.indices[0].kind
                else {
                    admitter.error(
                        "E-SEQ-RECURRENCE",
                        "recurrence index must be a single variable such as `n`",
                        target.indices[0].source,
                    );
                    continue;
                };
                if segments.len() != 1 {
                    admitter.error(
                        "E-SEQ-RECURRENCE",
                        "recurrence index must be a single variable such as `n`",
                        target.indices[0].source,
                    );
                    continue;
                }
                let mut coefficients = BTreeMap::new();
                if let Err(code) =
                    recurrence_coefficients(value, name, &segments[0], 1.0, &mut coefficients)
                {
                    admitter.error(
                        code,
                        if code == "E-SEQ-TERMINATION" {
                            "recurrence self-references must structurally decrease the index"
                        } else {
                            "recurrence must be a finite linear combination of earlier terms"
                        },
                        value.source,
                    );
                    continue;
                }
                let order = coefficients.keys().next_back().copied().unwrap_or(0);
                let Some(bases) = sequence_bases.get(name) else {
                    admitter.error(
                        "E-SEQ-RECURRENCE",
                        "recurrence requires base cases starting at index 0",
                        target.source,
                    );
                    continue;
                };
                if order == 0 || (0..order).any(|index| !bases.contains_key(&index)) {
                    admitter.error(
                        "E-SEQ-RECURRENCE",
                        format!(
                            "order-{order} recurrence requires contiguous base cases `{name}[0]` through `{name}[{}]`",
                            order.saturating_sub(1)
                        ),
                        target.source,
                    );
                    continue;
                }
                if definitions.contains_key(name) {
                    admitter.error(
                        E_DUPLICATE_FIELD,
                        format!("duplicate recurrence definition `{name}`"),
                        target.source,
                    );
                    continue;
                }
                let initial = (0..order)
                    .map(|index| {
                        admitter.push_expr(
                            ExprNode::Literal(Literal::FloatBits(bases[&index].to_bits())),
                            target.source,
                        )
                    })
                    .collect();
                let recurrence = (1..=order)
                    .map(|offset| {
                        let value = coefficients.get(&offset).copied().unwrap_or(0.0);
                        admitter.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            target.source,
                        )
                    })
                    .collect();
                let initial = admitter.push_expr(ExprNode::Vector(initial), target.source);
                let recurrence = admitter.push_expr(ExprNode::Vector(recurrence), target.source);
                let budget = admitter.push_expr(
                    ExprNode::Literal(Literal::FloatBits(1024.0_f64.to_bits())),
                    target.source,
                );
                let id = admitter.push_expr(
                    ExprNode::Call {
                        function: QualifiedName("generating_function".into()),
                        arguments: vec![initial, recurrence, budget],
                    },
                    target.source,
                );
                definitions.insert(name.clone(), id);
                admitter
                    .definitions
                    .insert(name.clone(), (id, Infer::Sequence));
                admitter.record(
                    "sema",
                    format!("structurally decreasing recurrence `{name}` typed"),
                    target.source,
                );
                continue;
            }
            // 04 §5.2 (emath-r3-observations-9ffu): the model/observation
            // line — a definition binding an observation name would let
            // model output silently overwrite measured data.
            if observation_names.contains(name.as_str()) {
                admitter.error(
                    "E-OBS-WRITE",
                    format!(
                        "`{name}` is an observation: observations are read-only measured evidence and are never written by the model — bind a different name for the model quantity"
                    ),
                    target.source,
                );
                continue;
            }
            if definitions.contains_key(name) {
                admitter.error(
                    E_DUPLICATE_FIELD,
                    format!("duplicate definition `{name}`"),
                    target.source,
                );
                continue;
            }
            match admitter.lower_expr(value) {
                Some((
                    id,
                    infer @ (Infer::F64
                    | Infer::Nat
                    | Infer::Int
                    | Infer::Rat
                    | Infer::Complex
                    | Infer::Bool
                    | Infer::Text
                    | Infer::Set(_)
                    | Infer::Record(_)
                    | Infer::Unit { .. }
                    | Infer::HostDeferred
                    | Infer::Series
                    | Infer::Sequence
                    | Infer::Vector { .. }
                    | Infer::Matrix { .. }
                    | Infer::Tensor { .. }
                    | Infer::OptionCarrier
                    | Infer::ResultCarrier),
                )) => {
                    if let Some(output) = outputs_raw
                        .iter()
                        .find(|output| output.name == *name)
                    {
                        let declared = admitter.type_of(output.ty);
                        if !infer_conforms(&infer, &declared) {
                            admitter.error(
                                "E-TYPE-012",
                                format!(
                                    "definition `{name}` has type {infer}, expected {declared}"
                                ),
                                value.source,
                            );
                        }
                        // FieldPrime exactness: a prime-field output is an
                        // exact integer type; a float definition must not
                        // numerically widen into it. Plain Int keeps the
                        // legacy F64→Int widening (untouched).
                        if matches!(
                            admitter.node_of(output.ty),
                            Some(emath_ir::TypeNode::FieldPrime { .. })
                        ) && matches!(infer, Infer::F64)
                        {
                            admitter.error(
                                "E-TYPE-012",
                                format!(
                                    "definition `{name}` has type {infer}, expected an exact integer field element ({declared}); a float does not conform to a `Field` type"
                                ),
                                value.source,
                            );
                        }
                    }
                    admitter.record("sema", format!("definition `{name}` typed"), value.source);
                    definitions.insert(name.clone(), id);
                    // Later definitions may name earlier ones (`b = a * a`).
                    admitter.definitions.insert(name.clone(), (id, infer));
                }
                Some((_, Infer::Opaque)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("definition `{name}` must be numeric; opaque host values are not scalars"),
                        value.source,
                    );
                }
                None => {}
            }
        }
    }
    admit_equations(&mut admitter, &by_name, &mut definitions, is_model);
    admit_event_payloads(&mut admitter, &by_name, &algebraic_fields);
    admit_transitions(&mut admitter, &by_name, &algebraic_fields);
    if is_model
        && definitions.is_empty()
        && !by_name.contains_key("definitions")
        && !by_name.contains_key("equations")
        && !by_name.contains_key("equation")
    {
        admitter.error(
            "E-KIND-011",
            "kind `model` requires section `definitions` or `equations`",
            decl.head_source,
        );
    }
    // Kind coaching (F4): a `emath model` with no dynamics payload is
    // almost certainly a stateless formula; suggest the right kind as a
    // note (never a refusal — the declaration still admits).
    if is_model
        && !by_name.contains_key("state")
        && !by_name.contains_key("equations")
        && !by_name.contains_key("equation")
        && !by_name.contains_key("algebraic")
        && !by_name.contains_key("constructors")
        && by_name.contains_key("definitions")
    {
        admitter.note(
            "N-KIND-001",
            "this `emath model` has only `definitions:` and no `state:`, `equations:`, or `algebraic:` — a stateless formula should be `emath function`",
            decl.head_source,
        );
    }
    if is_model && (by_name.contains_key("equations") || by_name.contains_key("equation")) {
        let residual_rates: BTreeSet<String> = admitter
            .residuals
            .iter()
            .flat_map(|residual| residual.rates.iter().cloned())
            .collect();
        for field in &state {
            let rate_name = format!("der_{}", field.name);
            if !definitions.contains_key(&rate_name)
                && !residual_rates.contains(field.name.as_str())
            {
                admitter.error(
                    "E-NAME-025",
                    format!(
                        "state `{}` has no `derivative({})` equation",
                        field.name, field.name
                    ),
                    field.source,
                );
            }
        }
    }
    // Causalization validation: the implicit residual system must be
    // square (unknown components == residual components) and every
    // declared `algebraic:` variable must be referenced by a residual.
    if is_model && !admitter.residuals.is_empty() {
        let mut unknown_dims: Vec<(String, usize)> = Vec::new();
        for field in &algebraic_fields {
            match fields_infer.get(&field.name) {
                Some(Infer::F64) => unknown_dims.push((field.name.clone(), 1)),
                Some(Infer::Vector {
                    extent: Some(Extent::Fixed(n)),
                }) => unknown_dims.push((field.name.clone(), *n)),
                _ => {
                    admitter.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "algebraic variable `{}` must be a Float64 scalar or a fixed-length vector of Float64",
                            field.name
                        ),
                        field.source,
                    );
                    unknown_dims.push((field.name.clone(), 0));
                }
            }
        }
        let rate_unknowns: Vec<(String, ExprId)> = admitter
            .residuals
            .iter()
            .flat_map(|residual| {
                residual
                    .rates
                    .iter()
                    .map(|rate| (rate.clone(), residual.expr))
            })
            .collect();
        for (rate, residual_expr) in &rate_unknowns {
            match admitter.states.get(rate) {
                Some(Infer::F64) => unknown_dims.push((format!("der({rate})"), 1)),
                Some(Infer::Vector {
                    extent: Some(Extent::Fixed(n)),
                }) => unknown_dims.push((format!("der({rate})"), *n)),
                _ => {
                    admitter.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "rate unknown `der({rate})` must derive a Float64 scalar or fixed-length vector state"
                        ),
                        residual_span(&admitter, *residual_expr),
                    );
                    unknown_dims.push((format!("der({rate})"), 0));
                }
            }
        }
        let unknown_total: usize = unknown_dims.iter().map(|(_, dims)| dims).sum();
        let residual_total: usize = admitter
            .residuals
            .iter()
            .map(|residual| residual.components as usize)
            .sum();
        if unknown_total == 0 {
            admitter.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "implicit residual has no unknown to solve for; declare `algebraic:` variables or write explicit `der(state) = rhs` rates"
                ),
                residual_span(&admitter, admitter.residuals[0].expr),
            );
        } else if unknown_total != residual_total {
            admitter.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "implicit residual system is not square: {} unknown component(s) ([{}]) vs {} residual component(s); every `algebraic:` variable must participate in the residual equations",
                    unknown_total,
                    unknown_dims
                        .iter()
                        .map(|(name, dims)| format!("{name}:{dims}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    residual_total
                ),
                residual_span(&admitter, admitter.residuals[0].expr),
            );
        }
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for residual in &admitter.residuals {
            if let Some((node, _)) = admitter.exprs.get(residual.expr.0 as usize) {
                collect_node_names(&admitter.exprs, node, &mut referenced);
            }
        }
        for field in &algebraic_fields {
            if !referenced.contains(&field.name) {
                admitter.error(
                    E_UNKNOWN_VARIABLE,
                    format!(
                        "algebraic variable `{}` is not referenced by any implicit residual equation",
                        field.name
                    ),
                    field.source,
                );
            }
        }
        for residual in &mut admitter.residuals {
            residual.algebraic = algebraic_fields
                .iter()
                .map(|field| field.name.clone())
                .collect();
        }
    } else if is_model && !algebraic_fields.is_empty() {
        admitter.error(
            E_UNSUPPORTED_TYPE,
            "`algebraic:` variables are only solved by at least one implicit residual equation in `equations:`",
            algebraic_fields[0].source,
        );
    }
    for output in &outputs_raw {
        if !definitions.contains_key(&output.name) {
            admitter.error(
                "E-NAME-023",
                format!("output `{}` has no definition", output.name),
                output.source,
            );
        }
    }
    if outputs_omitted && schema.default_for("outputs") == Some("definitions") {
        for name in definitions.keys() {
            if name.starts_with("der_") {
                continue;
            }
            let infer = admitter
                .definitions
                .get(name)
                .map(|(_, inf)| inf.clone())
                .unwrap_or(Infer::F64);
            let node = match infer {
                Infer::Bool => TypeNode::Bool,
                Infer::Complex => TypeNode::Complex(Box::new(TypeNode::Float64)),
                Infer::Set(element) => TypeNode::Set(Box::new(match *element {
                    Infer::Bool => TypeNode::Bool,
                    Infer::Nat => TypeNode::Nat,
                    Infer::Int => TypeNode::Int,
                    Infer::Text => TypeNode::Other(QualifiedName("Text".into())),
                    _ => TypeNode::Float64,
                })),
                Infer::Record(name) => TypeNode::Record(QualifiedName(name)),
                Infer::Text => TypeNode::Other(QualifiedName("Text".into())),
                Infer::Vector { extent } => TypeNode::Vector {
                    element: Box::new(TypeNode::Float64),
                    extent,
                },
                Infer::Matrix { rows, cols } => TypeNode::Matrix {
                    element: Box::new(TypeNode::Float64),
                    rows,
                    cols,
                },
                Infer::Tensor { shape } => TypeNode::Tensor {
                    element: Box::new(TypeNode::Float64),
                    shape,
                },
                Infer::Nat => TypeNode::Nat,
                Infer::Int => TypeNode::Int,
                _ => TypeNode::Float64,
            };
            let ty = admitter.type_id(node);
            outputs_raw.push(Field {
                name: name.clone(),
                ty,
                visibility: Visibility::Public,
                source: decl.source,
            });
        }
    }

    // Constructors.
    let mut constructors: Vec<Constructor> = Vec::new();
    if is_policy || is_model {
        if let Some(section) = by_name.get("constructors") {
            for stmt in &section.suite.statements {
                if let StmtKind::FnDecl {
                    visibility,
                    name,
                    params,
                    ret,
                    suite,
                    ..
                } = &stmt.kind
                {
                    if name != "new"
                        || !matches!(visibility, Some(emath_core::tree::Visibility::Public))
                    {
                        admitter.error(
                            "E-CTOR-036",
                            format!(
                                "Phase 1 admits exactly one public `new` constructor, found `{name}`"
                            ),
                            stmt.source,
                        );
                        continue;
                    }
                    if !constructors.is_empty() {
                        admitter.error(
                            "E-CTOR-036",
                            "multiple public `new` constructors are outside the Phase 1 subset",
                            stmt.source,
                        );
                        continue;
                    }
                    let mut constructor = admit_constructor(
                        &mut admitter,
                        params,
                        ret.as_ref(),
                        suite.as_ref(),
                        stmt.source,
                    );
                    constructor.name.clone_from(name);
                    constructor.is_public = true;
                    constructors.push(constructor);
                } else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `public fn new(...)` inside `constructors:`",
                        stmt.source,
                    );
                }
            }
        } else if is_policy {
            admitter.error(
                "E-CTOR-031",
                "policy declarations require a `constructors:` section with a public `new`",
                decl.head_source,
            );
        }
        // Constructor assignments must cover all state fields.
        if let Some(constructor) = constructors.first() {
            for field in &state {
                if !constructor.assignments.contains_key(&field.name) {
                    admitter.error(
                        "E-CTOR-030",
                        format!("missing state assignment for `{}`", field.name),
                        decl.head_source,
                    );
                }
            }
        }
    } else if let Some(section) = by_name.get("constructors") {
        admitter.error(
            "E-KIND-010",
            format!(
                "`constructors:` (stateful objects built by `public fn new`) are not admitted on `emath {kind_label}` — did you mean `emath policy`?"
            ),
            section.source,
        );
    }
    if !is_policy && !is_model && !state.is_empty() {
        admitter.error(
            "E-KIND-010",
            format!(
                "`emath {kind_label}` cannot carry `state:` — state belongs on `emath model` (continuous ODEs simulated over time) or `emath policy` (stateful object with constructors); did you mean one of those?"
            ),
            decl.head_source,
        );
    }

    // Compile spec.
    let compile_spec = admit_compile_spec(&mut admitter, by_name.get("compile").copied());

    // Exports.
    let mut exports = Vec::new();
    if let Some(section) = by_name.get("exports") {
        for stmt in &section.suite.statements {
            let StmtKind::Command { head, .. } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "exports must be `public <kind> <name>` commands",
                    stmt.source,
                );
                continue;
            };
            let mut words = head.iter().map(String::as_str);
            let visibility_word = words.next().unwrap_or("");
            let kind = words.next().unwrap_or("");
            let name = words.next().unwrap_or("");
            let public = visibility_word == "public";
            if !public {
                admitter.error(
                    "E-NAME-021",
                    "Phase 1 exports must be `public`",
                    stmt.source,
                );
                continue;
            }
            match kind {
                "constructor" => {
                    if name != "new" || constructors.is_empty() {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported constructor `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "constructor".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "function" => {
                    let from_diff = name.strip_prefix("gradient_").is_some_and(|target| {
                        by_name.get("goals").is_some_and(|section| {
                            section.suite.statements.iter().any(|stmt| {
                                matches!(
                                    &stmt.kind,
                                    StmtKind::Section(goal)
                                        if goal.name == "differentiate"
                                            && goal.generic.as_deref() == Some(target)
                                )
                            })
                        })
                    });
                    if !definitions.contains_key(name)
                        && !outputs_raw.iter().any(|o| o.name == *name)
                        && !from_diff
                    {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported function `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "function".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "type" => {
                    if name != decl.name {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported type `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "type".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                other => {
                    admitter.error(
                        "E-NAME-021",
                        format!("unsupported export kind `{other}`"),
                        stmt.source,
                    );
                }
            }
        }
    }

    // Tests.
    let mut tests: Vec<TestCase> = Vec::new();
    if let Some(section) = by_name.get("tests") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `example <name>:` blocks inside `tests:`",
                    stmt.source,
                );
                continue;
            };
            if example.name != "example" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown test block `{}`", example.name),
                    example.source,
                );
                continue;
            }
            let mut given: BTreeMap<String, ExprId> = BTreeMap::new();
            let mut expect: Option<ExprId> = None;
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        if !admitter.inputs.contains_key(name)
                            && !admitter.params.contains_key(name)
                            && !(is_model && admitter.states.contains_key(name))
                        {
                            admitter.error(
                                "E-NAME-026",
                                format!(
                                    "`given` name `{name}` is not an input, constructor parameter, or model state field"
                                ),
                                inner.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((
                                id,
                                Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Rat
                                | Infer::Complex
                                | Infer::Unit { .. }
                                | Infer::HostDeferred
                                | Infer::Vector { .. }
                                | Infer::Matrix { .. }
                                | Infer::Tensor { .. }
                                | Infer::OptionCarrier
                                | Infer::ResultCarrier,
                            )) => {
                                given.insert(name.clone(), id);
                            }
                            Some((
                                _,
                                Infer::Bool | Infer::Text | Infer::Set(_) | Infer::Record(_),
                            )) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric or tensor"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric; opaque host values are not scalars"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Series | Infer::Sequence)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric or tensor; a series is admitted data, not a scalar input"),
                                    inner.source,
                                );
                            }
                            None => {}
                        }
                    }
                    StmtKind::Expect(expr) => match admitter.lower_expr(expr) {
                        Some((id, Infer::Bool)) => {
                            // Multiple `expect` lines are a conjunction; keeping
                            // only the last one silently dropped earlier checks.
                            expect = Some(match expect {
                                Some(prev) => admitter.push_expr(
                                    ExprNode::Binary {
                                        operation: BinaryOp::And,
                                        left: prev,
                                        right: id,
                                    },
                                    inner.source,
                                ),
                                None => id,
                            });
                        }
                        Some((
                            _,
                            Infer::F64
                            | Infer::Nat
                            | Infer::Int
                            | Infer::Rat
                            | Infer::Complex
                            | Infer::Vector { .. }
                            | Infer::Matrix { .. }
                            | Infer::Tensor { .. }
                            | Infer::Unit { .. }
                            | Infer::HostDeferred
                            | Infer::Series
                            | Infer::Sequence
                            | Infer::Text
                            | Infer::Set(_)
                            | Infer::Record(_)
                            | Infer::OptionCarrier
                            | Infer::ResultCarrier
                            | Infer::Opaque,
                        )) => {
                            admitter.error(
                                "E-TYPE-012",
                                "`expect` must be a Boolean comparison",
                                inner.source,
                            );
                        }
                        None => {}
                    },
                    other => {
                        let _ = other;
                        admitter.error(
                            "E-SYN-101",
                            "only `given x = ...` and `expect ...` are allowed in example blocks",
                            inner.source,
                        );
                    }
                }
            }
            if is_policy || (is_model && !constructors.is_empty()) {
                // constructor parameters must be supplied by `given` values
                let constructor_params: Vec<String> = constructors
                    .first()
                    .map(|c| c.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();
                for param in &constructor_params {
                    if !given.contains_key(param) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "policy example `{}` must supply constructor parameter `{param}` via `given`",
                                example.generic.clone().unwrap_or_default()
                            ),
                            example.source,
                        );
                    }
                }
            }
            if is_model && constructors.is_empty() {
                for field in &state {
                    if !given.contains_key(&field.name) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "model example `{}` must supply state `{name}` via `given`",
                                example.generic.clone().unwrap_or_default(),
                                name = field.name
                            ),
                            example.source,
                        );
                    }
                }
            }
            tests.push(TestCase {
                name: example
                    .generic
                    .clone()
                    .unwrap_or_else(|| format!("test_{}", tests.len())),
                given,
                expect,
                source: example.source,
            });
        }
    }

    // Rebuild inputs/outputs/state as neutral fields.
    let input_fields = inputs.clone();
    let output_fields = outputs_raw.clone();
    let state_fields = state.clone();
    let known_bindings = input_fields
        .iter()
        .chain(&output_fields)
        .chain(&state_fields)
        .chain(&algebraic_fields)
        .map(|field| field.name.clone())
        .chain(definitions.keys().cloned())
        // Observations carry provenance too (04 §5.2): the instrument
        // run behind a measured datum is named like any other binding.
        .chain(observation_names.iter().cloned())
        .collect();
    let binding_provenance = if is_law {
        BTreeMap::new()
    } else {
        admit_binding_provenance(
            &mut admitter,
            by_name.get("provenance").copied(),
            &known_bindings,
        )
    };

    let about = admit_about(&mut admitter, by_name.get("about").copied());
    let mut evidence = admit_evidence(&mut admitter, by_name.get("evidence").copied());
    let law_metadata = is_law.then(|| {
        admit_law_metadata(
            &mut admitter,
            by_name.get("assumptions").copied(),
            by_name.get("domain").copied(),
            by_name.get("provenance").copied(),
            by_name.get("citations").copied(),
            decl.head_source,
        )
    });
    if let Some(metadata) = &law_metadata {
        if evidence.is_empty() {
            admitter.error(
                "E-LAW-002",
                "`emath law` requires at least one `evidence:` claim",
                decl.head_source,
            );
        }
        for claim in &mut evidence {
            claim.assumptions = metadata.assumptions.clone();
        }
    }
    let host = admit_host(&mut admitter, by_name.get("host").copied());

    let declaration = Declaration {
        id: emath_ir::DeclarationId(0),
        name: QualifiedName::single(decl.name.clone()),
        kind: QualifiedName::single(if is_policy {
            "policy"
        } else if is_model {
            "model"
        } else if is_law {
            "law"
        } else {
            "function"
        }),
        kind_label,
        inputs: input_fields,
        outputs: output_fields,
        state: state_fields,
        algebraic: algebraic_fields,
        constructors,
        definitions,
        invariants: admitter.constraints.clone(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports,
        compile_spec,
        about,
        evidence,
        host,
        source: decl.source,
    };

    (
        Some(declaration),
        tests,
        admitter.types,
        admitter.exprs,
        admitter.trace,
        admitter.diagnostics,
        admitter.residuals,
        admitter.events,
        admitter.transitions,
        law_metadata,
        binding_provenance,
    )
}
