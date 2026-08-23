//! Section admission: constructors, compile specs, domain directives,
//! representation, about, evidence, host, and the top-level `check_tree`
//! entry point, extracted from `admit.rs` isomorphically.

use emath_core::tree::{
    CommandArgument, Expr, ExprKind, Section, StmtKind, SyntaxTree, TypeExpr,
    UnaryOp as SynUnOp,
};
use emath_core::{Diagnostics, Span};
use emath_ir::constructor::{Constructor, Field, Visibility};
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::{CompileSpec, EvidenceLevel};
use emath_ir::{
    ExprId, HostBinding, HostMethod, ImportEntry, ImportSelection, NumericProfile,
    SafetyProfile, TypeId, TypeNode, check_error_limit, check_precision_demand,
    parse_numeric_profile,
};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::{is_infer_marker, ty_display};
use super::expr_helpers::parse_float_constant;
use super::infer::{Infer, infer_from_node};
use super::types::{map_type, type_display};
use super::{
    admit_declaration, expr_number, Admitter, CheckResult, SemanticTrace, confusable_fold,
    E_DUPLICATE_FIELD,
};

/// Admits one `name: Type` (or untyped `Infer`) field into the structural
/// maps used for `inputs` / `outputs` / `state`. Untyped names are allowed
/// only when `allow_infer` is set (bare `inputs:` fields and head-args);
/// they default to Float64 and emit `N-TYPE-001`.
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
        admitter.record(
            "sema",
            format!("field `{name}` typed as {ty_name}"),
            span,
        );
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
        let Some(node) = map_type(&param.ty, &mut admitter.diagnostics, &admitter.host_types) else {
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
        if let Some(node) = map_type(ret, &mut admitter.diagnostics, &admitter.host_types) {
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
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => false,
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
        ExprKind::Derivative { value, wrt }
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
    }
}

pub(super) fn admit_compile_spec(admitter: &mut Admitter, section: Option<&Section>) -> CompileSpec {
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
        if value.is_finite() && value >= 0.0 && value == value.trunc() && value <= f64::from(u16::MAX)
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
        admitter.error("E-DOM-002", "domain directive requires an interval `lo..hi`", span);
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
        admitter.error(
            "E-DOM-002",
            "domain bounds must be numeric literals",
            span,
        );
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

pub(super) fn restore_input(locals: &mut BTreeMap<String, Infer>, name: &str, previous: Option<Infer>) {
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

pub(super) fn host_imported_types(imports: &[ImportEntry]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for import in imports {
        if import.path.first().map(String::as_str) != Some("host") {
            continue;
        }
        if let ImportSelection::Named(pairs) = &import.selection {
            for (name, alias) in pairs {
                names.insert(alias.clone().unwrap_or_else(|| name.clone()));
            }
        }
    }
    names
}

pub(super) fn admit_about(admitter: &mut Admitter, section: Option<&Section>) -> Option<String> {
    let section = section?;
    let mut summary = None;
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("summary") => {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Str(text) = &expr.kind {
                        summary = Some(text.clone());
                        admitter.record("sema", "about summary retained", expr.source);
                        continue;
                    }
                }
                admitter.error(
                    "E-SYN-101",
                    "`about.summary` must be a string literal",
                    stmt.source,
                );
            }
            _ => {
                admitter.error(
                    "E-SYN-101",
                    "`about:` admits `summary: \"...\"` in Phase 1",
                    stmt.source,
                );
            }
        }
    }
    summary
}

pub(super) fn admit_evidence(admitter: &mut Admitter, section: Option<&Section>) -> Vec<EvidenceClaim> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut claims = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(claim) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected `claim <name>:` blocks inside `evidence:`",
                stmt.source,
            );
            continue;
        };
        if claim.name != "claim" {
            admitter.error(
                "E-SYN-101",
                format!("unknown evidence block `{}`", claim.name),
                claim.head_source,
            );
            continue;
        }
        let id = claim.generic.clone().unwrap_or_default();
        if id.is_empty() {
            admitter.error(
                "E-SYN-101",
                "`claim` requires a name in `<...>`",
                claim.head_source,
            );
            continue;
        }
        let mut statement = String::new();
        let mut class = String::new();
        for inner in &claim.suite.statements {
            match &inner.kind {
                StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("statement") => {
                    statement = match argument {
                        Some(CommandArgument::Expr(expr)) => expr_text(expr),
                        _ if head.len() > 1 => head[1..].join(" "),
                        _ => String::new(),
                    };
                }
                StmtKind::Require(expr) => {
                    class = expr_text(expr);
                }
                StmtKind::Command { head, .. } if head.first().map(String::as_str) == Some("require") => {
                    class = head.get(1).cloned().unwrap_or_default();
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "evidence claims admit `statement ...` and `require ...`",
                        inner.source,
                    );
                }
            }
        }
        admitter.record(
            "sema",
            format!("evidence claim `{id}` recorded (verdict not-run)"),
            claim.head_source,
        );
        claims.push(EvidenceClaim {
            id,
            statement,
            class,
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: ClaimVerdict::NotRun,
            level: EvidenceLevel::E1,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    claims
}

pub(super) fn admit_host(admitter: &mut Admitter, section: Option<&Section>) -> Vec<HostBinding> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut bindings = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(language) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected a language section (`rust:`) inside `host:`",
                stmt.source,
            );
            continue;
        };
        for inner in &language.suite.statements {
            let StmtKind::Section(implement) = &inner.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `implement Trait for Type:` inside `host:`",
                    inner.source,
                );
                continue;
            };
            if implement.name != "implement" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown host block `{}`", implement.name),
                    implement.head_source,
                );
                continue;
            }
            let generic = implement.generic.clone().unwrap_or_default();
            let (trait_path, target) = match generic.rsplit_once("::") {
                Some((trait_path, target)) => (trait_path.to_string(), target.to_string()),
                None => (generic, String::new()),
            };
            let mut methods = Vec::new();
            for method_stmt in &implement.suite.statements {
                let StmtKind::FnDecl {
                    name, params, ret, suite, ..
                } = &method_stmt.kind
                else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `method name(...)` inside `implement`",
                        method_stmt.source,
                    );
                    continue;
                };
                let mut body = Vec::new();
                if let Some(suite) = suite {
                    for body_stmt in &suite.statements {
                        body.push(stmt_text(body_stmt));
                    }
                }
                methods.push(HostMethod {
                    name: name.clone(),
                    params: params
                        .iter()
                        .map(|param| {
                            let ty = type_display(&param.ty);
                            let ty = if param.by_ref {
                                format!("&{ty}")
                            } else {
                                ty
                            };
                            (param.name.clone(), ty)
                        })
                        .collect(),
                    ret: ret.as_ref().map(type_display),
                    body,
                });
            }
            admitter.record(
                "sema",
                format!(
                    "host binding `{}/{}` retained (trait impl codegen is a Phase 1 no-claim)",
                    language.name, trait_path
                ),
                implement.head_source,
            );
            bindings.push(HostBinding {
                language: language.name.clone(),
                trait_path,
                target,
                methods,
            });
        }
    }
    bindings
}

pub(super) fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.join("."),
        ExprKind::Call { function, args } => {
            format!(
                "{}({})",
                expr_text(function),
                args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Str(text) => format!("\"{text}\""),
        ExprKind::Int(text) | ExprKind::Float(text) => text.clone(),
        ExprKind::Bool(value) => value.to_string(),
        _ => "expr".to_string(),
    }
}

pub(super) fn stmt_text(stmt: &emath_core::tree::Stmt) -> String {
    match &stmt.kind {
        StmtKind::Command { head, argument } => {
            let mut text = head.join(" ");
            if let Some(argument) = argument {
                text.push(' ');
                text.push_str(&command_argument_text(argument));
            }
            text
        }
        _ => "stmt".to_string(),
    }
}

pub(super) fn command_argument_text(argument: &CommandArgument) -> String {
    match argument {
        CommandArgument::Expr(expr) => expr_text(expr),
        CommandArgument::Assignment { name, value } => {
            format!("{name} = {}", expr_text(value))
        }
        CommandArgument::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    // Front-end: package identity and `use` imports. External file
    // imports remain a Phase 2 refusal (E-PKG-050).
    let has_recognition_items = tree.items.iter().any(|item| match item {
        emath_core::tree::Item::Package { .. } | emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => decl.item_kind != "custom",
    });
    let recognition = if has_recognition_items {
        let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
        package.package_path = front_end.package_path;
        package.imports = front_end.imports;
        Some(crate::recognition::collect_kind_defs(tree))
    } else {
        None
    };
    let host_types = host_imported_types(&package.imports);

    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if let Some(kind_defs) = &recognition {
            if decl.item_kind != "custom" {
                crate::recognition::admit_declaration(
                    decl,
                    kind_defs,
                    &mut package,
                    &mut diagnostics,
                    &mut trace,
                );
                continue;
            }
        }
        // Duplicate declaration names are a typed refusal (E-NAME-022):
        // two `custom <Foo>` declarations would collide in generated
        // Rust, so the second is never admitted.
        if !seen_declaration_names.insert(decl.name.clone()) {
            diagnostics.error(
                "E-NAME-022",
                format!("duplicate declaration name `{}`", decl.name),
                decl.head_source,
            );
            continue;
        }
        // `_` is not a valid Rust type name and cannot be escaped; a
        // declaration named `_` is refused up front (E-NAME-023).
        if decl.name == "_" {
            diagnostics.error(
                "E-NAME-023",
                "declaration name `_` is reserved and cannot be a Rust type",
                decl.head_source,
            );
            continue;
        }
        // Confusable identity (spec `01_LEXICAL_LAYOUT_AND_SOURCE`): a
        // declaration name that differs from an already-seen one only by
        // lookalike glyphs (Latin `o` vs Cyrillic `о`) is refused
        // (E-NAME-024) — the public API would present two visually
        // indistinguishable names.
        let folded = confusable_fold(&decl.name);
        if let Some(existing) = seen_folded_declaration_names.get(&folded) {
            diagnostics.error(
                "E-NAME-024",
                format!(
                    "declaration name `{}` is confusable with `{existing}` and is refused",
                    decl.name
                ),
                decl.head_source,
            );
            continue;
        }
        seen_folded_declaration_names.insert(folded, decl.name.clone());
        if decl.item_kind != "custom" {
            diagnostics.error(
                "E-KIND-001",
                format!(
                    "declaration kind `{}` is not supported; Phase 1 uses `emath custom`",
                    decl.item_kind
                ),
                decl.head_source,
            );
            continue;
        }
        if decl.as_kind != "function" && decl.as_kind != "policy" && decl.as_kind != "model" {
            diagnostics.error(
                "E-KIND-100",
                format!(
                    "declaration type `{}` is outside the Phase 1 subset (function, policy, model)",
                    decl.as_kind
                ),
                decl.head_source,
            );
            continue;
        }
        let (declaration, tests, types, exprs, entries, admit_diagnostics, residuals) =
            admit_declaration(decl, &host_types);
        diagnostics.extend_from(&admit_diagnostics);
        trace.entries.extend(entries);
        let Some(mut declaration) = declaration else {
            diagnostics.error(
                "E-KIND-002",
                "declaration could not be admitted",
                decl.head_source,
            );
            continue;
        };
        declaration.id = emath_ir::DeclarationId(declaration_id);
        declaration_id += 1;
        if !residuals.is_empty() {
            package.residuals.insert(declaration.id, residuals);
        }
        package.types.extend(types);
        package.exprs.extend(exprs.iter().map(|(e, _)| e.clone()));
        package.expr_spans.extend(exprs.iter().map(|(_, s)| *s));
        for test in tests {
            declaration.tests.push(package.push_test(test));
        }
        package.declarations.push(declaration);
    }
    if !package.declarations.is_empty() {
        package.seal();
    }
    CheckResult {
        package,
        diagnostics,
        trace,
    }
}
