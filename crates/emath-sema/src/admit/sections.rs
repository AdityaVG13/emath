//! Section admission: constructors, compile specs, domain directives,
//! and representation, extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    CommandArgument, Expr, ExprKind, Section, StmtKind, TypeExpr, UnaryOp as SynUnOp,
};
use emath_core::Span;
use emath_ir::constructor::{Constructor, Field, Visibility};
use emath_ir::goal::CompileSpec;
use emath_ir::{
    check_error_limit, check_precision_demand, parse_numeric_profile, ExprId, NumericProfile,
    SafetyProfile, TypeId, TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::{is_infer_marker, ty_display};
use super::expr_helpers::parse_float_constant;
use super::infer::{infer_from_node, Infer};
use super::types::{map_constructor_return, map_type};
use super::{expr_number, Admitter, E_DUPLICATE_FIELD};

/// Admits one `name: Type` (or untyped `Infer`) field; untyped names are
/// allowed only when `allow_infer` is set and default to Float64.
pub(super) fn admit_named_field(
    admitter: &mut Admitter,
    fields_infer: &mut BTreeMap<String, Infer>,
    fields_by_section: &mut BTreeMap<&str, Vec<Field>>,
    section_name: &'static str,
    name: &str,
    ty: &TypeExpr,
    span: Span,
    allow_infer: bool,
) -> bool {
    let (infer, ty_id) = if is_infer_marker(ty) {
        if !allow_infer {
            admitter.error(
                "E-SYN-101",
                format!("only `name: Type` declarations are allowed in `{section_name}`"),
                span,
            );
            return false;
        }
        let ty_id = admitter.type_id(TypeNode::Float64);
        admitter.note(
            "N-TYPE-001",
            format!("input `{name}` defaulted to Float64"),
            span,
        );
        admitter.record(
            "sema",
            format!("field `{name}` typed as Float64 (defaulted)"),
            span,
        );
        (Infer::F64, ty_id)
    } else {
        let Some(node) = map_type(ty, &mut admitter.diagnostics, &admitter.host_types) else {
            return false;
        };
        let infer = infer_from_node(&node);
        let ty_id = admitter.type_id(node);
        let ty_name = admitter
            .types
            .get(ty_id.index())
            .map(ty_display)
            .unwrap_or_else(|| format!("type#{}", ty_id.index()));
        admitter.record("sema", format!("field `{name}` typed as {ty_name}"), span);
        (infer, ty_id)
    };
    if fields_infer.contains_key(name) {
        admitter.error(
            E_DUPLICATE_FIELD,
            format!("duplicate field `{name}` (declared in section `{section_name}`)"),
            span,
        );
        return false;
    }
    fields_infer.insert(name.to_string(), infer);
    fields_by_section
        .entry(section_name)
        .or_default()
        .push(Field {
            name: name.to_string(),
            ty: ty_id,
            visibility: Visibility::Public,
            source: span,
        });
    true
}

impl Admitter {
    pub(super) fn type_of(&self, id: TypeId) -> Infer {
        self.types
            .get(id.index())
            .map(infer_from_node)
            .unwrap_or(Infer::F64)
    }
}

pub(super) fn admit_constructor(
    admitter: &mut Admitter,
    params: &[emath_core::tree::Param],
    ret: Option<&TypeExpr>,
    suite: Option<&emath_core::tree::Suite>,
    source: Span,
) -> Constructor {
    let mut parameters = Vec::new();
    let mut param_names = BTreeSet::new();
    for param in params {
        if !param_names.insert(param.name.clone()) {
            admitter.error(
                "E-CTOR-034",
                format!("duplicate constructor parameter `{}`", param.name),
                param.source,
            );
            continue;
        }
        let Some(node) = map_type(&param.ty, &mut admitter.diagnostics, &admitter.host_types)
        else {
            continue;
        };
        let infer = infer_from_node(&node);
        let ty_id = admitter.type_id(node);
        admitter.params.insert(param.name.clone(), infer);
        parameters.push(Field {
            name: param.name.clone(),
            ty: ty_id,
            visibility: Visibility::Public,
            source: param.source,
        });
    }

    let mut preconditions = Vec::new();
    let mut assignments: BTreeMap<String, ExprId> = BTreeMap::new();
    let mut postconditions = Vec::new();
    let mut error_type = None;
    if let Some(ret) = ret {
        if let Some(node) =
            map_constructor_return(ret, &mut admitter.diagnostics, &admitter.host_types)
        {
            error_type = Some(admitter.type_id(node));
        }
    }

    // Inputs are not visible while constructing: save and restore scopes.
    let saved_inputs = std::mem::take(&mut admitter.inputs);
    if let Some(suite) = suite {
        for stmt in &suite.statements {
            match &stmt.kind {
                StmtKind::Require(expr) => {
                    if let Some(id) = admitter.lower_requirement(expr) {
                        admitter.record(
                            "sema",
                            format!(
                                "constructor precondition #{} enforced",
                                preconditions.len() + 1
                            ),
                            stmt.source,
                        );
                        preconditions.push(id);
                    }
                }
                StmtKind::Ensure(expr) | StmtKind::Invariant(expr) => {
                    if let Some(id) = admitter.lower_requirement(expr) {
                        postconditions.push(id);
                    }
                }
                StmtKind::SelfBlock { assignments: block } => {
                    for (name, value) in block {
                        if !admitter.states.contains_key(name) {
                            admitter.error(
                                "E-CTOR-033",
                                format!("`{name}` is not a state field"),
                                stmt.source,
                            );
                            continue;
                        }
                        if assignments.contains_key(name) {
                            admitter.error(
                                "E-CTOR-035",
                                format!("duplicate assignment for state field `{name}`"),
                                stmt.source,
                            );
                            continue;
                        }
                        // state references are not readable during construction
                        if contains_state_reference(value) {
                            admitter.error(
                                "E-CTOR-033",
                                format!(
                                    "constructor cannot read `state.{name}` while constructing"
                                ),
                                value.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((
                                id,
                                Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Complex
                                | Infer::Unit { .. }
                                | Infer::HostDeferred
                                | Infer::Vector { .. }
                                | Infer::Matrix { .. }
                                | Infer::Tensor { .. },
                            )) => {
                                assignments.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("state field `{name}` must be numeric or tensor"),
                                    value.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("state field `{name}` must be numeric; opaque host values are not scalars"),
                                    value.source,
                                );
                            }
                            None => {}
                        }
                    }
                }
                other => {
                    let _ = other;
                    admitter.error(
                        "E-SYN-101",
                        "only `require`, `ensure`, `invariant` and `Self:` blocks are allowed in constructors",
                        stmt.source,
                    );
                }
            }
        }
    }
    admitter.inputs = saved_inputs;
    Constructor {
        name: "new".to_string(),
        parameters,
        preconditions,
        assignments,
        postconditions,
        defaults: BTreeMap::new(),
        error_type,
        is_public: true,
        source,
    }
}

pub(super) fn contains_state_reference(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.first().is_some_and(|s| s == "state"),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Rational { .. }
        | ExprKind::Bool(_)
        | ExprKind::Str(_) => false,
        ExprKind::Quantity { value, .. } | ExprKind::Unary { value, .. } => {
            contains_state_reference(value)
        }
        ExprKind::Call { function, args } => {
            contains_state_reference(function) || args.iter().any(contains_state_reference)
        }
        ExprKind::Binary { left, right, .. } => {
            contains_state_reference(left) || contains_state_reference(right)
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            contains_state_reference(condition)
                || contains_state_reference(then_value)
                || contains_state_reference(else_value)
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            items.iter().any(contains_state_reference)
        }
        ExprKind::Index { value, indices } => {
            contains_state_reference(value) || indices.iter().any(contains_state_reference)
        }
        ExprKind::Slice { start, end } => {
            start.as_ref().is_some_and(|e| contains_state_reference(e))
                || end.as_ref().is_some_and(|e| contains_state_reference(e))
        }
        ExprKind::Range { start, end, .. } => {
            start.as_ref().is_some_and(|e| contains_state_reference(e))
                || end.as_ref().is_some_and(|e| contains_state_reference(e))
        }
        ExprKind::Binder { binders, body, .. } => {
            binders
                .iter()
                .any(|b| b.domain.as_ref().is_some_and(contains_state_reference))
                || contains_state_reference(body)
        }
        ExprKind::Derivative { value, wrt, .. }
        | ExprKind::Solve { value, wrt }
        | ExprKind::Optimize { value, wrt, .. } => {
            contains_state_reference(value)
                || wrt
                    .as_ref()
                    .is_some_and(|v| v.iter().any(contains_state_reference))
        }
        ExprKind::At { value, location } | ExprKind::On { value, location } => {
            contains_state_reference(value) || contains_state_reference(location)
        }
        ExprKind::Conditioned { value, condition } => {
            contains_state_reference(value) || contains_state_reference(condition)
        }
        ExprKind::UnitQuery { expr, .. } => contains_state_reference(expr),
        ExprKind::Limit { target, body, .. } | ExprKind::SampleLimit { target, body, .. } => {
            contains_state_reference(target) || contains_state_reference(body)
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            subject
                .as_ref()
                .is_some_and(|s| contains_state_reference(s))
                || arms
                    .iter()
                    .any(|(c, v)| contains_state_reference(c) || contains_state_reference(v))
                || contains_state_reference(else_arm)
        }
    }
}

pub(super) fn admit_compile_spec(
    admitter: &mut Admitter,
    section: Option<&Section>,
) -> CompileSpec {
    let mut spec = CompileSpec {
        target: "rust".into(),
        profile: "library".into(),
        numeric: NumericProfile::default_phase1(),
        safety: SafetyProfile::ForbidUnsafe,
        unresolved: None,
    };
    let Some(section) = section else {
        admitter.record(
            "sema",
            "compile section absent; defaults: rust/library/strict-f64/forbid-unsafe",
            Span::default(),
        );
        return spec;
    };
    for stmt in &section.suite.statements {
        let StmtKind::Command { head, argument } = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "compile directives must be commands (e.g. `target rust`)",
                stmt.source,
            );
            continue;
        };
        let key = head.first().map_or("", String::as_str);
        let value_text = match argument {
            Some(CommandArgument::Expr(expr)) => match &expr.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                ExprKind::Int(text) | ExprKind::Float(text) => Some(text.clone()),
                _ => None,
            },
            _ => None,
        }
        .or_else(|| head.get(1).cloned());
        let value_text = value_text.unwrap_or_default();
        match key {
            "target" => {
                if value_text != "rust" {
                    admitter.error(
                        "E-CODEGEN-051",
                        format!(
                            "compile target `{value_text}` is outside the Phase 1 subset (rust)"
                        ),
                        stmt.source,
                    );
                }
                spec.target = value_text;
            }
            "profile" => {
                if value_text != "library" {
                    admitter.error(
                        "E-CODEGEN-052",
                        format!(
                            "compile profile `{value_text}` is outside the Phase 1 subset (library)"
                        ),
                        stmt.source,
                    );
                }
                spec.profile = value_text;
            }
            "numeric" => match parse_numeric_profile(&value_text) {
                Ok(profile) => {
                    spec.numeric = profile;
                    admitter.record(
                        "sema",
                        format!("numeric model `{}`", profile.as_str()),
                        stmt.source,
                    );
                }
                Err(error) => admitter.error(error.code, error.message, stmt.source),
            },
            "precision" => {
                let Some(bits) = command_u16(argument.as_ref(), &value_text) else {
                    admitter.error(
                        "E-NUM-002",
                        format!("precision demand `{value_text}` is not a bit count"),
                        stmt.source,
                    );
                    continue;
                };
                if let Err(error) = check_precision_demand(spec.numeric, bits) {
                    admitter.error(error.code, error.message, stmt.source);
                }
            }
            "error-limit" => {
                let Some(limit) = command_f64(argument.as_ref(), &value_text) else {
                    admitter.error(
                        "E-NUM-003",
                        format!("error-limit `{value_text}` is not a finite bound"),
                        stmt.source,
                    );
                    continue;
                };
                if let Err(error) = check_error_limit(spec.numeric, limit) {
                    admitter.error(error.code, error.message, stmt.source);
                }
            }
            "domain" => {
                if !admit_domain_directive(admitter, argument.as_ref(), stmt.source) {
                    continue;
                }
            }
            "representation" => {
                admit_representation(admitter, &mut spec, head, argument.as_ref(), stmt.source);
            }
            "safety" => {
                if value_text != "forbid-unsafe" {
                    admitter.error(
                        "E-CODEGEN-054",
                        format!(
                            "safety profile `{value_text}` is outside the Phase 1 subset (forbid-unsafe)"
                        ),
                        stmt.source,
                    );
                }
                spec.safety = SafetyProfile::ForbidUnsafe;
            }
            "unresolved" => {
                if value_text != "parametric" {
                    admitter.error(
                        "E-CODEGEN-055",
                        format!(
                            "`unresolved {value_text}` is outside the Phase 1 subset (parametric)"
                        ),
                        stmt.source,
                    );
                } else {
                    admitter.record(
                        "sema",
                        "compile unresolved parametric: host types stay host-deferred",
                        stmt.source,
                    );
                }
                spec.unresolved = Some(value_text);
            }
            other => {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown compile directive `{other}`"),
                    stmt.source,
                );
            }
        }
    }
    spec
}

pub(super) fn command_u16(argument: Option<&CommandArgument>, fallback: &str) -> Option<u16> {
    command_f64(argument, fallback).and_then(|value| {
        if value.is_finite()
            && value >= 0.0
            && value == value.trunc()
            && value <= f64::from(u16::MAX)
        {
            Some(value as u16)
        } else {
            None
        }
    })
}

pub(super) fn command_f64(argument: Option<&CommandArgument>, fallback: &str) -> Option<f64> {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => parse_float_constant(text),
            ExprKind::Unary {
                op: SynUnOp::Neg,
                value,
            } => match &value.kind {
                ExprKind::Int(text) | ExprKind::Float(text) => {
                    parse_float_constant(text).map(|value| -value)
                }
                _ => None,
            },
            _ => parse_float_constant(fallback),
        },
        _ => parse_float_constant(fallback),
    }
}

pub(super) fn admit_domain_directive(
    admitter: &mut Admitter,
    argument: Option<&CommandArgument>,
    span: Span,
) -> bool {
    let Some(CommandArgument::Expr(expr)) = argument else {
        admitter.error(
            "E-DOM-002",
            "domain directive requires an interval `lo..hi`",
            span,
        );
        return false;
    };
    let ExprKind::Range {
        start: Some(start),
        end: Some(end),
        ..
    } = &expr.kind
    else {
        admitter.error(
            "E-DOM-002",
            "domain directive requires a bounded interval `lo..hi`",
            span,
        );
        return false;
    };
    let (Some(low), Some(high)) = (expr_number(start), expr_number(end)) else {
        admitter.error("E-DOM-002", "domain bounds must be numeric literals", span);
        return false;
    };
    match emath_ir::Interval::checked(low, high) {
        Ok(_) => true,
        Err(error) => {
            admitter.error(error.code, error.message, span);
            false
        }
    }
}

pub(super) fn integer_range(expr: &Expr) -> Option<(i64, i64)> {
    let ExprKind::Range {
        start,
        end,
        inclusive,
    } = &expr.kind
    else {
        return None;
    };
    let start = start.as_ref().and_then(|expr| integer_bound(expr))?;
    let end = end.as_ref().and_then(|expr| integer_bound(expr))?;
    let end = if *inclusive { end.checked_add(1)? } else { end };
    Some((start, end))
}

pub(super) fn integer_bound(expr: &Expr) -> Option<i64> {
    let value = expr_number(expr)?;
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    Some(value as i64)
}

pub(super) fn restore_index_local(
    locals: &mut BTreeMap<String, i64>,
    name: &str,
    previous: Option<i64>,
) {
    match previous {
        Some(value) => {
            locals.insert(name.to_string(), value);
        }
        None => {
            locals.remove(name);
        }
    }
}

pub(super) fn restore_input(
    locals: &mut BTreeMap<String, Infer>,
    name: &str,
    previous: Option<Infer>,
) {
    match previous {
        Some(infer) => {
            locals.insert(name.to_string(), infer);
        }
        None => {
            locals.remove(name);
        }
    }
}

pub(super) fn admit_representation(
    admitter: &mut Admitter,
    spec: &mut CompileSpec,
    head: &[String],
    argument: Option<&CommandArgument>,
    span: Span,
) {
    let source_name = head.get(1).map(String::as_str).unwrap_or("");
    let model_name = head.get(2).cloned().or_else(|| match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => Some(segments.join(".")),
            ExprKind::Call { function, .. } => match &function.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    });
    if source_name == "Real" && model_name.is_none() {
        admitter.error(
            "E-NUM-004",
            "do not map `Real` to `f64` without profile evidence (`representation Real => Float64` or `numeric strict-f64`)",
            span,
        );
        return;
    }
    let Some(model_name) = model_name.or_else(|| {
        if source_name.is_empty() {
            None
        } else {
            Some(source_name.to_string())
        }
    }) else {
        admitter.error(
            "E-NUM-004",
            "`representation` requires a named numeric model (Float64 or Interval)",
            span,
        );
        return;
    };
    match parse_numeric_profile(&model_name) {
        Ok(profile) => {
            spec.numeric = profile;
            admitter.record(
                "sema",
                format!(
                    "representation evidence: {} → {}",
                    if source_name.is_empty() {
                        "declared"
                    } else {
                        source_name
                    },
                    profile.as_str()
                ),
                span,
            );
        }
        Err(error) => admitter.error(error.code, error.message, span),
    }
}
