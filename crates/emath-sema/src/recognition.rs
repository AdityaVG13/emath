//! Recognition-level admission (corpus front-end).
//!
//! Package identity, `use` imports, declaration-kind admission, the
//! custom-kind schema registry, and per-kind structural validation.
//! Out-of-subset constructs get a typed refusal; body expressions are
//! validated structurally (typed SIR lowering is the intent-compiler lane).

use crate::admit::SemanticTrace;
use emath_core::Diagnostics;
use emath_core::tree::{
    ArgumentValue, Attribute, BinaryOp, Binder, BinderKind, CommandArgument, Declaration, Expr,
    ExprKind, Item, ReactionArrow, Section, Stmt, StmtKind, TypeKind, UnaryOp, UseTree,
};
use emath_ir::{ImportEntry, ImportSelection};
use std::collections::{BTreeMap, BTreeSet};

/// Declaration kinds admitted by this front-end.
pub const RECOGNIZED_KINDS: &[&str] = &[
    "function",
    "record",
    "policy",
    "model",
    "kind",
    "search",
    "experiment",
    "type",
];

mod schema;
mod text;

pub use schema::{KindDef, SchemaRule};
pub use text::{expr_text, type_text};

use schema::*;
use text::*;

// ---- admission ------------------------------------------------------------

/// Outcome of the file front-end.
#[derive(Clone, Debug, Default)]
pub struct V6FrontEnd {
    /// `package <dotted>` identity, if declared.
    pub package_path: Option<Vec<String>>,
    /// Admitted imports in source order.
    pub imports: Vec<ImportEntry>,
}

/// Collect the `emath kind` definitions declared in the file.
#[must_use]
pub fn collect_kind_defs(tree: &emath_core::tree::SyntaxTree) -> BTreeMap<String, KindDef> {
    let mut defs = BTreeMap::new();
    for item in &tree.items {
        if let Item::Use { path, .. } = item {
            if let Some(def) = imported_kind(path) {
                defs.entry(def.name.clone()).or_insert(def);
            }
            continue;
        }
        let Item::Declaration(decl) = item else {
            continue;
        };
        // Parser remaps `emath kind Name:` to `item_kind=custom` with
        // the original spelling in `as_kind`; hand-built trees keep
        // `item_kind == "kind"`.
        let is_kind_def =
            decl.item_kind == "kind" || (decl.item_kind == "custom" && decl.as_kind == "kind");
        if !is_kind_def {
            continue;
        }
        let mut def = KindDef {
            name: decl.name.clone(),
            extends: None,
            schema: Vec::new(),
        };
        for stmt in &decl.body {
            match &stmt.kind {
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("extends") =>
                {
                    def.extends = head.get(1).cloned();
                }
                StmtKind::Section(section) if section.name == "schema" => {
                    def.schema.extend(schema_rules_from_section(section));
                }
                _ => {}
            }
        }
        defs.insert(decl.name.clone(), def);
    }
    defs
}

fn schema_rules_from_section(section: &emath_core::tree::Section) -> Vec<SchemaRule> {
    let mut rules = Vec::new();
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Require(expr) => {
                if let Some(head) = require_head(expr) {
                    rules.push(head);
                }
            }
            // `allow section <name>` → [.., "section", name].
            StmtKind::Command { head, .. }
                if head.first().map(String::as_str) == Some("allow")
                    && head.get(1).map(String::as_str) == Some("section") =>
            {
                if let Some(name) = head.get(2) {
                    rules.push(SchemaRule::AllowSection(name.clone()));
                }
            }
            _ => {}
        }
    }
    rules
}

/// Interpret a `require <expr>` schema statement as a rule.
/// The parser folds `section input` / `exactly_one output` into a plain
/// path expression (`["section", "input"]`).
fn require_head(expr: &Expr) -> Option<SchemaRule> {
    let ExprKind::Path { segments, .. } = &expr.kind else {
        return None;
    };
    match segments.first().map(String::as_str) {
        Some("section") => segments.get(1).cloned().map(SchemaRule::RequireSection),
        Some("exactly_one") => segments
            .get(1)
            .cloned()
            .map(SchemaRule::RequireExactlyOneSection),
        _ => None,
    }
}

/// Admit the file-level front-end items (`package`, `use`).
pub fn admit_front_end(
    tree: &emath_core::tree::SyntaxTree,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) -> V6FrontEnd {
    let mut result = V6FrontEnd::default();
    for item in &tree.items {
        match item {
            Item::Package { path, source } => {
                result.package_path = Some(path.clone());
                trace.record("recognize:package", path.join("."), Some(*source));
            }
            Item::Use { path, tree, source } => {
                if is_external_import(path) {
                    diagnostics.error(
                        "E-PKG-050",
                        format!(
                            "external file import `{}` is outside the front-end subset (library-path imports only)",
                            path.join(".")
                        ),
                        *source,
                    );
                    continue;
                }
                if is_unresolved_law_import(path, tree) {
                    diagnostics.error(
                        "E-PKG-052",
                        format!(
                            "law package import `{}` is not resolved yet; declare the law locally until the curated package registry lands",
                            path.join("::")
                        ),
                        *source,
                    );
                    continue;
                }
                let mut path = path.clone();
                let selection = match tree {
                    UseTree::All => ImportSelection::All,
                    UseTree::Named(names) if names.is_empty() && path.len() >= 2 => {
                        // `use std.numeric.Real`: the parser keeps the
                        // single imported name in the path.
                        let name = path.pop().unwrap_or_default();
                        ImportSelection::Named(vec![(name, None)])
                    }
                    UseTree::Named(names) => ImportSelection::Named(
                        names.iter().map(|(n, a)| (n.clone(), a.clone())).collect(),
                    ),
                };
                trace.record(
                    "recognize:import",
                    format!("{} {selection:?}", path.join("::")),
                    Some(*source),
                );
                result.imports.push(ImportEntry {
                    path,
                    selection,
                    source: *source,
                });
            }
            Item::Declaration(_) => {}
            Item::Notation(_) => {}
        }
    }
    result
}

// ---- item attributes and the experimental lane ----------------------------

/// `@experimental` items require the source file to declare the
/// `experimental-syntax` capability (ELP experimental lane; see
/// `elps/README.md`).
const E_EXPERIMENTAL_CAPABILITY: &str = "E-PKG-064";
/// Unknown capability key in `@capabilities(...)` (nothing is dropped).
const E_UNKNOWN_CAPABILITY: &str = "E-PKG-065";
/// Constitution: the `experimental` attribute takes no arguments.
const E_ATTRIBUTE_ARG: &str = "E-SYN-117";
/// An attribute the front-end does not understand is refused, never
/// silently ignored.
const E_UNKNOWN_ATTRIBUTE: &str = "E-SYN-118";

/// Post-pass over item attributes (ELP governance, file scope).
///
/// Capability keys: `experimental-syntax` is the only declared key today.
/// `@experimental` on any item requires the file to declare it via
/// `@capabilities(experimental-syntax)` on any item; unknown attributes
/// and unknown capability keys are typed refusals so no syntax is ever
/// silently dropped.
pub fn admit_capability_gates(tree: &emath_core::tree::SyntaxTree, diagnostics: &mut Diagnostics) {
    let mut capabilities: BTreeSet<String> = BTreeSet::new();
    let mut experimental: Vec<(&str, emath_core::Span)> = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        for attribute in &decl.attributes {
            if attribute.name == "capabilities" {
                for arg in &attribute.args {
                    let key = unquote_attribute_arg(arg);
                    if key == "experimental-syntax" {
                        capabilities.insert(key);
                    } else {
                        diagnostics.error(
                            E_UNKNOWN_CAPABILITY,
                            format!("unknown capability `{key}` in `@capabilities` (declared: experimental-syntax)"),
                            attribute.source,
                        );
                    }
                }
            } else if attribute.name == "experimental" {
                if !attribute.args.is_empty() {
                    diagnostics.error(
                        E_ATTRIBUTE_ARG,
                        "the `experimental` attribute takes no arguments",
                        attribute.source,
                    );
                }
                experimental.push((&decl.name, decl.source));
            } else if attribute.name == "significant_figures" {
                admit_sig_figures(decl, attribute, diagnostics);
            } else if attribute.name == "units_profile" {
                // Admitted by `admit_units_profiles` (04 §6.1); listed
                // here so the unknown-attribute refusal does not fire
                // before the profile pass runs.
            } else {
                diagnostics.error(
                    E_UNKNOWN_ATTRIBUTE,
                    format!("unknown attribute `@{}`", attribute.name),
                    attribute.source,
                );
            }
        }
    }
    if !capabilities.contains("experimental-syntax") {
        for (name, source) in experimental {
            diagnostics.error(
                E_EXPERIMENTAL_CAPABILITY,
                format!(
                    "`@experimental` on `{name}` requires the `experimental-syntax` capability; \
                     declare `@capabilities(experimental-syntax)` on an item in this file"
                ),
                source,
            );
        }
    }
}

// ---- units profiles (emath-r3-honesty-syntax-x83o, 04 §6.1) ----------------

/// A bare numeric quantity (numeric value + unit, no uncertainty wrapper)
/// under a profile that demands declared uncertainty.
const E_PROFILE_BARE_QUANTITY: &str = "E-UNIT-106";
/// The publication profile requires the declaration's honesty header.
const E_PROFILE_MISSING_PROVENANCE: &str = "E-UNIT-107";

/// The four units-profile levels (04 §6.1), weakest to strongest.
const UNITS_PROFILE_LEVELS: &[&str] = &["permissive", "lab", "engineering", "publication"];

/// `@units_profile(level)` admission (04 §6.1): validate the level,
/// refuse a second profile on the same declaration, and enforce the
/// ladder. A profile can strengthen a core check, never weaken one
/// (ch. 12 negative-space rule 5); `permissive` — and the absence of the
/// attribute — keeps today's behavior byte-for-byte. Returns the
/// effective per-declaration table in source order (the §6.5 pack-table).
pub fn admit_units_profiles(
    tree: &emath_core::tree::SyntaxTree,
    diagnostics: &mut Diagnostics,
) -> Vec<(String, String)> {
    let mut table = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        let mut level: Option<String> = None;
        for attribute in &decl.attributes {
            if attribute.name != "units_profile" {
                continue;
            }
            if attribute.args.len() != 1 {
                diagnostics.error(
                    "E-SYN-117",
                    "the `units_profile` attribute takes exactly one of `permissive`, `lab`, \
                     `engineering`, `publication`"
                        .to_string(),
                    attribute.source,
                );
                continue;
            }
            let requested = unquote_attribute_arg(&attribute.args[0]);
            if !UNITS_PROFILE_LEVELS.contains(&requested.as_str()) {
                diagnostics.error(
                    "E-SYN-117",
                    format!(
                        "unknown units_profile level `{requested}` (one of `permissive`, `lab`, \
                         `engineering`, `publication`)"
                    ),
                    attribute.source,
                );
                continue;
            }
            if level.replace(requested).is_some() {
                diagnostics.error(
                    "E-SYN-117",
                    format!(
                        "`{}` declares one units_profile; a second would silently override \
                         the first",
                        decl.name
                    ),
                    attribute.source,
                );
                continue;
            }
        }
        let Some(level) = level else { continue };
        if matches!(level.as_str(), "engineering" | "publication") {
            let mut found: Vec<emath_core::Span> = Vec::new();
            for stmt in &decl.body {
                collect_bare_quantities(stmt, &mut found);
            }
            for span in found {
                diagnostics.error(
                    E_PROFILE_BARE_QUANTITY,
                    format!(
                        "bare numeric quantity under the `{level}` units profile: uncertainty \
                         must be declared for physical quantities (a Measured/uncertainty \
                         spelling, not a bare literal); lower the profile to `lab` if this \
                         value is exact by construction"
                    ),
                    span,
                );
            }
        }
        if level == "publication"
            && !decl
                .sections_vec()
                .iter()
                .any(|section| section.name == "provenance")
        {
            diagnostics.error(
                E_PROFILE_MISSING_PROVENANCE,
                format!(
                    "`{}` declares the publication units profile: a `provenance:` section is \
                     required (the declaration header is its honesty declaration)",
                    decl.name
                ),
                decl.source,
            );
        }
        table.push((decl.name.clone(), level.clone()));
    }
    table
}

/// Collect spans of bare numeric quantities (numeric literal under a
/// unit, no uncertainty wrapper) from one statement tree.
fn collect_bare_quantities(stmt: &Stmt, found: &mut Vec<emath_core::Span>) {
    match &stmt.kind {
        StmtKind::Section(section) => {
            if let Some(args) = &section.args {
                for argument in args {
                    if let ArgumentValue::Expr(expr) = &argument.value {
                        collect_bare_quantities_expr(expr, found);
                    }
                }
            }
            for nested in &section.suite.statements {
                collect_bare_quantities(nested, found);
            }
        }
        StmtKind::FieldDecl { default, .. } => {
            if let Some(value) = default {
                collect_bare_quantities_expr(value, found);
            }
        }
        StmtKind::FnDecl {
            params, suite, ..
        } => {
            for param in params {
                if let Some(default) = &param.default {
                    collect_bare_quantities_expr(default, found);
                }
            }
            if let Some(suite) = suite {
                for nested in &suite.statements {
                    collect_bare_quantities(nested, found);
                }
            }
        }
        StmtKind::OperatorDecl { .. } => {}
        StmtKind::Let { value, .. } | StmtKind::Given { value, .. } => {
            collect_bare_quantities_expr(value, found);
        }
        StmtKind::Assign { value, .. } => collect_bare_quantities_expr(value, found),
        StmtKind::Require(expr) | StmtKind::Ensure(expr) | StmtKind::Invariant(expr) => {
            collect_bare_quantities_expr(expr, found);
        }
        _ => {}
    }
}

fn collect_bare_quantities_expr(expr: &Expr, found: &mut Vec<emath_core::Span>) {
    match &expr.kind {
        ExprKind::Quantity { value, .. } => {
            if matches!(value.kind, ExprKind::Int(_) | ExprKind::Float(_)) {
                found.push(expr.source);
            } else {
                collect_bare_quantities_expr(value, found);
            }
        }
        ExprKind::Measured { .. } => {}
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Rational { .. } | ExprKind::Bool(_)
        | ExprKind::Str(_) | ExprKind::Path { .. } => {}
        ExprKind::Call { function, args } => {
            collect_bare_quantities_expr(function, found);
            for arg in args {
                collect_bare_quantities_expr(arg, found);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_bare_quantities_expr(value, found);
            for index in indices {
                collect_bare_quantities_expr(index, found);
            }
        }
        ExprKind::Slice { start, end } => {
            if let Some(start) = start {
                collect_bare_quantities_expr(start, found);
            }
            if let Some(end) = end {
                collect_bare_quantities_expr(end, found);
            }
        }
        ExprKind::Unary { value, .. } => collect_bare_quantities_expr(value, found),
        ExprKind::Binary { left, right, .. } => {
            collect_bare_quantities_expr(left, found);
            collect_bare_quantities_expr(right, found);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_bare_quantities_expr(condition, found);
            collect_bare_quantities_expr(then_value, found);
            collect_bare_quantities_expr(else_value, found);
        }
        _ => {}
    }
}


// ---- significant figures (bead emath-r3-sigfigs-formatting-yf28, 04 §1.6) --

/// `@significant_figures` admission. Modes: `display [<sf>]` records the
/// declaration's literal precision; `enforce <sf>` turns under-reported
/// literals into warning receipts. Warnings are receipts, never refusals:
/// sig-figs are a display contract, not uncertainty propagation. Mixing
/// `Measured` (uncertainty) values with bare sf-values in one declaration
/// is likewise a warning receipt, never silence and never a refusal.
fn admit_sig_figures(decl: &Declaration, attribute: &Attribute, diagnostics: &mut Diagnostics) {
    let mut malformed = || {
        diagnostics.error(
            E_ATTRIBUTE_ARG,
            "the `significant_figures` attribute takes `display`, `display <sf>`, or `enforce <sf>`",
            attribute.source,
        );
    };
    let args: Vec<String> = attribute
        .args
        .iter()
        .map(|arg| unquote_attribute_arg(arg))
        .collect();
    let sf_of = |text: &str| -> Option<u32> { text.parse::<u32>().ok().filter(|count| *count > 0) };
    let enforce_count: Option<u32> = match args.first().map(String::as_str) {
        Some("display") => match args.len() {
            1 => None,
            2 => match sf_of(&args[1]) {
                Some(_) => None,
                None => {
                    malformed();
                    return;
                }
            },
            _ => {
                malformed();
                return;
            }
        },
        Some("enforce") => match (args.len(), sf_of(&args.get(1).map(String::as_str).unwrap_or(""))) {
            (2, Some(count)) => Some(count),
            _ => {
                malformed();
                return;
            }
        },
        _ => {
            malformed();
            return;
        }
    };

    let mut literals: Vec<(String, emath_core::Span)> = Vec::new();
    let mut ledger = emath_core::sigfigs::PrecisionLedger::default();
    for stmt in &decl.body {
        collect_stmt_precision(stmt, &mut literals, &mut ledger);
    }
    if let Some(declared) = enforce_count {
        for (text, span) in &literals {
            if let Some(literal_sf) = emath_core::sigfigs::count_sig_figs(text) {
                if literal_sf < declared {
                    diagnostics.warning(
                        emath_core::sigfigs::E_SF_UNDER_REPORT,
                        format!(
                            "literal `{text}` carries {literal_sf} significant figure(s), fewer \
                             than the declared {declared} (sig-figs are a display contract; \
                             widen the literal or lower the declared count)"
                        ),
                        *span,
                    );
                }
            }
        }
    }
    if let Some(emath_core::sigfigs::PrecisionWarning::MixedMeasuredBareSf { measured, bare_sf }) =
        ledger.mix_warning()
    {
        diagnostics.warning(
            emath_core::sigfigs::E_SF_MIXED_KINDS,
            format!(
                "{measured} Measured (uncertainty) value(s) mixed with {bare_sf} bare sf-value(s) \
                 in one precision context; sig-figs and uncertainty are different evidence kinds \
                 and must be labeled separately"
            ),
            attribute.source,
        );
    }
}

/// Collect decimal literals and the Measured/bare-sf ledger from one
/// expression, recursing through every subexpression.
fn collect_expr_precision(
    expr: &Expr,
    literals: &mut Vec<(String, emath_core::Span)>,
    ledger: &mut emath_core::sigfigs::PrecisionLedger,
) {
    match &expr.kind {
        ExprKind::Int(value) | ExprKind::Float(value) => {
            literals.push((value.clone(), expr.source));
            ledger.record_bare_sf();
        }
        ExprKind::Measured { value, .. } => {
            literals.push((value.clone(), expr.source));
            ledger.record_measured();
        }
        ExprKind::Quantity { value, .. } => {
            collect_expr_precision(value, literals, ledger);
        }
        ExprKind::Rational { .. } | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Path { .. } => {
        }
        ExprKind::Call { function, args } => {
            collect_expr_precision(function, literals, ledger);
            for arg in args {
                collect_expr_precision(arg, literals, ledger);
            }
        }
        ExprKind::Index { value, indices } => {
            collect_expr_precision(value, literals, ledger);
            for index in indices {
                collect_expr_precision(index, literals, ledger);
            }
        }
        ExprKind::Slice { start, end } => {
            if let Some(start) = start {
                collect_expr_precision(start, literals, ledger);
            }
            if let Some(end) = end {
                collect_expr_precision(end, literals, ledger);
            }
        }
        ExprKind::Unary { value, .. } => collect_expr_precision(value, literals, ledger),
        ExprKind::Binary { left, right, .. } => {
            collect_expr_precision(left, literals, ledger);
            collect_expr_precision(right, literals, ledger);
        }
        ExprKind::Approx {
            left,
            right,
            tolerance,
        } => {
            collect_expr_precision(left, literals, ledger);
            collect_expr_precision(right, literals, ledger);
            if let Some(tolerance) = tolerance {
                if let Some(rtol) = &tolerance.rtol {
                    collect_expr_precision(rtol, literals, ledger);
                }
                if let Some(atol) = &tolerance.atol {
                    collect_expr_precision(atol, literals, ledger);
                }
            }
        }
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
            for item in items {
                collect_expr_precision(item, literals, ledger);
            }
        }
        // U9: table cells are decimal-bearing positions; the precision
        // ledger sees them like any other literal.
        ExprKind::Table { rows, .. } => {
            for cell in rows.iter().flatten() {
                collect_expr_precision(cell, literals, ledger);
            }
        }
        ExprKind::SetComprehension {
            element,
            domain,
            guard,
            ..
        } => {
            collect_expr_precision(element, literals, ledger);
            collect_expr_precision(domain, literals, ledger);
            if let Some(guard) = guard {
                collect_expr_precision(guard, literals, ledger);
            }
        }
        ExprKind::Record { fields, .. } => {
            for (_, value) in fields {
                collect_expr_precision(value, literals, ledger);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                collect_expr_precision(start, literals, ledger);
            }
            if let Some(end) = end {
                collect_expr_precision(end, literals, ledger);
            }
        }
        ExprKind::Derivative {
            value,
            wrt,
            holding,
            ..
        } => {
            collect_expr_precision(value, literals, ledger);
            if let Some(wrt) = wrt {
                for expr in wrt {
                    collect_expr_precision(expr, literals, ledger);
                }
            }
            for expr in holding {
                collect_expr_precision(expr, literals, ledger);
            }
        }
        ExprKind::Solve { value, wrt } | ExprKind::Optimize { value, wrt, .. } => {
            collect_expr_precision(value, literals, ledger);
            if let Some(wrt) = wrt {
                for expr in wrt {
                    collect_expr_precision(expr, literals, ledger);
                }
            }
        }
        ExprKind::At { value, location } | ExprKind::On { value, location } => {
            collect_expr_precision(value, literals, ledger);
            collect_expr_precision(location, literals, ledger);
        }
        ExprKind::Conditioned { value, condition } => {
            collect_expr_precision(value, literals, ledger);
            collect_expr_precision(condition, literals, ledger);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_expr_precision(condition, literals, ledger);
            collect_expr_precision(then_value, literals, ledger);
            collect_expr_precision(else_value, literals, ledger);
        }
        ExprKind::Binder {
            binders,
            body,
            guard,
            ..
        } => {
            for binder in binders {
                if let Some(domain) = &binder.domain {
                    collect_expr_precision(domain, literals, ledger);
                }
            }
            collect_expr_precision(body, literals, ledger);
            if let Some(guard) = guard {
                collect_expr_precision(guard, literals, ledger);
            }
        }
        ExprKind::UnitQuery { expr, .. } => collect_expr_precision(expr, literals, ledger),
        ExprKind::Limit { target, body, .. } | ExprKind::SampleLimit { target, body, .. } => {
            collect_expr_precision(target, literals, ledger);
            collect_expr_precision(body, literals, ledger);
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            if let Some(subject) = subject {
                collect_expr_precision(subject, literals, ledger);
            }
            for (condition, value) in arms {
                collect_expr_precision(condition, literals, ledger);
                collect_expr_precision(value, literals, ledger);
            }
            collect_expr_precision(else_arm, literals, ledger);
        }
        // U9: table cells are decimal-bearing positions; the precision
        // ledger sees them like any other literal. (Single arm: the
        // earlier Table arm was removed as an unreachable duplicate.)
        // 04 §5.4 slice 1: series pair elements are data literals, not
        // precision-bearing computation inputs; the pairs are recorded
        // as SI scalars at lowering, so nothing to ledger here.
        ExprKind::WithSeriesPolicy { .. } => {}
    }
}

/// Collect precision evidence from one statement, recursing through
/// nested suites and every expression-bearing position.
fn collect_stmt_precision(
    stmt: &Stmt,
    literals: &mut Vec<(String, emath_core::Span)>,
    ledger: &mut emath_core::sigfigs::PrecisionLedger,
) {
    match &stmt.kind {
        StmtKind::Section(section) => {
            if let Some(args) = &section.args {
                for argument in args {
                    if let ArgumentValue::Expr(expr) = &argument.value {
                        collect_expr_precision(expr, literals, ledger);
                    }
                }
            }
            for nested in &section.suite.statements {
                collect_stmt_precision(nested, literals, ledger);
            }
        }
        StmtKind::FieldDecl { default, .. } => {
            if let Some(value) = default {
                collect_expr_precision(value, literals, ledger);
            }
        }
        StmtKind::FnDecl {
            params, suite, ..
        } => {
            for param in params {
                if let Some(default) = &param.default {
                    collect_expr_precision(default, literals, ledger);
                }
            }
            if let Some(suite) = suite {
                for nested in &suite.statements {
                    collect_stmt_precision(nested, literals, ledger);
                }
            }
        }
        StmtKind::OperatorDecl { .. } => {}
        StmtKind::Let { value, .. } | StmtKind::Given { value, .. } => {
            collect_expr_precision(value, literals, ledger);
        }
        StmtKind::Assign { value, .. } => collect_expr_precision(value, literals, ledger),
        StmtKind::Require(expr) | StmtKind::Ensure(expr) | StmtKind::Invariant(expr) => {
            collect_expr_precision(expr, literals, ledger);
        }
        StmtKind::Expect(expr) | StmtKind::Expr(expr) => {
            collect_expr_precision(expr, literals, ledger);
        }
        StmtKind::If {
            condition,
            then,
            else_branches,
            else_tail,
        } => {
            collect_expr_precision(condition, literals, ledger);
            for nested in &then.statements {
                collect_stmt_precision(nested, literals, ledger);
            }
            for (branch_condition, branch) in else_branches {
                collect_expr_precision(branch_condition, literals, ledger);
                for nested in &branch.statements {
                    collect_stmt_precision(nested, literals, ledger);
                }
            }
            if let Some(else_tail) = else_tail {
                for nested in &else_tail.statements {
                    collect_stmt_precision(nested, literals, ledger);
                }
            }
        }
        StmtKind::BinderStmt {
            binders,
            suite,
            guard,
            ..
        } => {
            for binder in binders {
                if let Some(domain) = &binder.domain {
                    collect_expr_precision(domain, literals, ledger);
                }
            }
            for nested in &suite.statements {
                collect_stmt_precision(nested, literals, ledger);
            }
            if let Some(guard) = guard {
                collect_expr_precision(guard, literals, ledger);
            }
        }
        StmtKind::SelfBlock { assignments } => {
            for (_, value) in assignments {
                collect_expr_precision(value, literals, ledger);
            }
        }
        StmtKind::Command { argument, .. } => {
            if let Some(argument) = argument {
                match argument {
                    CommandArgument::Expr(expr) => {
                        collect_expr_precision(expr, literals, ledger);
                    }
                    CommandArgument::Assignment { value, .. } => {
                        collect_expr_precision(value, literals, ledger);
                    }
                    CommandArgument::List(items) => {
                        for item in items {
                            collect_expr_precision(item, literals, ledger);
                        }
                    }
                }
            }
        }
        StmtKind::Equation { left, right } => {
            collect_expr_precision(left, literals, ledger);
            collect_expr_precision(right, literals, ledger);
        }
        // Reaction lines carry no decimal literals (coefficients are u64
        // counts, species are names); nothing to feed the precision ledger.
        StmtKind::Reaction { .. } => {}
    }
}

/// Attribute args are stored as canonical source text; string literals
/// carry their quotes. Return the bare value.
fn unquote_attribute_arg(arg: &str) -> String {
    if arg.len() >= 2 && arg.starts_with('"') && arg.ends_with('"') {
        arg[1..arg.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        arg.to_string()
    }
}

/// A command argument of the form `key: "quoted string"` yields the
/// quoted string's text; anything else (bare word, arithmetic, absent)
/// is not a quoted-string argument.
fn quoted_string_argument(argument: Option<&CommandArgument>) -> Option<&str> {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Str(text) => Some(text),
            _ => None,
        },
        _ => None,
    }
}

/// A `use` whose path references a file rather than a library path remains
/// a Phase 2 external import.
fn is_external_import(path: &[String]) -> bool {
    let Some(first) = path.first() else {
        return true;
    };
    first.starts_with("./")
        || first.starts_with("../")
        || first.starts_with('/')
        || path
            .iter()
            .any(|segment| segment.to_ascii_lowercase().ends_with(".emath"))
}

fn is_unresolved_law_import(path: &[String], tree: &UseTree) -> bool {
    path == ["physics", "NewtonSecond"]
        || (path == ["physics"]
            && matches!(
                tree,
                UseTree::Named(names)
                    if names.iter().any(|(name, _)| name == "NewtonSecond")
            ))
}

/// Admit one declaration into the package and trace.
pub fn admit_declaration(
    decl: &Declaration,
    kind_defs: &BTreeMap<String, KindDef>,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let item_kind = decl.item_kind.as_str();
    if item_kind == "extern" {
        admit_extern(decl, package, diagnostics, trace);
        return;
    }
    if item_kind == "custom" && decl.as_kind.is_empty() {
        // Bare `emath custom Name:` (spec 09 / `emath-nko`): a custom
        // world constructor declaration. Body sections `world` /
        // `artifact` carry the constructor level; the declaration stays
        // evidence-neutral (E1/not-run, no checker) and is never lowered
        // into strict meaning here.
        admit_custom_world(decl, package, diagnostics, trace);
        return;
    }
    let schema_kind = if item_kind == "custom" {
        decl.as_kind.as_str()
    } else {
        item_kind
    };
    if let Some(def) = kind_defs.get(schema_kind) {
        admit_kind_application(decl, def, package, diagnostics, trace);
        return;
    }
    let Some(rules) = section_rules(item_kind) else {
        diagnostics.error(
            "E-KIND-001",
            format!("declaration kind `{item_kind}` is not supported by this front-end"),
            decl.head_source,
        );
        return;
    };
    if item_kind == "kind" {
        // The definition itself is a registry entry. A malformed schema must
        // not leave a registered marker that later applications can observe.
        let errors_before = diagnostics.errors().count();
        validate_body(decl, &rules, diagnostics, trace);
        if diagnostics.errors().count() > errors_before {
            return;
        }
        // 02yn (custom-kind execution story): record the registered kind
        // as a marker declaration so a later APPLICATION of the kind gets
        // the explicit no-run-path refusal (E-KIND-100 naming the
        // kind-execution follow-up) instead of a generic whitelist error.
        let mut marker = package_entry(decl, "kind");
        marker.id =
            emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
        package.declarations.push(marker);
        trace.record(
            "recognize:kind-registered",
            format!(
                "custom kind `{}` schema validated and registered",
                decl.name
            ),
            Some(decl.head_source),
        );
        return;
    }
    let mut declaration = package_entry(decl, item_kind);
    validate_body(decl, &rules, diagnostics, trace);
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:admit",
        format!("declaration `{}` kind `{item_kind}` admitted", decl.name),
        Some(decl.head_source),
    );
}

/// `emath field_pack Name:` admission (v9-06-2rdq.16; the parser
/// canonicalizes the spelled kind to `custom`/`as_kind`, so the compat
/// lane routes here). A pack is exported ARTIFACT data: validate the
/// closed section table, record the entry on the package, and never
/// lower the pack into runnable meaning — no declaration entry, no
/// silent custom→strict fallthrough, and no parser surface (unknown
/// sections refuse through the closed table).
pub(crate) fn admit_field_pack(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Ok(rules) = section_rules("field_pack").ok_or_else(|| "field_pack") else {
        diagnostics.error(
            "E-KIND-001",
            "field_pack section rules are missing (internal registration error)",
            decl.head_source,
        );
        return;
    };
    // Fail-closed: a pack whose body validation produced errors (e.g. a
    // keyword-injection section) yields NO installable data — the
    // FieldPackEntry is recorded only for a clean admission.
    let errors_before = diagnostics.errors().count();
    validate_body(decl, &rules, diagnostics, trace);
    if diagnostics.errors().count() > errors_before {
        return;
    }
    let mut exports: Vec<(String, String)> = Vec::new();
    for statement in &decl.body {
        let StmtKind::Section(section) = &statement.kind else {
            continue;
        };
        if section.name != "exports" {
            continue;
        }
        for nested in &section.suite.statements {
            if let StmtKind::Command { head, .. } = &nested.kind {
                if let [export_kind, name] = head.as_slice() {
                    exports.push((export_kind.clone(), name.clone()));
                }
            }
        }
    }
    let export_count = exports.len();
    package.field_packs.push(emath_ir::FieldPackEntry {
        name: decl.name.clone(),
        exports,
    });
    trace.record(
        "recognize:field_pack",
        format!("pack `{}` admitted with {} export(s)", decl.name, export_count),
        Some(decl.head_source),
    );
}

/// Custom world constructor declaration (spec 09 / `emath-nko`).
///
/// Admits `world constructor <name>:` sections (bounded strategies,
/// protect, portfolio output) with an E1/not-run claim; `artifact
/// constructor <name>:` is outside the Phase 1 subset and refuses rather
/// than silently accepting unimplemented lowering. An `authority:` body
/// section may not raise evidence authority.
pub(crate) fn admit_custom_world(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let mut valid = true;
    let mut has_world_ctor = false;
    for statement in &decl.body {
        let StmtKind::Section(section) = &statement.kind else {
            diagnostics.error(
                "E-KIND-027",
                "a custom world declaration carries `world constructor ...:` body sections",
                statement.source,
            );
            valid = false;
            continue;
        };
        match section.name.as_str() {
            "world" => {
                has_world_ctor = true;
                for nested in &section.suite.statements {
                    match &nested.kind {
                        StmtKind::Section(nested) => {
                            match nested.name.as_str() {
                                "strategies" | "protect" | "output" => {}
                                other => {
                                    diagnostics.error(
                                        "E-KIND-027",
                                        format!("unknown `world constructor` clause `{other}`"),
                                        nested.head_source,
                                    );
                                    valid = false;
                                }
                            }
                        }
                        // strategy names are bare idents inside `strategies:`
                        StmtKind::Expr(_) | StmtKind::Command { .. } => {}
                        _ => {
                            diagnostics.error(
                                "E-KIND-027",
                                "`world constructor` clauses are sections (`strategies:`, `protect:`, `output:`)",
                                nested.source,
                            );
                            valid = false;
                        }
                    }
                }
            }
            "artifact" => {
                diagnostics.error(
                    "E-KIND-001",
                    "artifact constructors are outside the Phase 1 subset (packaging lands with the field-pack tooling)",
                    section.head_source,
                );
                valid = false;
            }
            "authority" => {
                diagnostics.error(
                    "E-KIND-027",
                    "custom world declarations cannot mint evidence authority by declaration alone",
                    section.head_source,
                );
                valid = false;
            }
            other => {
                diagnostics.error(
                    "E-KIND-027",
                    format!("unknown custom world section `{other}`"),
                    section.head_source,
                );
                valid = false;
            }
        }
    }
    if !has_world_ctor {
        return;
    }
    if !valid {
        return;
    }

    let mut declaration = package_entry(decl, "custom");
    declaration.about =
        Some("Custom world constructor; expansion is deterministic and evidence-neutral".into());
    declaration.evidence.push(emath_ir::EvidenceClaim {
        id: "custom-world-constructor".into(),
        statement: format!(
            "custom world `{}` declares a bounded world constructor; portfolio output only, no claimed meaning",
            decl.name
        ),
        class: "custom-world-constructor".into(),
        scope: "declaration".into(),
        assumptions: Vec::new(),
        producer: "source".into(),
        checker: None,
        verdict: emath_ir::ClaimVerdict::NotRun,
        level: emath_ir::EvidenceLevel::E1,
        falsifiers: vec!["expansion is non-deterministic or mints evidence".into()],
        artifacts: Vec::new(),
        fresh_until: None,
    });
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:custom-world",
        format!("custom world `{}` admitted; constructor levels bounded", decl.name),
        Some(decl.head_source),
    );
}

/// Declarative world (spec wave 14 / `emath-v9-06-2rdq.15`).
///
/// Admits `emath world Name:` with `operators:` maps (`"glyph" => target`),
/// optional `interpretations:`/`protect:` clauses, and exactly one `output:`
/// portfolio name. The interpretation is world-local: the package records
/// the map as data (E1/not-run, no checker) and the strict lane never
/// inherits it — a strict use of the mapped glyph refuses `E-TYPE-003`.
fn admit_world(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let mut valid = true;
    let mut output_count = 0_usize;
    for statement in &decl.body {
        match &statement.kind {
            // `output: "Portfolio"` is the `head: value` command form; a
            // section-spelled `output:` also counts.
            StmtKind::Command { head, .. } if head.first().map(String::as_str) == Some("output") => {
                output_count += 1;
            }
            StmtKind::Section(section) if section.name == "output" => {
                output_count += 1;
            }
            StmtKind::Section(section) => match section.name.as_str() {
                "operators" => {
                    for entry in &section.suite.statements {
                        match &entry.kind {
                            StmtKind::Command { head, argument } => {
                                if head.len() != 2
                                    || head[0] != "operator"
                                    || head[1].is_empty()
                                    || !matches!(argument, Some(CommandArgument::Expr(_)))
                                {
                                    diagnostics.error(
                                        "E-KIND-027",
                                        "operator maps are `\"glyph\" => target` entries",
                                        entry.source,
                                    );
                                    valid = false;
                                }
                            }
                            _ => {
                                diagnostics.error(
                                    "E-KIND-027",
                                    "operator maps are `\"glyph\" => target` entries",
                                    entry.source,
                                );
                                valid = false;
                            }
                        }
                    }
                }
                // interpretations/protect: free-form guarantees (fields,
                // exprs, or commands); deeper semantics ride the World ABI
                // bead.
                "interpretations" | "protect" => {}
                other => {
                    diagnostics.error(
                        "E-KIND-027",
                        format!("unknown world section `{other}`"),
                        section.head_source,
                    );
                    valid = false;
                }
            },
            _ => {
                diagnostics.error(
                    "E-KIND-027",
                    "a world declaration carries `operators:` / `interpretations:` / `protect:` / `output:` entries",
                    statement.source,
                );
                valid = false;
            }
        }
    }
    if output_count != 1 {
        diagnostics.error(
            "E-KIND-003",
            format!(
                "kind `world` requires exactly one `output` portfolio; application `{}` declares {output_count}",
                decl.name
            ),
            decl.head_source,
        );
        valid = false;
    }
    if !valid {
        return;
    }

    let mut declaration = package_entry(decl, "world");
    declaration.about =
        Some("Declarative world; interpretations are world-local, never strict-inherited".into());
    declaration.evidence.push(emath_ir::EvidenceClaim {
        id: "world-interpretation".into(),
        statement: format!(
            "world `{}` maps custom-term operators; interpretations apply to custom terms only, never to strict source",
            decl.name
        ),
        class: "world-interpretation".into(),
        scope: "declaration".into(),
        assumptions: Vec::new(),
        producer: "source".into(),
        checker: None,
        verdict: emath_ir::ClaimVerdict::NotRun,
        level: emath_ir::EvidenceLevel::E1,
        falsifiers: vec![
            "a world interpretation applied to strict source or minting evidence".into()
        ],
        artifacts: Vec::new(),
        fresh_until: None,
    });
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:world",
        format!("world `{}` admitted; operator maps are world-local", decl.name),
        Some(decl.head_source),
    );
}

fn admit_extern(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    if decl.as_kind != "operator" {
        diagnostics.error(
            "E-KIND-001",
            format!(
                "extern kind `{}` is not supported (operator only)",
                decl.as_kind
            ),
            decl.head_source,
        );
        return;
    }
    if !decl.generics.is_empty() {
        // Phase 1 has no generic-operator semantics; refusing here keeps
        // the extern surface honest instead of silently dropping the
        // generic parameter list downstream.
        diagnostics.error(
            "E-TYPE-112",
            "generic `extern operator` declarations are outside the Phase 1 strict subset",
            decl.head_source,
        );
        return;
    }
    if decl.signature.is_none() {
        // Missing signature: refuse the declaration entirely; admitting an
        // operator with an empty parameter list would be a silent accept.
        diagnostics.error(
            "E-SYN-101",
            "extern operator requires a parameter list and result type",
            decl.head_source,
        );
        return;
    }
    let rules = section_rules("extern").unwrap_or_default();
    validate_body(decl, &rules, diagnostics, trace);
    let mut declaration = package_entry(decl, "extern");
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:extern",
        format!("extern operator `{}` admitted", decl.name),
        Some(decl.head_source),
    );
}

fn admit_kind_application(
    decl: &Declaration,
    def: &KindDef,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    // Biform capability declarations (bead `emath-biform-cells-jswu6`):
    // a `class:` row or `spec:`/`algorithm:` side sections route to the
    // bespoke capability admission lane that reaches the IR closure
    // authority. Legacy capability declarations (imported kind schema,
    // no class row) keep the generic kind-application path byte-for-byte.
    if def.name == "capability" {
        let biform_surface = decl.body.iter().any(|stmt| match &stmt.kind {
            StmtKind::FieldDecl { name, .. } => name == "class",
            StmtKind::Section(section) => {
                matches!(section.name.as_str(), "spec" | "algorithm")
            }
            _ => false,
        });
        if biform_surface {
            admit_capability(decl, package, diagnostics, trace);
            return;
        }
    }
    let rules = sections_for_application(def);
    validate_body(decl, &rules, diagnostics, trace);
    enforce_schema(decl, def, diagnostics);
    if def.name == "family" {
        if !diagnostics.has_errors() {
            expand_family(decl, package, diagnostics, trace);
        }
        return;
    }
    if def.name == "method" {
        if !diagnostics.has_errors() {
            admit_method(decl, package, diagnostics, trace);
        }
        return;
    }
    if def.name == "experiment" {
        if !diagnostics.has_errors() {
            admit_experiment(decl, package, diagnostics, trace);
        }
        return;
    }
    if def.name == "world" {
        if !diagnostics.has_errors() {
            admit_world(decl, package, diagnostics, trace);
        }
        return;
    }
    if def.name == "migration" {
        if !diagnostics.has_errors() {
            admit_migration(decl, package, diagnostics, trace);
        }
        return;
    }
    if matches!(def.name.as_str(), "theory" | "model" | "morphism") {
        if !diagnostics.has_errors() {
            admit_categorical_kind(decl, &def.name, package, diagnostics, trace);
        }
        return;
    }
    let mut declaration = package_entry(decl, &def.name);
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:kind-application",
        format!(
            "declaration `{}` admitted under kind `{}`",
            decl.name, def.name
        ),
        Some(decl.head_source),
    );
}

/// Capability cell admission (bead `emath-biform-cells-jswu6`): a
/// `class:`-carrying capability declaration is admitted against the
/// capability layer's authority model (`crates/emath-ir/src/capability.rs`)
/// — the bounded cell descriptor ([`CellSchema`]) and, for biform cells,
/// the spec/algorithm side closure ([`assess_biform_closure`]). One cell,
/// two authorities: the `spec:` side (laws, types, units — what the cell
/// claims) and the `algorithm:` side (reference semantics — how it is
/// computed) carry INDEPENDENT evidence objects, so a green algorithm
/// test never stamps the spec proved.
///
/// Surface: body rows `class: <token>`, `version: "dotted"`,
/// `migration: frozen|"bump-and-note"`, `inputs:`/`outputs:` sections,
/// and (biform only) `spec:`/`algorithm:` sections binding
/// `evidence: "…"` with an optional `authority: authored|verified|provider`
/// row.
///
/// Refusals are the IR's typed codes surfaced as diagnostics: E-CELL-001
/// (unknown class), E-CELL-002 (missing version), E-CELL-004 (arity
/// bound), E-CELL-005 (bare cell name), E-CELL-009 (missing side),
/// E-CELL-010 (authority escalation), E-CELL-011 (one evidence object for
/// both sides). Legacy capability declarations without a `class:` row
/// keep the generic kind-application path; nothing here silently accepts
/// an unknown row — other statements refuse E-SYN-101.
fn admit_capability(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    use emath_ir::capability::{
        BiformAuthority, BiformSide, CellClass, CellSchema, MigrationPolicy, SideEvidence,
        admit_cell, assess_biform_closure,
    };

    // Fail-closed from the first check: a capability whose admission
    // produced ANY error (bad metadata row, wrong section shape, closure
    // refusal) records nothing — the same rule as `admit_field_pack` and
    // the custom-kind marker path. A malformed cell never lands in the
    // package's capability arena.
    let errors_before = diagnostics.errors().count();

    // --- metadata rows -----------------------------------------------------
    let mut class_text: Option<String> = None;
    let mut version_text: Option<String> = None;
    let mut migration_text: Option<String> = None;
    let mut spec_section: Option<&Section> = None;
    let mut algorithm_section: Option<&Section> = None;
    let mut seen_sections: BTreeSet<&str> = BTreeSet::new();

    for stmt in &decl.body {
        match &stmt.kind {
            StmtKind::FieldDecl { name, ty, .. } if name == "class" => {
                class_text = Some(type_text(ty));
            }
            StmtKind::FieldDecl { name, ty, .. } if name == "migration" => {
                migration_text = Some(type_text(ty));
            }
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("version") =>
            {
                match quoted_string_argument(argument.as_ref()) {
                    Some(text) => version_text = Some(text.to_string()),
                    None => {
                        diagnostics.error(
                            "E-SYN-101",
                            "`version:` takes a quoted schema version string, e.g. `version: \"1.0.0\"`",
                            stmt.source,
                        );
                    }
                }
            }
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("migration") =>
            {
                match quoted_string_argument(argument.as_ref()) {
                    Some(text) => migration_text = Some(text.to_string()),
                    None => {
                        diagnostics.error(
                            "E-SYN-101",
                            "`migration:` takes `frozen` or a quoted `\"bump-and-note\"`",
                            stmt.source,
                        );
                    }
                }
            }
            StmtKind::Section(section) => {
                // Every biform-relevant section is single-occurrence: a
                // repeated `spec:`/`algorithm:`/`inputs` must refuse, never
                // silently replace the first (a dropped side section's
                // evidence would skip the closure check entirely).
                if !seen_sections.insert(section.name.as_str())
                    && matches!(
                        section.name.as_str(),
                        "inputs" | "outputs" | "spec" | "algorithm"
                    )
                {
                    diagnostics.error(
                        "E-KIND-003",
                        format!(
                            "kind `capability` requires exactly one `{}` section; `{}` declares more",
                            section.name, decl.name
                        ),
                        section.head_source,
                    );
                }
                match section.name.as_str() {
                    "spec" => spec_section = Some(section),
                    "algorithm" => algorithm_section = Some(section),
                    "inputs" | "outputs" => {}
                    other => {
                        diagnostics.error(
                            "E-SYN-101",
                            format!(
                                "section `{other}` is not admitted for capability cells \
                                 (the biform surface admits `inputs`, `outputs`, `spec`, `algorithm`)"
                            ),
                            section.head_source,
                        );
                    }
                }
            }
            _ => {
                diagnostics.error(
                    "E-SYN-101",
                    "statement at declaration level is not admitted for capability cells \
                     (class/version/migration rows, then inputs/outputs/spec/algorithm sections)",
                    stmt.source,
                );
            }
        }
    }

    // `class:` was present (this lane routes here only on class/sides).
    let class = match class_text.as_deref().map(CellClass::parse) {
        Some(Ok(class)) => class,
        Some(Err(refusal)) => {
            diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
            return;
        }
        // A `spec:`/`algorithm:` trigger without a class row: the sides
        // belong to no authority class — refuse instead of defaulting.
        None => {
            diagnostics.error(
                "E-CELL-001",
                format!(
                    "capability `{}` declares biform side sections but no `class:` row \
                     (E-CELL-001); side sections require `class: biform`",
                    decl.name
                ),
                decl.head_source,
            );
            return;
        }
    };

    // Required sections mirror the kind schema: `inputs:` present and
    // exactly one `outputs:`.
    let inputs_present = decl.sections().any(|section| section.name == "inputs");
    if !inputs_present {
        diagnostics.error(
            "E-KIND-003",
            format!(
                "kind `capability` requires an `inputs` section; application `{}` has none",
                decl.name
            ),
            decl.head_source,
        );
    }
    let outputs_count = decl.sections().filter(|section| section.name == "outputs").count();
    if outputs_count != 1 {
        diagnostics.error(
            "E-KIND-003",
            format!(
                "kind `capability` requires exactly one `outputs` section; application `{}` declares {outputs_count}",
                decl.name
            ),
            decl.head_source,
        );
    }

    // --- bounded descriptor (IR authority seam) ---------------------------
    let Some(version) = version_text else {
        diagnostics.error(
            "E-CELL-002",
            format!(
                "capability cell `{}` declares no schema version (E-CELL-002); \
                 add `version: \"…\"` (bounded admission refuses a version-less cell)",
                decl.name
            ),
            decl.head_source,
        );
        return;
    };
    let migration = match migration_text.as_deref() {
        Some("frozen") => Some(MigrationPolicy::Frozen),
        Some("bump-and-note") => Some(MigrationPolicy::BumpAndNote { note: String::new() }),
        Some(other) => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "unknown migration policy `{other}` on capability `{}` (frozen | \"bump-and-note\")",
                    decl.name
                ),
                decl.head_source,
            );
            None
        }
        None => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "capability `{}` declares no `migration:` policy (frozen | \"bump-and-note\"); \
                     a cell never mutates silently",
                    decl.name
                ),
                decl.head_source,
            );
            None
        }
    };
    let Some(migration) = migration else { return };
    let arity = decl
        .sections()
        .find(|section| section.name == "inputs")
        .map(|section| {
            let count = section
                .suite
                .statements
                .iter()
                .filter(|stmt| matches!(&stmt.kind, StmtKind::FieldDecl { .. }))
                .count();
            // Saturate, never wrap: an absurd input count must fail the
            // arity bound, not wrap into a small passing arity.
            u16::try_from(count).unwrap_or(u16::MAX)
        })
        .unwrap_or(0);
    let canonical = match &package.package_path {
        Some(path) if !path.is_empty() => format!("{}.{}", path.join("."), decl.name),
        _ => decl.name.clone(),
    };
    let schema = CellSchema {
        name: emath_core::QualifiedName(canonical.clone()),
        class,
        version: version,
        migration,
        arity,
        about: Some(
            "capability cell declared in source; bounded admission records the descriptor".into(),
        ),
    };
    if let Err(refusal) = admit_cell(&schema) {
        diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
        return;
    }

    // --- biform sides: independent evidence, typed closure ----------------
    let mut sides: Vec<SideEvidence> = Vec::new();
    if class == CellClass::Biform {
        for (side, section) in [
            (BiformSide::Spec, spec_section),
            (BiformSide::Algorithm, algorithm_section),
        ] {
            let Some(section) = section else { continue };
            let default_authority = match side {
                BiformSide::Spec => BiformAuthority::Authored,
                BiformSide::Algorithm => BiformAuthority::Verified,
            };
            let mut evidence_id: Option<String> = None;
            let mut authority: Option<BiformAuthority> = None;
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Command { head, argument }
                        if head.first().map(String::as_str) == Some("evidence") =>
                    {
                        match quoted_string_argument(argument.as_ref()) {
                            Some(text) => evidence_id = Some(text.to_string()),
                            None => {
                                diagnostics.error(
                                    "E-SYN-101",
                                    format!(
                                        "the `{}` side binds `evidence: \"…\"` (a string evidence-object token)",
                                        side.as_str()
                                    ),
                                    stmt.source,
                                );
                            }
                        }
                    }
                    StmtKind::FieldDecl { name, ty, .. } if name == "authority" => {
                        let word = type_text(ty);
                        authority = match word.as_str() {
                            "authored" => Some(BiformAuthority::Authored),
                            "verified" => Some(BiformAuthority::Verified),
                            "provider" => Some(BiformAuthority::Provider),
                            other => {
                                diagnostics.error(
                                    "E-SYN-101",
                                    format!(
                                        "unknown authority `{other}` on the {} side \
                                         (authored | verified | provider)",
                                        side.as_str()
                                    ),
                                    stmt.source,
                                );
                                None
                            }
                        };
                    }
                    _ => {
                        diagnostics.error(
                            "E-SYN-101",
                            format!(
                                "the `{}` side admits `evidence: \"…\"` and `authority: <word>` entries",
                                side.as_str()
                            ),
                            stmt.source,
                        );
                    }
                }
            }
            if let Some(evidence_id) = evidence_id {
                sides.push(SideEvidence {
                    side,
                    evidence_id,
                    authority: authority.unwrap_or(default_authority),
                });
            }
            // A side with no evidence object: the closure reports the
            // typed missing-side refusal below.
        }
        for refusal in assess_biform_closure(&schema, &sides) {
            diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
        }
    } else if spec_section.is_some() || algorithm_section.is_some() {
        // Side sections are biform-only: a non-biform cell declaring
        // them would smuggle two authorities into one class. The rows
        // were accepted as sections above; refuse now, typed.
        diagnostics.error(
            "E-SYN-101",
            format!(
                "capability `{}` declares `spec:`/`algorithm:` side sections but class `{}` \
                 is not `biform`; side sections carry independent evidence and only a biform \
                 cell may claim them",
                decl.name,
                class.as_str()
            ),
            decl.head_source,
        );
    }
    if diagnostics.errors().count() > errors_before {
        return;
    }

    // --- record ------------------------------------------------------------
    let mut declaration = package_entry(decl, "capability");
    declaration.about = Some(format!(
        "capability cell `{canonical}` ({}); biform sides carry independent evidence objects",
        class.as_str()
    ));
    for evidence in &sides {
        declaration.evidence.push(emath_ir::EvidenceClaim {
            id: evidence.evidence_id.clone(),
            statement: format!(
                "biform side `{}` of `{}` attested by `{}` authority as evidence object `{}`",
                evidence.side.as_str(),
                canonical,
                evidence.authority.as_str(),
                evidence.evidence_id
            ),
            class: "biform-side-evidence".into(),
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: emath_ir::ClaimVerdict::NotRun,
            level: emath_ir::EvidenceLevel::E1,
            falsifiers: vec![
                "the side's evidence object shared across sides or authority escalated".into(),
            ],
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    package.capabilities.push(emath_ir::capability::Capability {
        name: emath_core::QualifiedName(canonical.clone()),
        class,
    });
    trace.record(
        "recognize:capability",
        format!(
            "capability `{canonical}` admitted as `{}` with {} biform side(s) from `{}`",
            class.as_str(),
            sides.len(),
            decl.name
        ),
        Some(decl.head_source),
    );
}

/// Migration card (v9-06-2rdq.19): the card classifies a declared
/// change and carries the evidence that supports it. Admission rules:
/// every area in `from.changes` must be classified in `rules` as
/// presentation | meaning | evidence | provider (`E-MIGR-011` otherwise —
/// omission is never a classification, so a silent numeric-policy change
/// refuses); a `meaning` classification requires the `evidence:` section
/// (`E-MIGR-012`); `raise` in `rules` refuses outright (`E-MIGR-012`) —
/// authority never increases through the card alone. The card records as
/// a package declaration under kind `migration` with a NotRun evidence
/// claim; the rewrite engine itself is future work, this is the typed
/// record.
fn admit_migration(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(from) = decl.sections().find(|section| section.name == "from") else {
        return;
    };
    let mut valid = true;
    let from_values = command_values(from, &["kind", "to", "changes"], diagnostics);
    let from_kind = required_value(&from_values, "kind", from, diagnostics);
    let to = required_value(&from_values, "to", from, diagnostics);
    let changed: Vec<String> = from_values
        .get("changes")
        .map(|changes| {
            changes
                .split(',')
                .map(|area| area.trim().to_string())
                .filter(|area| !area.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // `rules:` classification vocabulary + authority fence.
    let mut classifications: BTreeMap<String, String> = BTreeMap::new();
    let mut has_evidence_section = decl.sections().any(|section| section.name == "evidence");
    if let Some(rules) = decl.sections().find(|section| section.name == "rules") {
        for statement in &rules.suite.statements {
            let StmtKind::Command {
                head,
                argument: Some(argument),
            } = &statement.kind
            else {
                diagnostics.error(
                    "E-KIND-027",
                    "`rules` entries use `classify <area> = <kind>`",
                    statement.source,
                );
                valid = false;
                continue;
            };
            // `classify <area> = <kind>`: the command head collects only
            // `classify` (the next word precedes `=`, so head collection
            // stops); the area arrives as the Assignment name and the
            // classification as its value.
            if head.first().map(String::as_str) == Some("raise") {
                diagnostics.error(
                    "E-MIGR-012",
                    "`raise` is refused in a migration card: the card classifies a change, \
                     it does not self-grant authority; authority never increases through the card alone",
                    statement.source,
                );
                valid = false;
                continue;
            }
            if head.first().map(String::as_str) != Some("classify") {
                diagnostics.error(
                    "E-KIND-027",
                    "`rules` entries use `classify <area> = <kind>`",
                    statement.source,
                );
                valid = false;
                continue;
            }
            let classification = match argument {
                emath_core::tree::CommandArgument::Assignment { name, value } => match &value.kind {
                    ExprKind::Str(kind) | ExprKind::Int(kind) => Some((name.clone(), kind.clone())),
                    // A bare classification word arrives as a single-segment
                    // path (`classify numeric_policy = meaning`). The parser
                    // lane moved bare command-tail words from `Str` to `Path`
                    // after the original close, so the word text is read
                    // from both shapes; the vocabulary fence below still
                    // gates which words are legal.
                    ExprKind::Path { segments, .. } if segments.len() == 1 => {
                        Some((name.clone(), segments[0].clone()))
                    }
                    _ => {
                        diagnostics.error(
                            "E-MIGR-011",
                            format!(
                                "`classify {name}` needs a classification word \
                                 (presentation | meaning | evidence | provider)"
                            ),
                            value.source,
                        );
                        valid = false;
                        None
                    }
                },
                _ => {
                    diagnostics.error(
                        "E-MIGR-011",
                        "`classify` needs `<area> = presentation|meaning|evidence|provider`",
                        statement.source,
                    );
                    valid = false;
                    None
                }
            };
            let Some((area, kind)) = classification else {
                continue;
            };
            if !matches!(
                kind.as_str(),
                "presentation" | "meaning" | "evidence" | "provider"
            ) {
                diagnostics.error(
                    "E-MIGR-011",
                    format!(
                        "unknown classification `{kind}` for `{area}` (presentation | meaning | evidence | provider)"
                    ),
                    statement.source,
                );
                valid = false;
                continue;
            }
            if classifications
                .insert(area.clone(), kind.clone())
                .is_some()
            {
                diagnostics.error(
                    "E-MIGR-011",
                    format!("duplicate classification for `{area}`"),
                    statement.source,
                );
                valid = false;
            }
        }
    }
    // Every DECLARED change must be classified — omission is not a
    // classification (the bead's headline negative: a silent
    // numeric-policy change refuses).
    for area in &changed {
        if !classifications.contains_key(area) {
            diagnostics.error(
                "E-MIGR-011",
                format!(
                    "declared change `{area}` has no classification in `rules:` \
                     (presentation | meaning | evidence | provider); a silent semantic change is refused"
                ),
                from.source,
            );
            valid = false;
        }
    }
    // Authority never increases through the card alone: a meaning change
    // needs NEW evidence.
    for (area, kind) in &classifications {
        if kind == "meaning" && !has_evidence_section {
            diagnostics.error(
                "E-MIGR-012",
                format!(
                    "change `{area}` is classified `meaning`; authority never increases through the card alone — \
                     a meaning-affecting migration requires an `evidence:` section"
                ),
                from.source,
            );
            valid = false;
        }
    }
    let (Some(from_kind), Some(to)) = (from_kind, to) else {
        return;
    };
    if !valid {
        return;
    }
    let mut declaration = package_entry(decl, "migration");
    declaration.about = Some("Migration card; classification + evidence recorded, authority unchanged".into());
    for (area, kind) in &classifications {
        declaration.evidence.push(emath_ir::EvidenceClaim {
            id: format!("migration-{area}"),
            statement: format!(
                "migration card `{}`: `{area}` classified `{kind}` ({from_kind} -> {to})",
                decl.name
            ),
            class: "migration-classification".into(),
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: emath_ir::ClaimVerdict::NotRun,
            level: emath_ir::EvidenceLevel::E1,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:migration",
        format!(
            "migration card `{name}` admitted: {from_kind} -> {to}, changes {changed:?} classified {classifications:?}",
            name = decl.name
        ),
        Some(decl.head_source),
    );
}

const ELEMENTWISE_UNARY_INSTANCES: &[&str] = &[
    "abs", "cos", "exp", "ln", "log10", "log2", "recip", "sin", "sqrt", "tan",
];

fn expand_family(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let mut valid = true;
    if decl.name != "ElementwiseUnary" {
        diagnostics.error(
            "E-KIND-026",
            format!(
                "unknown family `{}`; the mounted std family is `ElementwiseUnary`",
                decl.name
            ),
            decl.head_source,
        );
        valid = false;
    }
    if decl.generics.len() != 1 || decl.generics[0].name != "Op" || decl.generics[0].bound.is_some()
    {
        diagnostics.error(
            "E-KIND-026",
            "`ElementwiseUnary` requires exactly one unbounded family parameter `<Op>`",
            decl.head_source,
        );
        valid = false;
    }

    let Some(instances) = decl.sections().find(|section| section.name == "instances") else {
        return;
    };
    let mut operations = BTreeSet::new();
    for statement in &instances.suite.statements {
        let StmtKind::Expr(Expr {
            kind: ExprKind::Str(operation),
            ..
        }) = &statement.kind
        else {
            diagnostics.error(
                "E-KIND-026",
                "family instances are string operation names, for example `\"sin\"`",
                statement.source,
            );
            valid = false;
            continue;
        };
        if !ELEMENTWISE_UNARY_INSTANCES.contains(&operation.as_str()) {
            diagnostics.error(
                "E-KIND-026",
                format!("unknown ElementwiseUnary operation `{operation}`"),
                statement.source,
            );
            valid = false;
            continue;
        }
        if !operations.insert(operation.clone()) {
            diagnostics.error(
                "E-KIND-026",
                format!("duplicate ElementwiseUnary operation `{operation}`"),
                statement.source,
            );
            valid = false;
        }
    }
    if operations.len() < 3 {
        diagnostics.error(
            "E-KIND-026",
            "a family requires at least three distinct instances; keep one-off operations as capability cells",
            instances.source,
        );
        valid = false;
    }
    if !valid {
        return;
    }

    let float = package.push_type(emath_ir::TypeNode::Float64);
    for operation in operations {
        let input = package.push_expr(
            emath_ir::ExprNode::Variable(emath_core::QualifiedName("x".into())),
            instances.source,
        );
        let result = package.push_expr(
            emath_ir::ExprNode::Call {
                function: emath_core::QualifiedName(operation.clone()),
                arguments: vec![input],
            },
            instances.source,
        );
        let mut name = operation.clone();
        if let Some(first) = name.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let id =
            emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
        package.declarations.push(emath_ir::Declaration {
            id,
            name: emath_core::QualifiedName(name.clone()),
            kind: emath_core::QualifiedName("capability".into()),
            kind_label: "capability".into(),
            inputs: vec![emath_ir::Field {
                name: "x".into(),
                ty: float,
                visibility: emath_ir::Visibility::Public,
                source: instances.source,
            }],
            outputs: vec![emath_ir::Field {
                name: "value".into(),
                ty: float,
                visibility: emath_ir::Visibility::Public,
                source: instances.source,
            }],
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions: BTreeMap::from([("value".into(), result)]),
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: Some(format!(
                "Generated by ElementwiseUnary<Op> for `{operation}`"
            )),
            evidence: Vec::new(),
            host: Vec::new(),
            source: decl.source,
        });
        trace.record(
            "recognize:family-instance",
            format!("family `{}` generated capability `{name}`", decl.name),
            Some(instances.source),
        );
    }
}

/// Method cards describe an algorithm and a standing falsifier. They are
/// proposals: admission records an E1/not-run claim with no checker and
/// never raises evidence authority, and methods stay optional on ordinary
/// files (no desugar into function semantics).
fn admit_method(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(algorithm) = decl.sections().find(|section| section.name == "algorithm") else {
        return;
    };
    let Some(falsifier) = decl.sections().find(|section| section.name == "falsifier") else {
        return;
    };
    let mut valid = true;
    // The algorithm family is a free string; it names what the method
    // proposes and mounts no domain-specific compiler branch.
    let algorithm_values = command_values(algorithm, &["kind"], diagnostics);
    let algorithm_kind = required_value(&algorithm_values, "kind", algorithm, diagnostics);
    let falsifier_values = command_values(falsifier, &["condition"], diagnostics);
    let condition = required_value(&falsifier_values, "condition", falsifier, diagnostics);
    if let Some(authority) = decl.sections().find(|section| section.name == "authority") {
        let values = command_values(authority, &["claims"], diagnostics);
        match required_value(&values, "claims", authority, diagnostics) {
            Some(claims) if claims == "proposal" => {}
            Some(claims) => {
                diagnostics.error(
                    "E-KIND-027",
                    format!(
                        "method authority is proposal-only; `{claims}` cannot be self-granted by a method declaration"
                    ),
                    authority.source,
                );
                valid = false;
            }
            None => valid = false,
        }
    }
    let (Some(algorithm_kind), Some(condition)) = (algorithm_kind, condition) else {
        return;
    };
    if !valid {
        return;
    }

    let mut declaration = package_entry(decl, "method");
    declaration.about = Some("Method proposal; authority stays proposal-only (E1, not-run)".into());
    declaration.evidence.push(emath_ir::EvidenceClaim {
        id: "method-proposal".into(),
        statement: format!(
            "method `{}` proposes a `{algorithm_kind}` algorithm; falsifiable when: {condition}",
            decl.name
        ),
        class: "method-proposal".into(),
        scope: "declaration".into(),
        assumptions: Vec::new(),
        producer: "source".into(),
        checker: None,
        verdict: emath_ir::ClaimVerdict::NotRun,
        level: emath_ir::EvidenceLevel::E1,
        falsifiers: vec![condition],
        artifacts: Vec::new(),
        fresh_until: None,
    });
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:method",
        format!(
            "method `{}` admitted as proposal-only; falsifier recorded",
            decl.name
        ),
        Some(decl.head_source),
    );
}

/// Research-programme sections (Wave 9 L4). An experiment references
/// methods and providers by name; it does not embed them, does not
/// re-implement goal-first verbs, and cannot raise evidence authority.
/// Protect constraints stay evidence policy: a keep-gate records what may
/// be promoted, it never promotes by declaration.
fn admit_experiment(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(problems) = decl.sections().find(|section| section.name == "problems") else {
        return;
    };
    let mut valid = true;
    // Each problem is a string naming a tracked problem or an
    // `open ...` statement naming an open question; both are problems.
    // An empty problems section is not a research programme.
    let mut problem_names = Vec::new();
    for statement in &problems.suite.statements {
        match &statement.kind {
            StmtKind::Expr(Expr {
                kind: ExprKind::Str(problem),
                ..
            }) => problem_names.push(problem.clone()),
            StmtKind::Command { head, argument, .. }
                if head.first().map(String::as_str) == Some("open") =>
            {
                problem_names.push(match argument {
                    Some(emath_core::tree::CommandArgument::Expr(expr)) => {
                        expr_text(expr).to_string()
                    }
                    _ => "open problem".to_string(),
                });
            }
            _ => {
                diagnostics.error(
                    "E-KIND-027",
                    "experiment problems are string names or `open ...` statements",
                    statement.source,
                );
                valid = false;
            }
        }
    }
    if problem_names.is_empty() {
        diagnostics.error(
            "E-KIND-027",
            "an experiment requires at least one problem name",
            problems.source,
        );
        valid = false;
    }
    if let Some(methods) = decl.sections().find(|section| section.name == "methods") {
        for statement in &methods.suite.statements {
            let valid_reference = matches!(
                &statement.kind,
                StmtKind::Expr(Expr {
                    kind: ExprKind::Str(_),
                    ..
                })
            );
            if !valid_reference {
                diagnostics.error(
                    "E-KIND-027",
                    "experiment methods reference method declarations by name",
                    statement.source,
                );
                valid = false;
            }
        }
    }
    if let Some(keep) = decl.sections().find(|section| section.name == "keep") {
        for statement in &keep.suite.statements {
            let StmtKind::Command { head, .. } = &statement.kind else {
                diagnostics.error(
                    "E-KIND-027",
                    "keep-gates use `key: ...` statements",
                    statement.source,
                );
                valid = false;
                continue;
            };
            match head.first().map(String::as_str) {
                // `record` is a reserved section-head spelling
                // (`record Name:`), so keep-gates use `gate:` instead.
                // `allow:` would grant an authority the experiment kind
                // does not carry, so it is refused like any other key.
                Some("policy") | Some("gate") => {}
                other => {
                    diagnostics.error(
                        "E-KIND-027",
                        format!(
                            "unknown keep-gate key `{}`; use `policy:` or `gate:` (a keep-gate cannot grant authority via `allow:`)",
                            other.unwrap_or("<missing>")
                        ),
                        statement.source,
                    );
                    valid = false;
                }
            }
        }
    }
    if !valid {
        return;
    }

    // A keep-gate is a record of promotion policy, not a promotion: the
    // claim stays E1/not-run with no checker.
    let about = match decl.sections().find(|section| section.name == "keep") {
        Some(_) => "Research programme; keep-gates record policy, not promotion (E1, not-run)".into(),
        None => "Research programme; references methods, does not embed them (E1, not-run)".into(),
    };
    let mut declaration = package_entry(decl, "experiment");
    declaration.about = Some(about);
    let falsifier: Vec<String> = problem_names
        .iter()
        .map(|problem| format!("programme falsifies when `{problem}` closes under a checked method"))
        .collect();
    declaration.evidence.push(emath_ir::EvidenceClaim {
        id: "experiment-programme".into(),
        statement: format!(
            "experiment `{}` tracks {} problem(s); methods and providers are references",
            decl.name,
            problem_names.len()
        ),
        class: "research-programme".into(),
        scope: "declaration".into(),
        assumptions: Vec::new(),
        producer: "source".into(),
        checker: None,
        verdict: emath_ir::ClaimVerdict::NotRun,
        level: emath_ir::EvidenceLevel::E1,
        falsifiers: falsifier,
        artifacts: Vec::new(),
        fresh_until: None,
    });
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        "recognize:experiment",
        format!(
            "experiment `{}` admitted as research programme; authority unchanged",
            decl.name
        ),
        Some(decl.head_source),
    );
}

/// Kind applications inherit the schema of their kind definition, with the
/// full section vocabulary available for the schema to allow.
fn sections_for_application(def: &KindDef) -> Vec<SectionRule> {
    let mut rules = function_sections_for_application();
    let mut names: BTreeSet<String> = rules.iter().map(|rule| rule.name.clone()).collect();
    // Migration cards (v9-06-2rdq.19): the `evidence:` section nests
    // `claim <name>:` blocks exactly like the ordinary evidence section —
    // REPLACE the pre-seeded function rule instead of adding a duplicate
    // (the dedupe below would otherwise keep the nested-less variant).
    if def.name == "migration" {
        for rule in rules.iter_mut() {
            if rule.name == "evidence" {
                rule.nested = &[NestedRule {
                    name: "claim",
                    statement_shapes: EVIDENCE_STMTS,
                    command_first_words: &[],
                }];
                break;
            }
        }
    }
    for schema_rule in &def.schema {
        let name = match schema_rule {
            SchemaRule::RequireSection(name)
            | SchemaRule::RequireExactlyOneSection(name)
            | SchemaRule::AllowSection(name) => name,
        };
        if !names.insert(name.clone()) {
            continue;
        }
        let rule = match name.as_str() {
            "equation" => sec("equation", EQUATION_STMTS),
            "constraint" => sec("constraint", CONSTRAINT_STMTS),
            "evidence" => sec("evidence", EVIDENCE_STMTS),
            "witness" => sec("witness", EVIDENCE_STMTS),
            "invariant" => sec("invariant", EXPR_STMTS),
            "protect" => sec("protect", EXPR_STMTS),
            "host" => cmd_sec("host", &["rust"]),
            "structure" | "finite" | "mapping" | "algorithm" | "falsifier" | "authority"
            | "keep" | "from" | "rules" => SectionRule {
                name: name.clone(),
                generics: None,
                statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                command_first_words: &[],
                fn_heads: &[],
                nested: &[],
            },
            "problems" | "methods" | "providers" => SectionRule {
                name: name.clone(),
                generics: None,
                statement_shapes: &[StmtShapeKind::Exprs, StmtShapeKind::CommandsAny],
                command_first_words: &[],
                fn_heads: &[],
                nested: &[],
            },
            // World sections (wave 14): operator maps are `operator <glyph>`
            // commands with path targets; interpretations are untyped
            // guarantee fields (`total`, `deterministic`).
            "operators" | "interpretations" | "output" => SectionRule {
                name: name.clone(),
                generics: None,
                statement_shapes: &[
                    StmtShapeKind::Fields,
                    StmtShapeKind::Exprs,
                    StmtShapeKind::CommandsAny,
                ],
                command_first_words: &[],
                fn_heads: &[],
                nested: &[],
            },
            "laws" => sec("laws", EXPR_STMTS),
            other => SectionRule {
                name: other.to_string(),
                generics: None,
                statement_shapes: EXPR_STMTS,
                command_first_words: &[],
                fn_heads: &[],
                nested: &[],
            },
        };
        rules.push(rule);
    }
    rules
}

fn command_values(
    section: &emath_core::tree::Section,
    allowed: &[&str],
    diagnostics: &mut Diagnostics,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for statement in &section.suite.statements {
        let StmtKind::Command {
            head,
            argument: Some(argument),
        } = &statement.kind
        else {
            diagnostics.error(
                "E-KIND-027",
                format!("`{}` fields use `key: value`", section.name),
                statement.source,
            );
            continue;
        };
        let Some(key) = head.first() else {
            continue;
        };
        if !allowed.contains(&key.as_str()) {
            diagnostics.error(
                "E-KIND-027",
                format!("unknown `{}` key `{key}`", section.name),
                statement.source,
            );
            continue;
        }
        let value = match argument {
            emath_core::tree::CommandArgument::Expr(expr) => match &expr.kind {
                ExprKind::Str(value) | ExprKind::Int(value) => value.clone(),
                _ => {
                    diagnostics.error(
                        "E-KIND-027",
                        format!("`{key}` requires a string or integer literal"),
                        expr.source,
                    );
                    continue;
                }
            },
            emath_core::tree::CommandArgument::Assignment { .. }
            | emath_core::tree::CommandArgument::List(_) => {
                diagnostics.error(
                    "E-KIND-027",
                    format!("`{key}` requires a direct value"),
                    statement.source,
                );
                continue;
            }
        };
        if values.insert(key.clone(), value).is_some() {
            diagnostics.error(
                "E-KIND-027",
                format!("duplicate `{}` key `{key}`", section.name),
                statement.source,
            );
        }
    }
    values
}

fn required_value(
    values: &BTreeMap<String, String>,
    key: &str,
    section: &emath_core::tree::Section,
    diagnostics: &mut Diagnostics,
) -> Option<String> {
    values.get(key).cloned().or_else(|| {
        diagnostics.error(
            "E-KIND-027",
            format!("`{}` requires `{key}: ...`", section.name),
            section.source,
        );
        None
    })
}

fn evidence_claim(
    id: &str,
    statement: String,
    verdict: emath_ir::ClaimVerdict,
    level: emath_ir::EvidenceLevel,
) -> emath_ir::EvidenceClaim {
    emath_ir::EvidenceClaim {
        id: id.into(),
        statement,
        class: "finite-exhaustive".into(),
        scope: "declaration".into(),
        assumptions: Vec::new(),
        producer: "source".into(),
        checker: (verdict == emath_ir::ClaimVerdict::Pass).then(|| "native.finite-law/v1".into()),
        verdict,
        level,
        falsifiers: Vec::new(),
        artifacts: Vec::new(),
        fresh_until: None,
    }
}

fn admit_categorical_kind(
    decl: &Declaration,
    kind: &str,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    match kind {
        "theory" => admit_theory(decl, package, diagnostics, trace),
        "model" => admit_finite_model(decl, package, diagnostics, trace),
        "morphism" => admit_finite_morphism(decl, package, diagnostics, trace),
        _ => unreachable!(),
    }
}

fn admit_theory(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(structure) = decl.sections().find(|section| section.name == "structure") else {
        return;
    };
    let values = command_values(
        structure,
        &["carrier", "operation", "identity"],
        diagnostics,
    );
    let Some(carrier) = required_value(&values, "carrier", structure, diagnostics) else {
        return;
    };
    let Some(operation) = required_value(&values, "operation", structure, diagnostics) else {
        return;
    };
    let Some(identity) = required_value(&values, "identity", structure, diagnostics) else {
        return;
    };
    if carrier != "finite" || operation != "binary" {
        diagnostics.error(
            "E-KIND-027",
            "the finite checker currently requires `carrier: \"finite\"` and `operation: \"binary\"`",
            structure.source,
        );
        return;
    }
    let Ok(identity) = identity.parse::<u32>() else {
        diagnostics.error(
            "E-KIND-027",
            "theory identity must be a non-negative integer",
            structure.source,
        );
        return;
    };
    let Some(laws) = decl.sections().find(|section| section.name == "laws") else {
        return;
    };
    let mut law_names = BTreeSet::new();
    for statement in &laws.suite.statements {
        let StmtKind::Expr(Expr {
            kind: ExprKind::Str(law),
            ..
        }) = &statement.kind
        else {
            diagnostics.error(
                "E-KIND-027",
                "theory laws are string names",
                statement.source,
            );
            continue;
        };
        if !matches!(law.as_str(), "associative" | "identity") {
            diagnostics.error(
                "E-KIND-027",
                format!("unknown finite theory law `{law}`"),
                statement.source,
            );
            continue;
        }
        law_names.insert(law.clone());
    }
    if law_names.is_empty() || diagnostics.has_errors() {
        return;
    }

    let mut declaration = package_entry(decl, "theory");
    let identity_expr = package.push_expr(
        emath_ir::ExprNode::Literal(emath_ir::Literal::Integer(identity.to_string())),
        structure.source,
    );
    declaration
        .definitions
        .insert("identity".into(), identity_expr);
    for law in law_names {
        let expr = package.push_expr(
            emath_ir::ExprNode::Call {
                function: emath_core::QualifiedName(format!("law::{law}")),
                arguments: Vec::new(),
            },
            laws.source,
        );
        declaration.definitions.insert(format!("law_{law}"), expr);
        declaration.evidence.push(evidence_claim(
            &law,
            format!("declared `{law}` law; authority awaits a checked model"),
            emath_ir::ClaimVerdict::NotRun,
            emath_ir::EvidenceLevel::E1,
        ));
    }
    declaration.about = Some("Finite algebraic theory; declaration does not self-certify".into());
    push_categorical_declaration(package, declaration, trace, "theory");
}

fn finite_operation(
    left_coefficient: u32,
    right_coefficient: u32,
    left: u32,
    right: u32,
    modulus: u32,
) -> u32 {
    let value = u64::from(left_coefficient) * u64::from(left)
        + u64::from(right_coefficient) * u64::from(right);
    u32::try_from(value % u64::from(modulus)).unwrap_or(0)
}

fn finite_scale(scale: u32, value: u32, modulus: u32) -> u32 {
    let value = u64::from(scale) * u64::from(value);
    u32::try_from(value % u64::from(modulus)).unwrap_or(0)
}

fn admit_finite_model(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(section) = decl.sections().find(|section| section.name == "finite") else {
        return;
    };
    let values = command_values(
        section,
        &[
            "theory",
            "modulus",
            "left_coefficient",
            "right_coefficient",
            "identity",
        ],
        diagnostics,
    );
    let Some(theory_name) = required_value(&values, "theory", section, diagnostics) else {
        return;
    };
    let Some(modulus_text) = required_value(&values, "modulus", section, diagnostics) else {
        return;
    };
    let Some(left_coefficient_text) =
        required_value(&values, "left_coefficient", section, diagnostics)
    else {
        return;
    };
    let Some(right_coefficient_text) =
        required_value(&values, "right_coefficient", section, diagnostics)
    else {
        return;
    };
    let Some(identity_text) = required_value(&values, "identity", section, diagnostics) else {
        return;
    };
    let Some(theory) = package
        .declarations
        .iter()
        .find(|candidate| candidate.kind_label == "theory" && candidate.name.leaf() == theory_name)
    else {
        diagnostics.error(
            "E-KIND-027",
            format!("finite model references unknown prior theory `{theory_name}`"),
            section.source,
        );
        return;
    };
    let theory_evidence = theory.evidence.clone();
    let Ok(modulus) = modulus_text.parse::<u32>() else {
        diagnostics.error("E-KIND-027", "modulus must be an integer", section.source);
        return;
    };
    let Ok(identity) = identity_text.parse::<u32>() else {
        diagnostics.error("E-KIND-027", "identity must be an integer", section.source);
        return;
    };
    let Ok(left_coefficient) = left_coefficient_text.parse::<u32>() else {
        diagnostics.error(
            "E-KIND-027",
            "left_coefficient must be an integer",
            section.source,
        );
        return;
    };
    let Ok(right_coefficient) = right_coefficient_text.parse::<u32>() else {
        diagnostics.error(
            "E-KIND-027",
            "right_coefficient must be an integer",
            section.source,
        );
        return;
    };
    if modulus == 0 || modulus > 256 || identity >= modulus {
        diagnostics.error(
            "E-KIND-027",
            "finite model requires 1 <= modulus <= 256 and identity < modulus",
            section.source,
        );
        return;
    }
    for claim in &theory_evidence {
        match claim.id.as_str() {
            "associative" => {
                for a in 0..modulus {
                    for b in 0..modulus {
                        for c in 0..modulus {
                            let left = finite_operation(
                                left_coefficient,
                                right_coefficient,
                                finite_operation(
                                    left_coefficient,
                                    right_coefficient,
                                    a,
                                    b,
                                    modulus,
                                ),
                                c,
                                modulus,
                            );
                            let right = finite_operation(
                                left_coefficient,
                                right_coefficient,
                                a,
                                finite_operation(
                                    left_coefficient,
                                    right_coefficient,
                                    b,
                                    c,
                                    modulus,
                                ),
                                modulus,
                            );
                            if left != right {
                                diagnostics.error(
                                    "E-LAW-003",
                                    format!(
                                        "model `{}` falsifies associativity at ({a}, {b}, {c}): {left} != {right}",
                                        decl.name
                                    ),
                                    section.source,
                                );
                                return;
                            }
                        }
                    }
                }
            }
            "identity" => {
                for value in 0..modulus {
                    if finite_operation(
                        left_coefficient,
                        right_coefficient,
                        identity,
                        value,
                        modulus,
                    ) != value
                        || finite_operation(
                            left_coefficient,
                            right_coefficient,
                            value,
                            identity,
                            modulus,
                        ) != value
                    {
                        diagnostics.error(
                            "E-LAW-003",
                            format!(
                                "model `{}` falsifies identity `{identity}` at value {value}",
                                decl.name
                            ),
                            section.source,
                        );
                        return;
                    }
                }
            }
            _ => {}
        }
    }

    let mut declaration = package_entry(decl, "model");
    for (name, value) in [
        ("modulus", modulus),
        ("identity", identity),
        ("left_coefficient", left_coefficient),
        ("right_coefficient", right_coefficient),
    ] {
        let expression = package.push_expr(
            emath_ir::ExprNode::Literal(emath_ir::Literal::Integer(value.to_string())),
            section.source,
        );
        declaration.definitions.insert(name.into(), expression);
    }
    declaration.evidence = theory_evidence
        .iter()
        .map(|claim| {
            evidence_claim(
                &claim.id,
                format!(
                    "exhaustive `{}` check over {}^{} tuples for model `{}`",
                    claim.id,
                    modulus,
                    if claim.id == "associative" { 3 } else { 1 },
                    decl.name
                ),
                emath_ir::ClaimVerdict::Pass,
                emath_ir::EvidenceLevel::E2,
            )
        })
        .collect();
    declaration.about = Some(format!(
        "Finite model of `{theory_name}`; authority raised only by exhaustive checking"
    ));
    push_categorical_declaration(package, declaration, trace, "model");
}

fn model_spec(package: &emath_ir::SemanticPackage, name: &str) -> Option<(u32, u32, u32)> {
    let declaration = package
        .declarations
        .iter()
        .find(|declaration| declaration.kind_label == "model" && declaration.name.leaf() == name)?;
    let modulus = declaration
        .definitions
        .get("modulus")
        .and_then(|id| package.expr(*id))
        .and_then(|expr| match expr {
            emath_ir::ExprNode::Literal(emath_ir::Literal::Integer(value)) => value.parse().ok(),
            _ => None,
        })?;
    let integer_definition = |name: &str| {
        declaration
            .definitions
            .get(name)
            .and_then(|id| package.expr(*id))
            .and_then(|expr| match expr {
                emath_ir::ExprNode::Literal(emath_ir::Literal::Integer(value)) => {
                    value.parse().ok()
                }
                _ => None,
            })
    };
    Some((
        modulus,
        integer_definition("left_coefficient")?,
        integer_definition("right_coefficient")?,
    ))
}

fn admit_finite_morphism(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(section) = decl.sections().find(|section| section.name == "mapping") else {
        return;
    };
    let values = command_values(section, &["source", "target", "scale"], diagnostics);
    let Some(source_name) = required_value(&values, "source", section, diagnostics) else {
        return;
    };
    let Some(target_name) = required_value(&values, "target", section, diagnostics) else {
        return;
    };
    let Some(scale_text) = required_value(&values, "scale", section, diagnostics) else {
        return;
    };
    let Some((source_modulus, source_left, source_right)) = model_spec(package, &source_name)
    else {
        diagnostics.error(
            "E-KIND-027",
            format!("morphism references unknown prior model `{source_name}`"),
            section.source,
        );
        return;
    };
    let Some((target_modulus, target_left, target_right)) = model_spec(package, &target_name)
    else {
        diagnostics.error(
            "E-KIND-027",
            format!("morphism references unknown prior model `{target_name}`"),
            section.source,
        );
        return;
    };
    let Ok(scale) = scale_text.parse::<u32>() else {
        diagnostics.error(
            "E-KIND-027",
            "morphism scale must be an integer",
            section.source,
        );
        return;
    };
    for a in 0..source_modulus {
        for b in 0..source_modulus {
            let source_value = finite_operation(source_left, source_right, a, b, source_modulus);
            let mapped_source = finite_scale(scale, source_value, target_modulus);
            let mapped_a = finite_scale(scale, a, target_modulus);
            let mapped_b = finite_scale(scale, b, target_modulus);
            let target_value = finite_operation(
                target_left,
                target_right,
                mapped_a,
                mapped_b,
                target_modulus,
            );
            if mapped_source != target_value {
                diagnostics.error(
                    "E-LAW-003",
                    format!(
                        "morphism `{}` fails preservation at ({a}, {b}): {mapped_source} != {target_value}",
                        decl.name
                    ),
                    section.source,
                );
                return;
            }
        }
    }

    let mut declaration = package_entry(decl, "morphism");
    let scale_expr = package.push_expr(
        emath_ir::ExprNode::Literal(emath_ir::Literal::Integer(scale.to_string())),
        section.source,
    );
    declaration.definitions.insert("scale".into(), scale_expr);
    declaration.evidence.push(evidence_claim(
        "preserves_operation",
        format!("exhaustive preservation check from `{source_name}` to `{target_name}`"),
        emath_ir::ClaimVerdict::Pass,
        emath_ir::EvidenceLevel::E2,
    ));
    declaration.about = Some(format!(
        "Finite morphism `{source_name}` -> `{target_name}` with scale {scale}"
    ));
    push_categorical_declaration(package, declaration, trace, "morphism");
}

fn push_categorical_declaration(
    package: &mut emath_ir::SemanticPackage,
    mut declaration: emath_ir::Declaration,
    trace: &mut SemanticTrace,
    kind: &str,
) {
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    let name = declaration.name.leaf().to_string();
    let source = declaration.source;
    package.declarations.push(declaration);
    trace.record(
        "recognize:categorical",
        format!("checked `{kind}` declaration `{name}`"),
        Some(source),
    );
}

fn function_sections_for_application() -> Vec<SectionRule> {
    // The application vocabulary mirrors the function/record/policy surface;
    // the kind's schema rules decide what is required/forbidden.
    let mut rules = section_rules("function").unwrap_or_default();
    rules.push(sec("inputs", FIELD_STMTS));
    rules.push(sec("outputs", FIELD_STMTS));
    rules.push(sec("definitions", ASSIGN_STMTS));
    rules.push(sec("state", FIELD_STMTS));
    rules.push(ctor_sec());
    rules.push(sec("evidence", EVIDENCE_STMTS));
    rules.push(cmd_sec("export", &["rust"]));
    rules
}

fn enforce_schema(decl: &Declaration, def: &KindDef, diagnostics: &mut Diagnostics) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for stmt in &decl.body {
        if let StmtKind::Section(section) = &stmt.kind {
            *counts.entry(section.name.clone()).or_insert(0) += 1;
        }
    }
    for rule in &def.schema {
        match rule {
            SchemaRule::RequireSection(name) => {
                if !counts.contains_key(name) {
                    diagnostics.error(
                        "E-KIND-003",
                        format!(
                            "kind `{}` requires section `{name}`; application `{}` lacks it",
                            def.name, decl.name
                        ),
                        decl.head_source,
                    );
                }
            }
            SchemaRule::RequireExactlyOneSection(name) => {
                if counts.get(name).copied().unwrap_or(0) != 1 {
                    diagnostics.error(
                        "E-KIND-003",
                        format!(
                            "kind `{}` requires exactly one `{name}` section; application `{}` declares {}",
                            def.name,
                            decl.name,
                            counts.get(name).copied().unwrap_or(0)
                        ),
                        decl.head_source,
                    );
                }
            }
            SchemaRule::AllowSection(_) => {}
        }
    }
}

fn package_entry(decl: &Declaration, kind: &str) -> emath_ir::Declaration {
    emath_ir::Declaration {
        id: emath_ir::DeclarationId(0),
        name: emath_core::QualifiedName(decl.name.clone()),
        kind: emath_core::QualifiedName(kind.to_string()),
        kind_label: kind.to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: emath_ir::goal::CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: decl.source,
    }
}

/// Validate a declaration body against its kind's section rules, emitting
/// trace entries for every recognized statement.
fn validate_body(
    decl: &Declaration,
    rules: &[SectionRule],
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    for stmt in &decl.body {
        match &stmt.kind {
            StmtKind::Section(section) => {
                let Some(rule) = rules.iter().find(|rule| rule.name == section.name) else {
                    diagnostics.error(
                        "E-SYN-101",
                        format!(
                            "section `{}` is not admitted for kind `{}`",
                            section.name, decl.item_kind
                        ),
                        section.head_source,
                    );
                    continue;
                };
                if !generic_allowed(rule, section.generic.as_deref()) {
                    diagnostics.error(
                        "E-SYN-101",
                        format!(
                            "section `{}` does not admit qualifier `{}`",
                            section.name,
                            section.generic.as_deref().unwrap_or("")
                        ),
                        section.head_source,
                    );
                    continue;
                }
                trace.record(
                    "recognize:section",
                    format_section_trace(section),
                    Some(section.head_source),
                );
                admit_section(section, rule, decl, diagnostics, trace);
            }
            StmtKind::FnDecl {
                head, name, suite, ..
            } if rules.iter().any(|r| r.fn_heads.contains(&head.as_str())) => {
                admit_fn_head(head, name, suite.as_ref(), decl, diagnostics, trace);
            }
            StmtKind::Command { head, .. } if body_command_allowed(decl, head) => {
                trace.record(
                    "recognize:body",
                    format!("command {} ({})", head.join(" "), decl.item_kind),
                    Some(stmt.source),
                );
            }
            _ => {
                diagnostics.error(
                    "E-SYN-101",
                    format!(
                        "statement at declaration level is not admitted for kind `{}`",
                        decl.item_kind
                    ),
                    stmt.source,
                );
            }
        }
    }
}

fn format_section_trace(section: &emath_core::tree::Section) -> String {
    match &section.generic {
        Some(generic) => format!("{} {generic}", section.name),
        None => section.name.clone(),
    }
}

fn body_command_allowed(decl: &Declaration, head: &[String]) -> bool {
    let Some(first) = head.first() else {
        return false;
    };
    match decl.item_kind.as_str() {
        // `extends policy` on kind definitions; `representation <type>` on
        // type aliases.
        "kind" => first == "extends",
        "type" => first == "representation",
        // World applications (wave 14): `output: "Portfolio"` names the
        // interpretation portfolio at body level.
        "custom" => decl.as_kind == "world" && first == "output",
        _ => false,
    }
}

fn generic_allowed(rule: &SectionRule, generic: Option<&str>) -> bool {
    match (rule.generics, generic) {
        (None, _) | (Some(_), None) => true,
        (Some(allowed), Some(generic)) => allowed.contains(&generic),
    }
}

fn admit_section(
    section: &emath_core::tree::Section,
    rule: &SectionRule,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    for stmt in &section.suite.statements {
        if let StmtKind::Section(nested) = &stmt.kind {
            // A nested rule with name "" accepts any nested section name
            // (used by `lower: <dotted>` blocks).
            let Some(nested_rule) = rule
                .nested
                .iter()
                .find(|nested_rule| nested_rule.name.is_empty() || nested_rule.name == nested.name)
            else {
                diagnostics.error(
                    "E-SYN-101",
                    format!(
                        "nested section `{}` is not admitted under `{}`",
                        nested.name, section.name
                    ),
                    nested.head_source,
                );
                continue;
            };
            for inner in &nested.suite.statements {
                admit_stmt(inner, nested_rule, decl, diagnostics, trace, None);
            }
            continue;
        }
        if let StmtKind::FnDecl { head, name, .. } = &stmt.kind {
            if rule.fn_heads.contains(&head.as_str()) {
                trace.record(
                    "recognize:fn",
                    format!("{} `{name}` under `{}`", head, section.name),
                    Some(stmt.source),
                );
                continue;
            }
        }
        admit_stmt(stmt, rule, decl, diagnostics, trace, Some(section));
    }
}

#[allow(clippy::too_many_arguments)]
fn admit_stmt<R: ShapeRule>(
    stmt: &Stmt,
    rule: &R,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
    section: Option<&emath_core::tree::Section>,
) {
    if !rule
        .statement_shapes()
        .iter()
        .any(|shape| shape_accepts(*shape, stmt))
    {
        diagnostics.error(
            "E-SYN-101",
            format!(
                "statement shape is not admitted in section `{}` of kind `{}`",
                section.map_or("body", |s| s.name.as_str()),
                decl.item_kind
            ),
            stmt.source,
        );
        return;
    }
    match &stmt.kind {
        StmtKind::FieldDecl { name, ty, .. } => {
            let detail = match &ty.kind {
                TypeKind::Path {
                    segments,
                    generic_args,
                } if generic_args.is_empty()
                    && segments.last().map(String::as_str) == Some("Infer") =>
                {
                    name.clone()
                }
                _ => format!("{}: {}", name, type_text(ty)),
            };
            trace.record("recognize:field", detail, Some(stmt.source));
        }
        StmtKind::Assign { target, value } => {
            trace.record(
                "recognize:define",
                format!("{} = {}", place_text(target), expr_text(value)),
                Some(stmt.source),
            );
        }
        StmtKind::Equation { left, right } => {
            trace.record(
                "recognize:equation",
                format!("{} = {}", expr_text(left), expr_text(right)),
                Some(stmt.source),
            );
        }
        StmtKind::Require(expr) => {
            if let Some(section) = section {
                if section.name == "schema" {
                    if let Some(rule) = require_head(expr) {
                        trace.record(
                            "recognize:schema-rule",
                            format!("{rule:?}"),
                            Some(stmt.source),
                        );
                        return;
                    }
                }
            }
            trace.record("recognize:require", expr_text(expr), Some(stmt.source));
        }
        StmtKind::Expr(expr) => {
            trace.record("recognize:expression", expr_text(expr), Some(stmt.source));
        }
        StmtKind::Invariant(expr) => {
            trace.record("recognize:invariant", expr_text(expr), Some(stmt.source));
        }
        StmtKind::Command { head, argument } => {
            let first = head.first().map_or("", String::as_str);
            let allowed = rule.command_first_words().is_empty()
                || rule.command_first_words().contains(&first);
            if !allowed {
                diagnostics.error(
                    "E-SYN-101",
                    format!(
                        "command `{}` is not admitted in section `{}` of kind `{}`",
                        head.join(" "),
                        section.map_or("body", |s| s.name.as_str()),
                        decl.item_kind
                    ),
                    stmt.source,
                );
                return;
            }
            let mut text = head.join(" ");
            if let Some(argument) = argument {
                text.push(' ');
                text.push_str(&argument_text(argument));
            }
            trace.record(
                "recognize:command",
                format!("{} ({})", text, section.map_or("body", |s| s.name.as_str())),
                Some(stmt.source),
            );
        }
        StmtKind::SelfBlock { .. } => {
            trace.record("recognize:self", "Self { ... }", Some(stmt.source));
        }
        StmtKind::Let { name, ty, value } => {
            let ty_text = ty.as_ref().map_or_else(String::new, type_text);
            trace.record(
                "recognize:let",
                format!("{}: {} = {}", name, ty_text, expr_text(value)),
                Some(stmt.source),
            );
        }
        other => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "statement shape is not admitted in section `{}` of kind `{}`",
                    section.map_or("body", |s| s.name.as_str()),
                    decl.item_kind
                ),
                stmt.source,
            );
            let _ = other;
        }
    }
}

fn shape_accepts(shape: StmtShapeKind, stmt: &Stmt) -> bool {
    match shape {
        StmtShapeKind::Fields => matches!(stmt.kind, StmtKind::FieldDecl { .. }),
        StmtShapeKind::Assigns => matches!(stmt.kind, StmtKind::Assign { .. }),
        StmtShapeKind::Equations => matches!(stmt.kind, StmtKind::Equation { .. }),
        StmtShapeKind::Exprs => matches!(stmt.kind, StmtKind::Expr(_)),
        StmtShapeKind::Requires => matches!(stmt.kind, StmtKind::Require(_)),
        StmtShapeKind::CommandsAny => matches!(stmt.kind, StmtKind::Command { .. }),
    }
}

fn admit_fn_head(
    head: &str,
    name: &str,
    suite: Option<&emath_core::tree::Suite>,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    if let Some(suite) = suite {
        for stmt in &suite.statements {
            admit_fn_statement(head, name, stmt, decl, diagnostics, trace);
        }
    }
}

fn admit_fn_statement(
    head: &str,
    fn_name: &str,
    stmt: &Stmt,
    _decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    match &stmt.kind {
        StmtKind::Require(expr) => {
            trace.record(
                "recognize:constructor-require",
                format!("{head} {fn_name}: require {}", expr_text(expr)),
                Some(stmt.source),
            );
        }
        StmtKind::SelfBlock { assignments } => {
            let assignments: Vec<String> = assignments
                .iter()
                .map(|(name, value)| format!("{name} = {}", expr_text(value)))
                .collect();
            trace.record(
                "recognize:constructor-self",
                format!("{head} {fn_name}: Self {{ {} }}", assignments.join("; ")),
                Some(stmt.source),
            );
        }
        StmtKind::Expr(expr) => {
            trace.record(
                "recognize:fn-expression",
                format!("{head} {fn_name}: {}", expr_text(expr)),
                Some(stmt.source),
            );
        }
        StmtKind::Assign { target, value } => {
            trace.record(
                "recognize:fn-assign",
                format!(
                    "{head} {fn_name}: {} = {}",
                    place_text(target),
                    expr_text(value)
                ),
                Some(stmt.source),
            );
        }
        StmtKind::Command { .. } if head == "constructor" => {
            trace.record(
                "recognize:constructor-command",
                format!("{head} {fn_name}"),
                Some(stmt.source),
            );
        }
        other => {
            diagnostics.error(
                "E-SYN-101",
                format!("statement is not admitted inside `{head} {fn_name}`"),
                stmt.source,
            );
            let _ = other;
        }
    }
}

/// Element symbols recognized for static balance checking. Species are
/// chemical formulas written as identifier-shaped spellings (`H2`, `H2O`,
/// `NaCl`); only elements with a two-letter or one-letter symbol from this
/// set are counted, so a species made of unknown letters fails balance
/// loudly instead of silently passing (04 section 3.1).
const ELEMENTS: &[&str] = &[
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Fe", "Cu", "Zn", "Ag", "Sn", "I", "Ba", "Pt", "Au", "Hg", "Pb",
];

/// Count atoms per element in one species spelling. `H2O` → {H:2, O:1};
/// `NaCl` → {Na:1, Cl:1}. Longest-symbol-first greedy match; an unknown
/// letter run yields `None` (caller reports E-CHEM-BALANCE rather than
/// guessing a molecular formula).
fn count_atoms(species: &str) -> Option<BTreeMap<&'static str, u64>> {
    let bytes = species.as_bytes();
    let mut atoms: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let rest = &species[index..];
        let symbol = ELEMENTS
            .iter()
            .filter(|element| rest.starts_with(**element))
            .max_by_key(|element| element.len())?;
        index += symbol.len();
        // Optional count digits directly after the symbol.
        let digits_end = bytes[index..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map_or(bytes.len(), |offset| index + offset);
        let count: u64 = if digits_end > index {
            species[index..digits_end].parse().ok()?
        } else {
            1
        };
        index = digits_end;
        *atoms.entry(symbol).or_insert(0) += count.max(1);
    }
    Some(atoms)
}

/// Admission for `emath reaction_network Name:` (04 section 3.1, bead
/// emath-r3-reactions-section-92hq): `species:` closes the world (every
/// reaction-line species must be declared — E-CHEM-SPECIES), and element
/// balance is checked statically per reaction line (E-CHEM-BALANCE on
/// imbalance). Admission-only: reaction networks are recognized meaning,
/// not lowered into strict Phase 1 definitions.
/// One admitted equilibrium constant (04 §3.3, ds6x). The numeric value
/// and its uncertainty stay in the tree (admission is not evaluation);
/// only the honesty shape is checked here.
struct ConstantEntry {
    value: f64,
    spread: f64,
    source: emath_core::Span,
}

/// Extract central value and uncertainty from a Measured literal. The
/// tree carries the raw spellings (`value: "1.75(3)e-5"`,
/// `uncertainty_digits: "3"`); admission parses the central value and
/// derives the spread in the literal's own last-digit units (the same
/// convention as the uncertainty beads).
fn parse_measured_literal(value: &Expr) -> Option<(f64, f64)> {
    let ExprKind::Measured {
        value,
        uncertainty,
        uncertainty_digits,
        ..
    } = &value.kind
    else {
        return None;
    };
    let central: f64 = value.trim().parse().ok()?;
    if !uncertainty.is_empty() {
        let spread: f64 = uncertainty.trim().parse().ok()?;
        return Some((central, spread));
    }
    // CODATA parenthesized digits: the shared helper does the
    // exponent/frac last-digit math.
    let spread = crate::admit::measured_digits_uncertainty(value, uncertainty_digits)?;
    Some((central, spread))
}

/// Sum `k1 + k2*10^0 …` style combined uncertainty is overkill at
/// admission; the gate compares relative spread against the K/ratio gap
/// with a generous 2-sigma envelope. Kept as a helper so the mutant has
/// one seam to flip.
fn consistent_within_uncertainty(k: f64, k_spread: f64, ratio: f64, ratio_spread: f64) -> bool {
    let delta = (k - ratio).abs();
    delta <= 2.0 * (k_spread + ratio_spread) + 1e-12 * delta.max(1.0)
}

pub(crate) fn admit_reaction_network(decl: &Declaration, diagnostics: &mut Diagnostics) {
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut reactions: Vec<&emath_core::tree::Stmt> = Vec::new();
    // Equilibrium constants (`Ka: Measured<Real> in M = 1.75(3)e-5 M`):
    // name -> (value-with-uncertainty text, source). 04 §3.3 (ds6x).
    let mut constants: BTreeMap<String, ConstantEntry> = BTreeMap::new();
    // Kinetic rate constants (`kf = 2.0` entries in a `rate:` section):
    // name -> exact numeric value. 04 §3.3 (ds6x) honesty-triangle input.
    let mut rates: BTreeMap<String, f64> = BTreeMap::new();
    // Declared approximation assumptions (`assumptions: quasi_steady_state`
    // — 04 §3.5, i6ri). Declared, never ambient: a non-mass-action rate
    // law without one carries a warning receipt.
    let mut has_assumptions = false;
    // 04 §3.2 (emath-r3-stoich-tables-pqs6): declared extents of reaction
    // (`extents:` → `xi: Real in mol`) — the names the ICE equilibrium
    // identity may reference.
    let mut extents: BTreeSet<String> = BTreeSet::new();
    // ICE tables are collected during the walk and processed AFTER the
    // reaction lines: section order is free, and the derived coefficients
    // (ν) only exist once the `reactions:` lines have been read.
    let mut pending_ice_tables: Vec<&Stmt> = Vec::new();
    // Rate-law warning receipts are collected during the section walk and
    // emitted after it: the `assumptions:` section may follow the
    // `rate:` section in source order (declaration order is free).
    let mut pending_ratelaw_receipts: Vec<(String, emath_core::Span)> = Vec::new();
    for statement in &decl.body {
        match &statement.kind {
            StmtKind::Section(section) => match section.name.as_str() {
                "species" => {
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            // Bare names parse as `FieldDecl` with the
                            // Infer type marker (the same shape `inputs:`
                            // entries carry).
                            StmtKind::FieldDecl { name, .. } => {
                                declared.insert(name.clone());
                            }
                            StmtKind::Expr(expr) => {
                                if let ExprKind::Path { segments, generics: None } = &expr.kind {
                                    if let [name] = segments.as_slice() {
                                        declared.insert(name.clone());
                                        continue;
                                    }
                                }
                                diagnostics.error(
                                    "E-CHEM-SPECIES",
                                    "a `species:` entry must be a bare species name",
                                    nested.source,
                                );
                            }
                            _ => diagnostics.error(
                                "E-CHEM-SPECIES",
                                "`species:` entries are bare names, one per line",
                                nested.source,
                            ),
                        }
                    }
                }
                "reactions" => reactions.extend(section.suite.statements.iter()),
                "assumptions" => {
                    // 04 §3.5 (i6ri): declared approximations — bare
                    // names, one per line (`quasi_steady_state`). Their
                    // hashing into artifacts is the build tier; here the
                    // declaration is recognized and silences the
                    // W-CHEM-RATELAW receipt for this network.
                    has_assumptions = true;
                    for nested in &section.suite.statements {
                        let admitted = match &nested.kind {
                            StmtKind::FieldDecl { ty, .. } => {
                                matches!(&ty.kind, TypeKind::Path { segments, .. } if segments.first().map(String::as_str) == Some("Infer"))
                            }
                            StmtKind::Expr(expr) => matches!(
                                &expr.kind,
                                ExprKind::Path { segments, generics: None } if segments.len() == 1
                            ),
                            _ => false,
                        };
                        if !admitted {
                            diagnostics.error(
                                "E-KIND-027",
                                "an `assumptions:` entry must be a bare name \
                                 (`quasi_steady_state`)",
                                nested.source,
                            );
                        }
                    }
                }
                "rate" => {
                    // A rate entry is a bare numeric assignment
                    // (`kf = 2.0`, ds6x honesty-triangle input) or a
                    // named rate-law call (`v = michaelis_menten(...)`,
                    // 04 §3.5 i6ri). Full rate-law semantics are the
                    // follow-up bead.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::Assign { target, value } => {
                                match &value.kind {
                                    ExprKind::Float(text) => {
                                        let Ok(rate_value) = text.parse::<f64>() else {
                                            diagnostics.error(
                                                "E-KIND-027",
                                                "a `rate:` entry value must be a numeric \
                                                 literal (`kf = 2.0`)",
                                                nested.source,
                                            );
                                            continue;
                                        };
                                        if let Some(rate_name) = target.segments.last() {
                                            rates.insert(rate_name.clone(), rate_value);
                                        }
                                    }
                                    // Named rate-law form (04 §3.5, i6ri): the callee
                                    // names the form (`michaelis_menten` from
                                    // sci::chem::ratelaws; registry membership is the
                                    // import tier). Arguments are bare names or §3.4
                                    // context-scoped concentration brackets: `[S]`
                                    // (a single-element list) reads as
                                    // concentration-of-S when S is a declared species.
                                    ExprKind::Call { function, args } => {
                                        for arg in args {
                                            match &arg.kind {
                                                ExprKind::Path { .. } => {}
                                                ExprKind::List(items) if items.len() == 1 => {
                                                    let concentration = match &items[0].kind {
                                                        ExprKind::Path {
                                                            segments,
                                                            generics: None,
                                                        } if segments.len() == 1
                                                            && declared.contains(&segments[0]) =>
                                                        {
                                                            true
                                                        }
                                                        _ => false,
                                                    };
                                                    if !concentration {
                                                        diagnostics.error(
                                                            "E-NOTATION-AMBIG",
                                                            "a bracketed rate-law argument \
                                                             `[X]` reads as concentration-of-X \
                                                             only when X is a declared species; \
                                                             this spelling has no resolvable \
                                                             reading in the rate context",
                                                            arg.source,
                                                        );
                                                    }
                                                }
                                                _ => diagnostics.error(
                                                    "E-NOTATION-AMBIG",
                                                    "rate-law arguments are bare names or \
                                                     concentration brackets `[S]` of declared \
                                                     species; a list literal here is the \
                                                     ambiguous spelling",
                                                    arg.source,
                                                ),
                                            }
                                        }
                                        if !has_assumptions {
                                            let form = match &function.kind {
                                                ExprKind::Path { segments, .. } => segments
                                                    .last()
                                                    .cloned()
                                                    .unwrap_or_else(|| "rate law".to_string()),
                                                _ => "rate law".to_string(),
                                            };
                                            pending_ratelaw_receipts
                                                .push((form, nested.source));
                                        }
                                    }
                                    _ => diagnostics.error(
                                        "E-KIND-027",
                                        "a `rate:` entry must be `name = <numeric literal>` \
                                         or a named rate-law call",
                                        nested.source,
                                    ),
                                }
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "a `rate:` entry must be `name = <numeric literal>` or a \
                                 named rate-law call",
                                nested.source,
                            ),
                        }
                    }
                }
                "stoichiometry" => {
                    // 04 §3.2 (emath-r3-stoich-tables-pqs6): the
                    // stoichiometric matrix is DERIVED from the reaction
                    // lines. The only admitted right-hand side is exactly
                    // `stoich(reactions)`; the name binds the derived
                    // matrix for the build tier. A re-entered matrix is
                    // precisely the transcription error this section
                    // exists to make impossible.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::Assign { target, value } => {
                                let derived = matches!(
                                    &value.kind,
                                    ExprKind::Call { function, args }
                                        if matches!(
                                            &function.kind,
                                            ExprKind::Path { segments, generics: None }
                                                if segments.len() == 1 && segments[0] == "stoich"
                                        ) && args.len() == 1
                                            && matches!(
                                                &args[0].kind,
                                                ExprKind::Path { segments, generics: None }
                                                    if segments.len() == 1
                                                        && segments[0] == "reactions"
                                            )
                                );
                                if !derived {
                                    let name = target
                                        .segments
                                        .last()
                                        .cloned()
                                        .unwrap_or_else(|| "matrix".to_string());
                                    diagnostics.error(
                                        "E-CHEM-STOICH",
                                        format!(
                                            "stoichiometric matrix `{name}` must be derived: \
                                             the only admitted right-hand side is \
                                             `stoich(reactions)`; re-entering coefficients \
                                             defeats the anti-transcription check"
                                        ),
                                        nested.source,
                                    );
                                }
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "a `stoichiometry:` entry is `name = stoich(reactions)` — \
                                 coefficients are derived from the reaction lines, never \
                                 re-entered",
                                nested.source,
                            ),
                        }
                    }
                }
                "extents" => {
                    // 04 §3.2 (pqs6): typed extents of reaction
                    // (`xi: Real in mol`) — declared symbols the ICE
                    // equilibrium identity references. The bead's bare
                    // `extent xi:` prefix spelling collides with the
                    // two-word section-head grammar (`extent` + name +
                    // `:` reads as a section head, and the trailing type
                    // then trips the definition-row unit fence), so the
                    // section idiom is the admitted spelling.
                    for nested in &section.suite.statements {
                        match &nested.kind {
                            StmtKind::FieldDecl { name, .. } => {
                                extents.insert(name.clone());
                            }
                            _ => diagnostics.error(
                                "E-KIND-027",
                                "an `extents:` entry is a typed extent of reaction \
                                 (`xi: Real in mol`)",
                                nested.source,
                            ),
                        }
                    }
                }
                "constraints" => {
                    // 04 §3.2 (pqs6): concentration claims over the
                    // species carrier. Admission checks the SHAPE
                    // (forall-over-species); the numeric check itself is
                    // the eval tier. Both binder statement forms are
                    // admitted: the expression form (`forall … : body`
                    // lowering to a binder expression) and the block
                    // `BinderStmt` form the statement parser produces
                    // when the binder body follows the colon.
                    for nested in &section.suite.statements {
                        let admitted = match &nested.kind {
                            StmtKind::Expr(expr) => is_forall_over_species(expr),
                            StmtKind::BinderStmt {
                                kind: BinderKind::ForAll,
                                binders,
                                guard: None,
                                suite,
                            } => {
                                binders.len() == 1
                                    && binder_domain_is_species(&binders[0])
                                    && !suite.statements.is_empty()
                            }
                            _ => false,
                        };
                        if !admitted {
                            diagnostics.error(
                                "E-KIND-027",
                                "a `constraints:` entry is a forall-over-species \
                                 concentration claim (`forall s in species: 0 M <= [s]`)",
                                nested.source,
                            );
                        }
                    }
                }
                "ice_table" => pending_ice_tables.push(statement),
                "conservation" => {
                    // Conservation laws carry their own beads (04 section
                    // 3.1 follow-ups); the reaction-line grammar and
                    // element balance are this bead's slice.
                }
                // 04 §4.1+§4.2 (emath-r3-compartments-e5zq): the
                // compartments/populations vocabulary names its design
                // forks instead of a generic unknown-section error.
                "compartments" => diagnostics.error(
                    "E-KIND-027",
                    "`compartments:` is outside the admitted reaction-network sections — \
                     the compartments design follow-up (emath-r3-compartments-e5zq) must \
                     first settle the C15 `@` collision (species identity `Drug@central` \
                     vs the attribute sigil: an explicit lexer rule or a different \
                     separator like `Drug.at(central)`) and the sink species typing \
                     (`∅` is the declared sink); compartment-qualified identity hashes \
                     into meaning",
                    section.head_source,
                ),
                "populations" => diagnostics.error(
                    "E-KIND-027",
                    "`populations:` is outside the admitted reaction-network sections — \
                     the populations design follow-up (emath-r3-compartments-e5zq) is \
                     WORLD-DEPENDENT: `continuous_ode` (mean-field, N: Real) and \
                     `gillespie_exact` (jump process, N: Nat) are two READINGS of the \
                     same equations, kept as a labeled portfolio pair with the stated \
                     approximation relation `continuous_ode ≈ gillespie_exact for N >> 1`; \
                     the stochastic world for SSA is the named prerequisite",
                    section.head_source,
                ),
                other => diagnostics.error(
                    "E-KIND-027",
                    format!("unknown `reaction_network` section `{other}` (expected `species:`, `reactions:`, `rate:`, `conservation:`, `stoichiometry:`, `extents:`, `ice_table <reaction>:`, or `constraints:`)"),
                    section.head_source,
                ),
            },
            // Equilibrium-constant line: `Ka: Measured<Real> in M = 1.75(3)e-5 M`
            // (ds6x). `Measured` is not a hard type on this tree (admission
            // probe: `E-TYPE-001 unknown type` outside uncertainty lanes),
            // so the honesty shape comes from the VALUE: a Measured
            // literal (± or parenthesized digits) is required — a bare
            // exact literal refuses (E-CHEM-KA-EXACT).
            StmtKind::FieldDecl {
                name,
                default: Some(value),
                ..
            } => {
                if let Some((central, spread)) = parse_measured_literal(value) {
                    constants.insert(name.clone(), ConstantEntry {
                        value: central,
                        spread,
                        source: statement.source,
                    });
                } else if matches!(value.kind, ExprKind::Measured { .. }) {
                    let ExprKind::Measured { value: text, uncertainty, uncertainty_digits, .. } = &value.kind else { unreachable!() };
                    diagnostics.error(
                        "E-CHEM-KA-EXACT",
                        format!(
                            "equilibrium constant `{name}` carries a Measured literal whose \
                             value/uncertainty text does not parse (value=`{text}` \
                             uncertainty=`{uncertainty}` digits=`{uncertainty_digits}`); fix \
                             the spelling"
                        ),
                        statement.source,
                    );
                } else {
                    diagnostics.error(
                        "E-CHEM-KA-EXACT",
                        format!(
                            "equilibrium constant `{name}` must be a Measured value with \
                             uncertainty (`1.75(3)e-5` or `1.75 ± 0.03e-5`); an exact literal \
                             is the dishonest spelling for a measured constant"
                        ),
                        statement.source,
                    );
                }
            }
            other => {
                diagnostics.error(
                    "E-KIND-027",
                    "a `reaction_network` body is `species:` and `reactions:` sections",
                    statement.source,
                );
            }
        }
    }
    // W-CHEM-RATELAW receipts (04 §3.5, i6ri): emitted after the walk so
    // a later `assumptions:` section still silences them.
    if !has_assumptions {
        for (form, source) in &pending_ratelaw_receipts {
            diagnostics.warning(
                "W-CHEM-RATELAW",
                format!(
                    "named rate-law form `{form}` is non-mass-action; declare the approximation \
                     with an `assumptions:` section (e.g. `quasi_steady_state`) so the receipt \
                     is explicit, not ambient"
                ),
                *source,
            );
        }
    }
    if declared.is_empty() {
        diagnostics.error(
            "E-CHEM-SPECIES",
            "`species:` closes the world: a `reaction_network` must declare at least one species",
            decl.head_source,
        );
        return;
    }
    // Honesty triangle (04 §3.3, ds6x): a network declaring BOTH a
    // reversible kinetic pair (`<->`) AND an equilibrium over the same
    // species pair (`<=>`) must carry a constant consistent with
    // K == kf/kr within combined uncertainty. Admission checks the SHAPE:
    // both arrows present + a constant name present. Numeric kf/kr live
    // in `rate:` lines (follow-up bead), so with rates absent the gate
    // records the pair and stays silent (no false positives on plain
    // networks).
    let has_reversible = reactions.iter().any(|reaction| {
        matches!(
            &reaction.kind,
            StmtKind::Reaction { arrow: ReactionArrow::Reversible, .. }
        )
    });
    let has_equilibrium = reactions.iter().any(|reaction| {
        matches!(
            &reaction.kind,
            StmtKind::Reaction { arrow: ReactionArrow::Equilibrium, .. }
        )
    });
    if has_reversible && has_equilibrium {
        let equilibrium = reactions.iter().find(|reaction| {
            matches!(
                &reaction.kind,
                StmtKind::Reaction { arrow: ReactionArrow::Equilibrium, .. }
            )
        });
        if constants.is_empty() {
            if let Some(reaction) = equilibrium {
                diagnostics.error(
                    "E-CHEM-THERMO",
                    "this network declares both a reversible kinetic pair (`<->`) and an \
                     equilibrium (`<=>`) over the same chemistry; thermodynamic consistency \
                     requires the equilibrium constant (K == kf/kr within combined uncertainty) \
                     to be declared — add a `K: Measured<Real> = …` line",
                    reaction.source,
                );
            }
        } else if let (Some(k_entry), Some(kf), Some(kr)) = (
            // `K` by name; a sole constant declared under another name
            // (`Ka`) is read as the equilibrium constant.
            constants
                .get("K")
                .or_else(|| (constants.len() == 1).then(|| constants.values().next())?),
            rates.get("kf"),
            rates.get("kr"),
        ) {
            let ratio = kf / kr;
            // Rates are declared exact; the uncertainty envelope comes
            // from the measured K alone.
            if !consistent_within_uncertainty(k_entry.value, k_entry.spread, ratio, 0.0) {
                if let Some(reaction) = equilibrium {
                    diagnostics.error(
                        "E-CHEM-THERMO",
                        format!(
                            "thermodynamic consistency violated: declared equilibrium constant \
                             K = {} ± {} is inconsistent with kf/kr = {} (K == kf/kr must hold \
                             within combined uncertainty)",
                            k_entry.value, k_entry.spread, ratio
                        ),
                        reaction.source,
                    );
                }
            }
        }
    }
    // 04 §3.2 (pqs6): the derived stoichiometric vector ν per reaction
    // name — lhs terms negative, rhs terms positive. This is the
    // anti-transcription source of truth the ICE tables check against.
    let mut nu_by_reaction: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for reaction in reactions {
        let StmtKind::Reaction {
            name,
            lhs,
            arrow: _,
            rhs,
        } = &reaction.kind
        else {
            diagnostics.error(
                "E-CHEM-SPECIES",
                "only reaction lines are admitted inside `reactions:`",
                reaction.source,
            );
            continue;
        };
        let mut nu: BTreeMap<String, i64> = BTreeMap::new();
        for (sign, side) in [(-1i64, lhs), (1, rhs)] {
            for term in side {
                *nu.entry(term.species.clone()).or_insert(0) +=
                    sign * term.coefficient as i64;
            }
        }
        nu_by_reaction.insert(name.clone(), nu);
        for term in lhs.iter().chain(rhs.iter()) {
            if !declared.contains(&term.species) {
                diagnostics.error(
                    "E-CHEM-SPECIES",
                    format!(
                        "species `{}` is not declared in `species:` (the world is closed; no \
                         implicit species)",
                        term.species
                    ),
                    reaction.source,
                );
            }
        }
        // 04 §4.1 (emath-r3-compartments-e5zq): a sink endpoint is the
        // declared `∅` — the parser admits it as an EMPTY side. A
        // structurally empty side that did not come from a declared
        // sink cannot exist past the parser fence; belt-and-braces
        // refusal here keeps the contract machine-checkable.
        if lhs.is_empty() && rhs.is_empty() {
            diagnostics.error(
                "E-BIO-SINK",
                format!(
                    "reaction `{name}` has no terms on either side; a degradation/elimination \
                     endpoint must be the declared sink `∅` (04 §4.1), never a silently empty \
                     side"
                ),
                reaction.source,
            );
            continue;
        }
        // Static element balance: sum per element over both sides.
        let mut balance: BTreeMap<String, i64> = BTreeMap::new();
        let mut countable = true;
        for (sign, side) in [(-1i64, lhs), (1, rhs)] {
            for term in side {
                // A species that is not an element formula (`A`, `B` in
                // generic networks) is an abstract label: balance cannot
                // be checked statically, so the reaction is skipped
                // rather than refused (04 §3.3, ds6x — kinetic pairs and
                // equilibria over abstract species are first-class).
                let Some(atoms) = count_atoms(&term.species) else {
                    countable = false;
                    continue;
                };
                for (element, count) in atoms {
                    *balance.entry(element.to_string()).or_insert(0) += sign * (count * term.coefficient) as i64;
                }
            }
        }
        if countable {
            let imbalanced: Vec<(String, i64)> = balance
                .iter()
                .filter(|(_, delta)| **delta != 0)
                .map(|(element, delta)| (element.clone(), *delta))
                .collect();
            if !imbalanced.is_empty() {
                let detail = imbalanced
                    .iter()
                    .map(|(element, delta)| format!("{element}{delta:+}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics.error(
                    "E-CHEM-BALANCE",
                    format!(
                        "reaction `{name}` is not element-balanced ({detail}); stoichiometric \
                         coefficients must conserve every element"
                    ),
                    reaction.source,
                );
            }
        }
    }
    // 04 §3.2 (emath-r3-stoich-tables-pqs6): ICE tables are checked
    // against the DERIVED coefficients — the change row is never taken
    // on faith. Section order is free, so the tables were collected
    // during the walk and are processed here, after ν exists.
    for table in pending_ice_tables {
        let StmtKind::Section(section) = &table.kind else {
            unreachable!("pending_ice_tables only collects sections");
        };
        let Some(reaction_name) = section.generic.as_deref() else {
            diagnostics.error(
                "E-CHEM-STOICH",
                "an `ice_table:` must name its reaction (`ice_table r1:`); the table is \
                 checked against that reaction's derived coefficients",
                section.head_source,
            );
            continue;
        };
        let Some(nu) = nu_by_reaction.get(reaction_name) else {
            diagnostics.error(
                "E-CHEM-STOICH",
                format!(
                    "ice_table {reaction_name}: no reaction named `{reaction_name}` is declared \
                     in `reactions:` — an ICE table is checked against a declared reaction, \
                     never a silent guess"
                ),
                section.head_source,
            );
            continue;
        };
        let mut initials: BTreeMap<String, f64> = BTreeMap::new();
        let mut changes: BTreeMap<String, i64> = BTreeMap::new();
        for nested in &section.suite.statements {
            match &nested.kind {
                StmtKind::Section(row) if row.name == "initial" || row.name == "change" => {
                    for entry in &row.suite.statements {
                        let StmtKind::Assign { target, value } = &entry.kind else {
                            diagnostics.error(
                                "E-KIND-027",
                                format!(
                                    "an `ice_table` `{}` row entry is `species = value` \
                                     (`A = 1.0`)",
                                    row.name
                                ),
                                entry.source,
                            );
                            continue;
                        };
                        let Some(species) = target.segments.last() else {
                            continue;
                        };
                        let Some(value) = ice_cell_value(value) else {
                            diagnostics.error(
                                "E-CHEM-STOICH",
                                format!(
                                    "the `ice_table` `{}` value for `{species}` must be a \
                                     numeric literal (`1.0`, `-1`)",
                                    row.name
                                ),
                                entry.source,
                            );
                            continue;
                        };
                        if row.name == "initial" {
                            initials.insert(species.clone(), value);
                        } else if value.fract() == 0.0 {
                            changes.insert(species.clone(), value as i64);
                        } else {
                            diagnostics.error(
                                "E-CHEM-STOICH",
                                format!(
                                    "the ICE change coefficient for `{species}` must be an \
                                     integer (stoichiometric coefficients are integers)"
                                ),
                                entry.source,
                            );
                        }
                    }
                }
                // The equilibrium row is the derived identity; any other
                // formula is a re-entered transcription error.
                StmtKind::Assign { target, value }
                    if target.segments.len() == 1 && target.segments[0] == "equilibrium" =>
                {
                    if !is_equilibrium_identity(value, &extents) {
                        diagnostics.error(
                            "E-CHEM-STOICH",
                            "the equilibrium row is the derived identity \
                             `equilibrium = initial + xi * change` (with `xi` declared in \
                             `extents:`); a re-entered formula defeats the ICE check",
                            nested.source,
                        );
                    }
                }
                _ => diagnostics.error(
                    "E-KIND-027",
                    "an `ice_table` body is `initial:` and `change:` rows plus the optional \
                     `equilibrium = initial + xi * change` identity",
                    nested.source,
                ),
            }
        }
        // Fidelity + coverage: ICE rows cover exactly the reaction's
        // species; every change entry equals the derived ν.
        for species in initials.keys().chain(changes.keys()) {
            if !nu.contains_key(species) {
                diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: `{species}` does not participate in the \
                         reaction; ICE rows cover exactly the reaction's species"
                    ),
                    section.head_source,
                );
            }
        }
        for (species, coefficient) in nu {
            if !initials.contains_key(species.as_str()) {
                diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: species `{species}` participates in the \
                         reaction but has no `initial:` entry"
                    ),
                    section.head_source,
                );
            }
            match changes.get(species.as_str()) {
                None => diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: species `{species}` participates in the \
                         reaction but has no `change:` entry"
                    ),
                    section.head_source,
                ),
                Some(delta) if *delta != *coefficient => diagnostics.error(
                    "E-CHEM-STOICH",
                    format!(
                        "ice_table {reaction_name}: the change coefficient for `{species}` is \
                         {delta} but the reaction derives {coefficient} — coefficients are \
                         derived from the reaction line, never re-entered"
                    ),
                    section.head_source,
                ),
                _ => {}
            }
        }
    }
}

/// A forall-over-species binder expression: one binder, domain `species`,
/// no guard — the admitted `constraints:` entry shape (pqs6).
fn is_forall_over_species(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Binder {
            kind: BinderKind::ForAll,
            binders,
            guard: None,
            ..
        } if binders.len() == 1 && binder_domain_is_species(&binders[0])
    )
}

/// The binder domain is the species carrier (`in species`).
fn binder_domain_is_species(binder: &Binder) -> bool {
    binder.domain.as_ref().is_some_and(|domain| {
        matches!(
            &domain.kind,
            ExprKind::Path { segments, generics: None }
                if segments.len() == 1 && segments[0] == "species"
        )
    })
}

/// Fold an ICE-table cell (`1.0`, `-2`, `+1`) to its numeric value:
/// Int/Float literals under an optional unary sign. Nothing else is an
/// ICE concentration or coefficient.
fn ice_cell_value(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Int(text) => text.parse::<f64>().ok(),
        ExprKind::Float(text) => text.parse::<f64>().ok(),
        ExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => Some(-ice_cell_value(value)?),
        ExprKind::Unary {
            op: UnaryOp::Pos,
            value,
        } => ice_cell_value(value),
        _ => None,
    }
}

/// The equilibrium row is the DERIVED identity `initial + xi * change` —
/// exactly `Add(Path("initial"), Mul(Path(extent), Path("change")))` with
/// `extent` declared in `extents:`. A re-entered formula is the classic
/// transcription error the bead refuses (pqs6).
fn is_equilibrium_identity(expr: &Expr, extents: &BTreeSet<String>) -> bool {
    let ExprKind::Binary {
        op: BinaryOp::Add,
        left,
        right,
    } = &expr.kind
    else {
        return false;
    };
    let ExprKind::Path { segments, generics: None } = &left.kind else {
        return false;
    };
    if segments.len() != 1 || segments[0] != "initial" {
        return false;
    }
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left: factor,
        right: rows,
    } = &right.kind
    else {
        return false;
    };
    let ExprKind::Path { segments: extent, generics: None } = &factor.kind else {
        return false;
    };
    if extent.len() != 1 || !extents.contains(&extent[0]) {
        return false;
    }
    matches!(
        &rows.kind,
        ExprKind::Path { segments, generics: None }
            if segments.len() == 1 && segments[0] == "change"
    )
}
