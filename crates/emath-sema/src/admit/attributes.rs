//! Item-attribute admission gates (ELP governance), restored from the
//! pre-contraction recognition layer.
//!
//! `@experimental` / `@capabilities` lane gating, `@units_profile`
//! ladders, and `@significant_figures` precision contracts. Experimental
//! syntax must never compile silently in a stable package: every gate
//! below is a typed refusal or a declared admit.

use emath_core::tree::{
    ArgumentValue, Attribute, CommandArgument, Declaration, Expr, ExprKind, Item, Section, Stmt,
    StmtKind,
};
use emath_core::Diagnostics;
use std::collections::BTreeSet;

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
    let mut ledger = emath_core::PrecisionLedger::default();
    for stmt in &decl.body {
        collect_stmt_precision(stmt, &mut literals, &mut ledger);
    }
    if let Some(declared) = enforce_count {
        for (text, span) in &literals {
            if let Some(literal_sf) = emath_core::count_sig_figs(text) {
                if literal_sf < declared {
                    diagnostics.warning(
                        emath_core::E_SF_UNDER_REPORT,
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
    if let Some(emath_core::PrecisionWarning::MixedMeasuredBareSf { measured, bare_sf }) =
        ledger.mix_warning()
    {
        diagnostics.warning(
            emath_core::E_SF_MIXED_KINDS,
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
    ledger: &mut emath_core::PrecisionLedger,
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
    ledger: &mut emath_core::PrecisionLedger,
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
