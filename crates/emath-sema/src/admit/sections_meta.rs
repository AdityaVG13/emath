//! Meta section admission: about, evidence, host bindings, text helpers,
//! and the top-level `check_tree` entry point, extracted from `sections.rs`
//! isomorphically.

use emath_core::Diagnostics;
use emath_core::tree::{CommandArgument, Expr, ExprKind, Section, StmtKind, SyntaxTree};
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::EvidenceLevel;
use emath_ir::ids::{ExprId, TypeId};
use emath_ir::{EventDecl, ExprNode, LawMetadata, Provenance, SliceAxis, TransitionDecl};
use emath_ir::{HostBinding, HostMethod, ImportEntry, ImportSelection, ModelResidual};
use std::collections::{BTreeMap, BTreeSet};

use super::types::{map_type, type_display};
use super::{
    Admitter, CheckResult, SemanticTrace, SiblingFunction, admit_declaration, confusable_fold,
};
use super::infer::infer_from_node;
use super::equations::is_infer_marker;

/// Offset all child ExprIds and TypeIds in one node from the admitter's
/// local arena into the package's global index space.
fn remap_expr_node(node: &mut ExprNode, expr_offset: u32, type_offset: u32) {
    let remap_e = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_t = |id: &mut TypeId| {
        id.0 += type_offset;
    };
    match node {
        ExprNode::Literal(_) | ExprNode::Variable(_) => {}
        // A series data constant carries no expr ids to remap (04 §5.4
        // slice 1): the pairs are inline f64s and the policy is text.
        ExprNode::Series { .. } => {}
        ExprNode::Call { arguments, .. } => {
            for id in arguments {
                remap_e(id);
            }
        }
        ExprNode::Unary { value, .. } => remap_e(value),
        ExprNode::Binary { left, right, .. } => {
            remap_e(left);
            remap_e(right);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            remap_e(condition);
            remap_e(then_value);
            remap_e(else_value);
        }
        ExprNode::Record { ty, fields } => {
            remap_t(ty);
            for (_, id) in fields {
                remap_e(id);
            }
        }
        ExprNode::Index { value, indices } => {
            remap_e(value);
            for id in indices {
                remap_e(id);
            }
        }
        ExprNode::Slice { value, axes } => {
            remap_e(value);
            for axis in axes.iter_mut() {
                match axis {
                    SliceAxis::Point(id) => remap_e(id),
                    SliceAxis::Range { start, end } => {
                        remap_e(start);
                        remap_e(end);
                    }
                }
            }
        }
        ExprNode::Binder { body, .. } => remap_e(body),
        ExprNode::Vector(ids) => {
            for id in ids {
                remap_e(id);
            }
        }
        ExprNode::Set { elements, guards } => {
            for id in elements {
                remap_e(id);
            }
            for id in guards.iter_mut().flatten() {
                remap_e(id);
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows.iter_mut() {
                for id in row.iter_mut() {
                    remap_e(id);
                }
            }
        }
        ExprNode::Tensor { elements, .. } => {
            for id in elements {
                remap_e(id);
            }
        }
        ExprNode::Differentiate { body, .. } => remap_e(body),
        ExprNode::Solve { body, .. } => remap_e(body),
        ExprNode::Optimize { body, .. } => remap_e(body),
        ExprNode::SampleLimit {
            body,
            target,
            direction,
            ..
        } => {
            remap_e(target);
            remap_e(direction);
            remap_e(body);
        }
        ExprNode::Apply { arguments, .. } => {
            for id in arguments {
                remap_e(id);
            }
        }
    }
}

/// Offset all ExprIds and TypeIds in a declaration and its test cases into
/// the package's global index space.
fn remap_ids(
    declaration: &mut emath_ir::Declaration,
    tests: &mut [emath_ir::constructor::TestCase],
    residuals: &mut [ModelResidual],
    events: &mut [EventDecl],
    transitions: &mut [TransitionDecl],
    expr_offset: u32,
    type_offset: u32,
) {
    let remap_expr = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_type = |id: &mut TypeId| {
        id.0 += type_offset;
    };

    // Definitions
    for (_, id) in &mut declaration.definitions {
        remap_expr(id);
    }
    // Invariants
    for id in &mut declaration.invariants {
        remap_expr(id);
    }
    // Inputs / outputs / state: Field ty
    for field in &mut declaration.inputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.outputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.state {
        remap_type(&mut field.ty);
    }
    // Constructors
    for ctor in &mut declaration.constructors {
        for id in &mut ctor.preconditions {
            remap_expr(id);
        }
        for (_, id) in &mut ctor.assignments {
            remap_expr(id);
        }
        for id in &mut ctor.postconditions {
            remap_expr(id);
        }
        for (_, id) in &mut ctor.defaults {
            remap_expr(id);
        }
        if let Some(id) = &mut ctor.error_type {
            remap_type(id);
        }
    }
    // Evidence claims: no ExprId fields, only string metadata
    // Constructors
    for test in tests.iter_mut() {
        for (_, id) in &mut test.given {
            remap_expr(id);
        }
        if let Some(id) = &mut test.expect {
            remap_expr(id);
        }
    }
    // Model residuals
    for residual in residuals.iter_mut() {
        remap_expr(&mut residual.expr);
    }
    // Hybrid event rules (r3-dynamical-03lh ch7): condition and action
    // expressions live in the same expression arena.
    for event in events.iter_mut() {
        remap_expr(&mut event.condition);
        remap_expr(&mut event.action.expr);
    }
    // Hybrid transition rules (r3-dynamical-03lh ch7): each action's
    // expression lives in the same expression arena.
    for transition in transitions.iter_mut() {
        for action in &mut transition.actions {
            remap_expr(&mut action.expr);
        }
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
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("summary") =>
            {
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

pub(super) fn admit_evidence(
    admitter: &mut Admitter,
    section: Option<&Section>,
) -> Vec<EvidenceClaim> {
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
        let mut level = EvidenceLevel::E1;
        let mut has_level = false;
        for inner in &claim.suite.statements {
            match &inner.kind {
                StmtKind::Command { head, argument }
                    if head.first().map(String::as_str) == Some("statement") =>
                {
                    statement = match argument {
                        Some(CommandArgument::Expr(expr)) => expr_text(expr),
                        _ if head.len() > 1 => head[1..].join(" "),
                        _ => String::new(),
                    };
                }
                StmtKind::Require(expr) => {
                    class = expr_text(expr);
                }
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("require") =>
                {
                    class = head.get(1).cloned().unwrap_or_default();
                }
                StmtKind::Command { head, argument }
                    if head.first().map(String::as_str) == Some("level") =>
                {
                    if has_level {
                        admitter.error(
                            "E-SYN-103",
                            "evidence claim declares `level` more than once",
                            inner.source,
                        );
                        continue;
                    }
                    has_level = true;
                    let value = head.get(1).cloned().or_else(|| match argument {
                        Some(CommandArgument::Expr(expr)) => Some(expr_text(expr)),
                        _ => None,
                    });
                    match value.as_deref().and_then(|value| value.parse().ok()) {
                        Some(parsed) => level = parsed,
                        None => admitter.error(
                            "E-EVID-115",
                            "unknown evidence level; expected E0, E1, E2, E3, E4, or E5",
                            inner.source,
                        ),
                    }
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "evidence claims admit `statement ...`, `require ...`, and `level E0` through `level E5`",
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
            level,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    claims
}

fn law_entries(
    admitter: &mut Admitter,
    section: Option<&Section>,
    section_name: &str,
    command: &str,
    missing_span: emath_core::Span,
) -> Vec<String> {
    let Some(section) = section else {
        admitter.error(
            "E-LAW-002",
            format!("`emath law` requires a `{section_name}:` section"),
            missing_span,
        );
        return Vec::new();
    };
    let mut entries = Vec::new();
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command {
                head,
                argument: Some(CommandArgument::Expr(expr)),
            } if head.first().map(String::as_str) == Some(command)
                && matches!(&expr.kind, ExprKind::Str(_)) =>
            {
                let ExprKind::Str(value) = &expr.kind else {
                    unreachable!()
                };
                entries.push(value.clone());
            }
            StmtKind::Require(_) if section_name == "assumptions" => {}
            _ => admitter.error(
                "E-SYN-101",
                format!("`{section_name}:` admits `{command} \"...\"` entries"),
                stmt.source,
            ),
        }
    }
    if entries.is_empty() {
        admitter.error(
            "E-LAW-002",
            format!("`emath law` requires at least one `{command} \"...\"` entry"),
            section.source,
        );
    }
    entries
}

pub(super) fn admit_law_metadata(
    admitter: &mut Admitter,
    assumptions: Option<&Section>,
    domain: Option<&Section>,
    provenance: Option<&Section>,
    citations: Option<&Section>,
    declaration_span: emath_core::Span,
) -> LawMetadata {
    let assumptions = law_entries(
        admitter,
        assumptions,
        "assumptions",
        "assume",
        declaration_span,
    );
    let domains = law_entries(admitter, domain, "domain", "name", declaration_span);
    if domains.len() > 1 {
        admitter.error(
            "E-LAW-002",
            "`domain:` requires exactly one `name \"...\"` entry",
            domain.map_or(emath_core::Span::default(), |section| section.source),
        );
    }
    LawMetadata {
        assumptions,
        domain: domains.into_iter().next().unwrap_or_default(),
        provenance: law_entries(
            admitter,
            provenance,
            "provenance",
            "source",
            declaration_span,
        ),
        citations: law_entries(admitter, citations, "citations", "cite", declaration_span),
    }
}

fn required_provenance_value(
    admitter: &mut Admitter,
    values: &BTreeMap<String, String>,
    key: &str,
    binding: &str,
    span: emath_core::Span,
) -> Option<String> {
    values
        .get(key)
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| {
            admitter.error(
                "E-SYN-152",
                format!("provenance for `{binding}` requires a non-empty `{key}: \"...\"`"),
                span,
            );
            None
        })
}

/// Admit declaration-local provenance keyed by binding name.
///
/// Shape:
/// `provenance: / <binding>: / kind: "Citation" / reference: "doi:..."`.
pub(super) fn admit_binding_provenance(
    admitter: &mut Admitter,
    section: Option<&Section>,
    known_bindings: &BTreeSet<String>,
) -> BTreeMap<String, Provenance> {
    let Some(section) = section else {
        return BTreeMap::new();
    };
    let mut admitted = BTreeMap::new();
    let mut seen_bindings = BTreeSet::new();
    for statement in &section.suite.statements {
        let StmtKind::Section(binding_section) = &statement.kind else {
            admitter.error(
                "E-SYN-152",
                "`provenance:` entries must be binding-named sections",
                statement.source,
            );
            continue;
        };
        let binding = binding_section.name.as_str();
        if !known_bindings.contains(binding) {
            admitter.error(
                "E-NAME-028",
                format!("provenance names unknown binding `{binding}`"),
                binding_section.head_source,
            );
            continue;
        }
        if !seen_bindings.insert(binding.to_string()) {
            admitter.error(
                "E-SYN-103",
                format!("duplicate provenance for binding `{binding}`"),
                binding_section.head_source,
            );
            continue;
        }

        let mut values = BTreeMap::new();
        for entry in &binding_section.suite.statements {
            let StmtKind::Command {
                head,
                argument: Some(CommandArgument::Expr(expr)),
            } = &entry.kind
            else {
                admitter.error(
                    "E-SYN-152",
                    "provenance fields use `key: \"value\"`",
                    entry.source,
                );
                continue;
            };
            let Some(key) = head.first() else {
                admitter.error("E-SYN-152", "empty provenance key", entry.source);
                continue;
            };
            if !matches!(
                key.as_str(),
                "kind"
                    | "source"
                    | "reference"
                    | "adjustment"
                    | "file"
                    | "processing"
                    | "fit_id"
                    | "reason"
                    // 04 §5.2 (emath-r3-observations-9ffu): declared digest
                    // of the raw data file; re-hashed by --verify-data.
                    | "sha256"
            ) {
                admitter.error(
                    "E-SYN-152",
                    format!("unknown provenance key `{key}`"),
                    entry.source,
                );
                continue;
            }
            let ExprKind::Str(value) = &expr.kind else {
                admitter.error(
                    "E-SYN-152",
                    format!("provenance key `{key}` requires a string value"),
                    expr.source,
                );
                continue;
            };
            if values.insert(key.clone(), value.clone()).is_some() {
                admitter.error(
                    "E-SYN-103",
                    format!("duplicate provenance key `{key}` for `{binding}`"),
                    entry.source,
                );
            }
        }

        let Some(kind) =
            required_provenance_value(admitter, &values, "kind", binding, binding_section.source)
        else {
            continue;
        };
        let (provenance, allowed): (Option<Provenance>, &[&str]) = match kind
            .to_ascii_lowercase()
            .as_str()
        {
            "exact" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "source",
                    binding,
                    binding_section.source,
                )
                .map(|source| Provenance::Exact { source }),
                &["kind", "source"],
            ),
            "citation" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "reference",
                    binding,
                    binding_section.source,
                )
                .map(|reference| Provenance::Citation {
                    reference,
                    adjustment: values.get("adjustment").cloned(),
                }),
                &["kind", "reference", "adjustment"],
            ),
            "instrumentrun" | "instrument_run" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "file",
                    binding,
                    binding_section.source,
                )
                .zip(required_provenance_value(
                    admitter,
                    &values,
                    "processing",
                    binding,
                    binding_section.source,
                ))
                .map(
                    |(file, processing)| Provenance::InstrumentRun {
                        file,
                        processing,
                        sha256: values.get("sha256").cloned(),
                    },
                ),
                &["kind", "file", "processing", "sha256"],
            ),
            "fitted" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "fit_id",
                    binding,
                    binding_section.source,
                )
                .map(|fit_id| Provenance::Fitted { fit_id }),
                &["kind", "fit_id"],
            ),
            "assumed" => (
                Some(Provenance::Assumed {
                    reason: values.get("reason").cloned(),
                }),
                &["kind", "reason"],
            ),
            "unstated" => (Some(Provenance::Unstated), &["kind"]),
            _ => {
                admitter.error(
                    "E-SYN-152",
                    format!(
                        "unknown provenance kind `{kind}`; expected Exact, Citation, InstrumentRun, Fitted, Assumed, or Unstated"
                    ),
                    binding_section.source,
                );
                (None, &["kind"])
            }
        };
        for key in values.keys() {
            if !allowed.contains(&key.as_str()) {
                admitter.error(
                    "E-SYN-152",
                    format!("provenance kind `{kind}` does not admit key `{key}`"),
                    binding_section.source,
                );
            }
        }
        if let Some(provenance) = provenance {
            admitted.insert(binding.to_string(), provenance);
        }
    }
    admitted
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
                    name,
                    params,
                    ret,
                    suite,
                    ..
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
                            let ty = if param.by_ref { format!("&{ty}") } else { ty };
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

fn embedded_law_package(path: &[String]) -> Option<&'static str> {
    match path {
        [domain, pack] if domain == "physics" && pack == "classical" => Some(include_str!(
            "../../../../language/stdlib/laws/physics-classical.emath"
        )),
        [domain, pack] if domain == "physics" && pack == "relativity" => Some(include_str!(
            "../../../../language/stdlib/laws/physics-relativity.emath"
        )),
        [domain, pack] if domain == "cs" && pack == "laws" => Some(include_str!(
            "../../../../language/stdlib/laws/computer-science.emath"
        )),
        [domain, pack] if domain == "probability" && pack == "laws" => Some(include_str!(
            "../../../../language/stdlib/laws/probability-statistics.emath"
        )),
        [domain, pack] if domain == "analysis" && pack == "laws" => Some(include_str!(
            "../../../../language/stdlib/laws/analysis.emath"
        )),
        [domain, pack] if domain == "number_theory" && pack == "laws" => Some(include_str!(
            "../../../../language/stdlib/laws/algebra-number-theory.emath"
        )),
        [domain, pack] if domain == "optimization_control" && pack == "laws" => Some(include_str!(
            "../../../../language/stdlib/laws/optimization-control.emath"
        )),
        _ => None,
    }
}

fn resolve_embedded_law_import(
    imports: &[ImportEntry],
    source: emath_core::Span,
) -> Option<CheckResult> {
    let [import] = imports else {
        if imports.len() > 1
            && imports
                .iter()
                .all(|import| embedded_law_package(&import.path).is_some())
        {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error(
                "E-PKG-053",
                "import one embedded law package per source; multi-package imports are not admitted yet",
                source,
            );
            let mut package = emath_ir::SemanticPackage::new();
            package.imports = imports.to_vec();
            return Some(CheckResult {
                package,
                diagnostics,
                trace: SemanticTrace::default(),
                units_profiles: Vec::new(),
            });
        }
        return None;
    };
    let package_source = embedded_law_package(&import.path)?;
    let Some(parser) = emath_core::source_parser() else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-SYN-120",
            "no source parser installed for embedded law package",
            source,
        );
        return Some(CheckResult {
            package: emath_ir::SemanticPackage::new(),
            diagnostics,
            trace: SemanticTrace::default(),
            units_profiles: Vec::new(),
        });
    };
    let (tree, parse_diagnostics) = parser.parse(
        package_source,
        source.file,
        &emath_core::limits::Limits::default(),
        emath_core::Edition::Ed2026,
    );
    let mut result = check_tree(&tree);
    result.diagnostics.extend_from(&parse_diagnostics);

    if let ImportSelection::Named(names) = &import.selection {
        let selected: BTreeSet<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
        for (name, alias) in names {
            if alias.is_some() {
                result.diagnostics.error(
                    "E-PKG-053",
                    "aliases on embedded law imports are not admitted yet",
                    import.source,
                );
            }
            if !result
                .package
                .declarations
                .iter()
                .any(|declaration| declaration.name.leaf() == name)
            {
                result.diagnostics.error(
                    "E-PKG-053",
                    format!(
                        "law symbol `{name}` is not exported by `{}`",
                        import.path.join("::")
                    ),
                    import.source,
                );
            }
        }
        let retained: BTreeSet<emath_ir::DeclarationId> = result
            .package
            .declarations
            .iter()
            .filter(|declaration| selected.contains(declaration.name.leaf()))
            .map(|declaration| declaration.id)
            .collect();
        result
            .package
            .declarations
            .retain(|declaration| retained.contains(&declaration.id));
        result
            .package
            .law_metadata
            .retain(|declaration, _| retained.contains(declaration));
    }
    result.package.imports = imports.to_vec();
    if !result.diagnostics.has_errors() {
        result.package.seal();
    }
    Some(result)
}

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    // Item-attribute gates are file-scoped and run for every checked
    // tree, independent of the front-end lane: the Phase 1 compat lane
    // canonicalizes every kind to `custom`, so gating inside
    // `admit_front_end` (which runs only for package/use/notation
    // items) would silently skip ordinary files.
    crate::recognition::admit_capability_gates(tree, &mut diagnostics);
    let units_profiles: Vec<(String, String)> =
        crate::recognition::admit_units_profiles(tree, &mut diagnostics);

    let has_declaration = tree
        .items
        .iter()
        .any(|item| matches!(item, emath_core::tree::Item::Declaration(_)));

    // Front-end: package identity and `use` imports. External file
    // imports remain a Phase 2 refusal (E-PKG-050).
    let has_recognition_items = tree.items.iter().any(|item| match item {
        emath_core::tree::Item::Package { .. } | emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => {
            decl.item_kind != "custom"
                || decl.as_kind.is_empty()
                || decl.as_kind == "reaction_network"
                || decl
                    .body
                    .iter()
                    .any(|stmt| matches!(&stmt.kind, emath_core::tree::StmtKind::Section(section) if section.name == "world" || section.name == "artifact"))
        }
        emath_core::tree::Item::Notation(_) => true,
    });
    let recognition = if has_recognition_items {
        let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
        package.package_path = front_end.package_path;
        package.imports = front_end.imports;
        Some(crate::recognition::collect_kind_defs(tree))
    } else {
        None
    };
    if !has_declaration {
        if let Some(mut resolved) = resolve_embedded_law_import(&package.imports, tree.source) {
            resolved.diagnostics.extend_from(&diagnostics);
            resolved.trace.entries.extend(trace.entries);
            return resolved;
        }
        diagnostics.error("E-PKG-081", "source has no declarations", tree.source);
        return CheckResult {
            package,
            diagnostics,
            trace,
            units_profiles,
        };
    }
    let host_types = host_imported_types(&package.imports);

    // Sibling `emath function` declarations callable from lowering time
    // (emath-0e68): head-args or `inputs:`/`outputs:` section form. This
    // is function DATA for the generic declared-call seam's inline path —
    // no new AST node, no registry entry. A callee whose parameter types
    // do not map is not registered; its own admission reports the error.
    let mut sibling_functions: BTreeMap<String, SiblingFunction> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" {
            continue;
        }
        let mut params: Vec<(String, super::infer::Infer)> = Vec::new();
        let mut param_types_ok = true;
        // Metadata pass: type diagnostics were (or will be) reported by the
        // declaration's own admission — route map_type diagnostics into a
        // throwaway sink so a refused type site (bare `Real`, unknown name)
        // is reported exactly once, by the pass that owns it.
        let mut type_diagnostics = Diagnostics::new();
        let mut collect_param = |ty: &emath_core::tree::TypeExpr, name: &str| {
            // Untyped inputs are the Infer marker: admission defaults them
            // to Float64 (N-TYPE-001) without an error, so the sibling
            // signature must mirror that default instead of routing the
            // marker into `map_type` (which would emit a spurious
            // E-TYPE-001 "unknown type `Infer`" no other pass reports).
            if is_infer_marker(ty) {
                params.push((name.to_string(), super::infer::Infer::F64));
                return;
            }
            match map_type(ty, &mut type_diagnostics, &host_types) {
                Some(node) => params.push((name.to_string(), infer_from_node(&node))),
                None => param_types_ok = false,
            }
        };
        if let Some(signature) = &decl.signature {
            for param in &signature.params {
                collect_param(&param.ty, &param.name);
            }
        } else {
            for section in decl.body.iter().filter_map(|stmt| match &stmt.kind {
                emath_core::tree::StmtKind::Section(section) if section.name == "inputs" => {
                    Some(section)
                }
                _ => None,
            }) {
                for stmt in &section.suite.statements {
                    let emath_core::tree::StmtKind::FieldDecl { name, ty, .. } = &stmt.kind
                    else {
                        continue;
                    };
                    collect_param(ty, name);
                }
            }
        }
        if !param_types_ok {
            continue;
        }
        let output_name = decl
            .body
            .iter()
            .find_map(|stmt| match &stmt.kind {
                emath_core::tree::StmtKind::Section(section) if section.name == "outputs" => {
                    section.suite.statements.iter().find_map(|stmt| match &stmt.kind {
                        emath_core::tree::StmtKind::FieldDecl { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                }
                _ => None,
            })
            .unwrap_or_else(|| decl.name.clone());
        let definitions: Vec<emath_core::tree::Stmt> = if decl.signature.is_some() {
            decl.body
                .iter()
                .filter(|stmt| {
                    matches!(stmt.kind, emath_core::tree::StmtKind::Assign { .. })
                })
                .cloned()
                .collect()
        } else {
            decl.body
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    emath_core::tree::StmtKind::Section(section)
                        if section.name == "definitions" =>
                    {
                        Some(section.suite.statements.clone())
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        };
        // Alpha-rename the parameters inside the callee's own body to
        // `param#owner` (emath-0e68): `#` is not a valid identifier
        // character (lexer: alphanumeric, `_`, alphabetic, combining
        // marks), so a renamed parameter can never collide with a caller
        // variable and the inline substitution can never make a
        // definition self-referential. One rename per function at
        // collection time; call sites bind the renamed names.
        let rename_map: BTreeMap<String, String> = params
            .iter()
            .map(|(name, _)| {
                (
                    name.clone(),
                    super::lowering::sibling_calls::renamed_parameter(&decl.name, name),
                )
            })
            .collect();
        let definitions: Vec<emath_core::tree::Stmt> = definitions
            .into_iter()
            .map(|stmt| {
                let emath_core::tree::StmtKind::Assign { target, value } = &stmt.kind else {
                    return stmt;
                };
                emath_core::tree::Stmt {
                    kind: emath_core::tree::StmtKind::Assign {
                        target: target.clone(),
                        value: super::lowering::sibling_calls::rename_parameter_uses(
                            value,
                            &rename_map,
                            &mut Vec::new(),
                        ),
                    },
                    source: stmt.source,
                }
            })
            .collect();
        let params = params
            .into_iter()
            .map(|(name, infer)| {
                (
                    super::lowering::sibling_calls::renamed_parameter(&decl.name, &name),
                    infer,
                )
            })
            .collect();
        sibling_functions.insert(
            decl.name.clone(),
            SiblingFunction {
                params,
                output_name,
                definitions,
            },
        );
    }

    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    // Declared capability cells' output-type text, keyed by canonical
    // cell name, captured when a cell admits cleanly. This is the
    // cell's OWN contract data for the generic capability-call path —
    // never a guessed type.
    let mut capability_output_types: BTreeMap<String, Option<String>> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if let Some(kind_defs) = &recognition {
            let imported_custom_kind =
                decl.item_kind == "custom" && kind_defs.contains_key(&decl.as_kind);
            if decl.item_kind != "custom" || imported_custom_kind {
                // A capability cell's declared output type is call-site
                // data: capture it when the cell admits cleanly (a
                // malformed cell records nothing, mirroring the
                // capability arena's fail-closed rule).
                let cell_admission = (decl.as_kind == "capability").then(|| {
                    let canonical = match &package.package_path {
                        Some(path) if !path.is_empty() => {
                            format!("{}.{}", path.join("."), decl.name)
                        }
                        _ => decl.name.clone(),
                    };
                    let output = decl.body.iter().find_map(|stmt| match &stmt.kind {
                        emath_core::tree::StmtKind::Section(section)
                            if section.name == "outputs" =>
                        {
                            section.suite.statements.iter().find_map(|stmt| match &stmt.kind {
                                emath_core::tree::StmtKind::FieldDecl { ty, .. } => {
                                    Some(crate::recognition::type_text(ty))
                                }
                                _ => None,
                            })
                        }
                        _ => None,
                    });
                    (diagnostics.errors().count(), canonical, output)
                });
                crate::recognition::admit_declaration(
                    decl,
                    kind_defs,
                    &mut package,
                    &mut diagnostics,
                    &mut trace,
                );
                if let Some((errors_before, canonical, output)) = cell_admission {
                    if diagnostics.errors().count() == errors_before {
                        capability_output_types.insert(canonical, output);
                    }
                }
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
        // Parser remaps `emath kind Name:` to `item_kind=custom`,
        // `as_kind=kind`. CAPABILITY admits that form for partial schema
        // validation and does not lower it to a runnable declaration.
        if decl.as_kind == "kind" {
            let mut kind_decl = decl.clone();
            kind_decl.item_kind = "kind".to_string();
            crate::recognition::admit_declaration(
                &kind_decl,
                &BTreeMap::new(),
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
        }
        // Bare `emath custom Name:` with a `world constructor` body (spec
        // 09 / `emath-nko`) routes through recognition: bounded strategies,
        // protect, portfolio output; never lowered into strict meaning.
        if decl.as_kind.is_empty() && decl.body.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                emath_core::tree::StmtKind::Section(section)
                    if section.name == "world" || section.name == "artifact"
            )
        }) {
            crate::recognition::admit_custom_world(
                decl,
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
        }
        // `emath reaction_network Name:` (04 section 3.1): species closure
        // + static element balance, admitted at the recognition seam like
        // `custom world`; never lowered into strict meaning.
        if decl.as_kind == "reaction_network" {
            crate::recognition::admit_reaction_network(decl, &mut diagnostics);
            continue;
        }
        // `emath field_pack Name:` (v9-06-2rdq.16): pack exports are
        // artifact data admitted at the recognition seam — never lowered
        // into strict meaning, never a silent custom fallthrough.
        if decl.as_kind == "field_pack" {
            crate::recognition::admit_field_pack(
                decl,
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
        }
        if !matches!(
            decl.as_kind.as_str(),
            "function" | "policy" | "model" | "law"
        ) {
            let type_name = if decl.as_kind.is_empty() {
                "custom"
            } else {
                decl.as_kind.as_str()
            };
            // 02yn (custom-kind execution story): a kind DEFINITION
            // registered earlier in this source is a real kind; an
            // APPLICATION of it gets an EXPLICIT refusal story naming
            // the kind-execution follow-up — never a generic whitelist
            // error that looks like a typo (the registry validated the
            // definition; what is missing is the run path: kind-level
            // goal semantics or codegen for custom kinds).
            let kind_defined = decl
                .as_kind
                .is_empty()
                .then_some(())
                .is_none()
                && package.declarations.iter().any(|existing| {
                    existing.kind.0 == "kind" && existing.name.0 == decl.as_kind
                });
            if !kind_defined {
                diagnostics.error(
                    "E-KIND-100",
                    format!(
                        "declaration type `{type_name}` is outside the Phase 1 subset (function, policy, model, law)"
                    ),
                    decl.head_source,
                );
                continue;
            }
        }
        // The generic declared/mounted capability surface: every cell in
        // the package's capability arena is callable by name — the
        // canonical dotted form, plus the bare declaration name when it
        // is unambiguous across cells. A call resolving here lowers to
        // `ExprNode::Apply` (the emitter's ApplyCapability path); no
        // builtin name is added and unknown names still refuse typed.
        let mut bare_cell_counts: BTreeMap<String, usize> = BTreeMap::new();
        for capability in &package.capabilities {
            let bare = capability
                .name
                .0
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_string();
            *bare_cell_counts.entry(bare).or_insert(0) += 1;
        }
        let capability_cells: Vec<(String, u32, Option<String>)> = package
            .capabilities
            .iter()
            .enumerate()
            .flat_map(|(index, capability)| {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                let output = capability_output_types
                    .get(&capability.name.0)
                    .cloned()
                    .flatten();
                let bare = capability
                    .name
                    .0
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let mut keys = vec![(capability.name.0.clone(), index, output.clone())];
                if bare != capability.name.0 && bare_cell_counts.get(&bare).copied() == Some(1) {
                    keys.push((bare, index, output));
                }
                keys
            })
            .collect();
        let (
            declaration,
            mut tests,
            types,
            exprs,
            entries,
            admit_diagnostics,
            mut residuals,
            mut events,
            mut transitions,
            law_metadata,
            binding_provenance,
        ) = admit_declaration(decl, &host_types, &capability_cells, &sibling_functions);
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
        // Remap local ExprIds and TypeIds to the package's global index
        // space before merging the arenas. Without this, declaration 2's
        // ExprId(0) would alias declaration 1's first expression.
        let expr_offset = u32::try_from(package.exprs.len()).unwrap_or(u32::MAX);
        let type_offset = u32::try_from(package.types.len()).unwrap_or(u32::MAX);
        remap_ids(
            &mut declaration,
            &mut tests,
            &mut residuals,
            &mut events,
            &mut transitions,
            expr_offset,
            type_offset,
        );
        if !residuals.is_empty() {
            package.residuals.insert(declaration.id, residuals);
        }
        if !events.is_empty() {
            package.events.insert(declaration.id, events);
        }
        if !transitions.is_empty() {
            package.transitions.insert(declaration.id, transitions);
        }
        if let Some(metadata) = law_metadata {
            package.law_metadata.insert(declaration.id, metadata);
        }
        for (binding, provenance) in binding_provenance {
            package.binding_provenance.insert(
                emath_ir::BindingSite::new(declaration.id, binding),
                provenance,
            );
        }
        package.types.extend(types);
        for (e, _) in &exprs {
            let mut node = e.clone();
            remap_expr_node(&mut node, expr_offset, type_offset);
            package.exprs.push(node);
        }
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
        units_profiles,
    }
}
