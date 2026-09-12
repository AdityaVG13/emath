//! Generic declaration, section-schema, field-pack, and extern admission.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// Admit one declaration into the package and trace.
///
/// Kind-registry admission is leftover. Live constructor admission is
/// `check_tree` / `admit_constructor_declaration`.
#[allow(unreachable_code, unused_variables)]
pub fn admit_declaration(
    decl: &emath_core::tree::Declaration,
    kind_defs: &BTreeMap<String, KindDef>,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    diagnostics.error(
        "E-KIND-GONE",
        "kind-registry admission is not constructor surface; write `emath object`, `emath function`, or `emath query`",
        decl.head_source,
    );
    return;
    let item_kind = decl.item_kind.as_str();
    if item_kind == "extern" {
        admit_extern(decl, package, diagnostics, trace);
        return;
    }
    let schema_kind = if item_kind == "custom" {
        decl.as_kind.as_str()
    } else {
        item_kind
    };
    if matches!(
        schema_kind,
        "model" | "policy" | "kind" | "law" | "search" | "experiment" | "reaction_network"
    ) {
        diagnostics.error(
            "E-KIND-GONE",
            format!(
                "declaration kind `{schema_kind}` is not a core kind; write `emath object`, `emath function`, or `emath query`"
            ),
            decl.head_source,
        );
        return;
    }
    if schema_kind == "capability" {
        super::capability::admit_capability(decl, package, diagnostics, trace);
        return;
    }
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
    let errors_before = diagnostics.errors().count();
    validate_body(decl, &rules, diagnostics, trace);
    if diagnostics.errors().count() > errors_before {
        return;
    }
    let mut declaration = package_entry(decl, item_kind);
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    trace.record(
        if item_kind == "kind" {
            "recognize:kind-registered"
        } else {
            "recognize:admit"
        },
        format!("declaration `{}` kind `{item_kind}` admitted", decl.name),
        Some(decl.head_source),
    );
}

fn admit_kind_application(
    decl: &emath_core::tree::Declaration,
    def: &KindDef,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let rules = sections_for_application(def);
    let errors_before = diagnostics.errors().count();
    validate_body(decl, &rules, diagnostics, trace);
    enforce_schema(decl, def, diagnostics);
    if diagnostics.errors().count() > errors_before {
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

/// Schema-only validation for a kind application. Function-shaped kinds
/// are lowered by the generic declaration pass after this returns true.
pub(crate) fn validate_kind_application(
    decl: &emath_core::tree::Declaration,
    def: &KindDef,
    diagnostics: &mut Diagnostics,
    _trace: &mut SemanticTrace,
) -> bool {
    // Function-shaped applications are section-checked by the generic
    // Admitter (`inputs`/`definitions`/`tests`). Recognition function
    // rules use different names (`input`/`define`) and must not refuse
    // a valid Phase 1 body here.
    let errors_before = diagnostics.errors().count();
    enforce_schema(decl, def, diagnostics);
    diagnostics.errors().count() == errors_before
}

fn sections_for_application(def: &KindDef) -> Vec<SectionRule> {
    let mut rules = section_rules("function").unwrap_or_default();
    let mut names: BTreeSet<String> = rules.iter().map(|rule| rule.name.clone()).collect();
    for schema_rule in &def.schema {
        let name = match schema_rule {
            SchemaRule::RequireSection(name)
            | SchemaRule::RequireExactlyOneSection(name)
            | SchemaRule::AllowSection(name) => name,
        };
        if !names.insert(name.clone()) {
            continue;
        }
        let shapes = match name.as_str() {
            "input" | "inputs" | "output" | "outputs" | "parameter" | "state" => FIELD_STMTS,
            "define" | "definitions" => ASSIGN_STMTS,
            "equation" | "equations" => EQUATION_STMTS,
            "constraint" | "constraints" => CONSTRAINT_STMTS,
            _ => &[
                StmtShapeKind::Fields,
                StmtShapeKind::Assigns,
                StmtShapeKind::Equations,
                StmtShapeKind::Exprs,
                StmtShapeKind::Requires,
                StmtShapeKind::CommandsAny,
            ],
        };
        rules.push(SectionRule {
            name: name.clone(),
            generics: None,
            statement_shapes: shapes,
            command_first_words: &[],
            fn_heads: &[],
            nested: &[],
        });
    }
    rules
}

fn enforce_schema(
    decl: &emath_core::tree::Declaration,
    def: &KindDef,
    diagnostics: &mut Diagnostics,
) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for stmt in &decl.body {
        if let emath_core::tree::StmtKind::Section(section) = &stmt.kind {
            *counts.entry(section.name.clone()).or_insert(0) += 1;
        }
    }
    for rule in &def.schema {
        match rule {
            SchemaRule::RequireSection(name) if !counts.contains_key(name) => diagnostics.error(
                "E-KIND-003",
                format!(
                    "kind `{}` requires section `{name}`; application `{}` lacks it",
                    def.name, decl.name
                ),
                decl.head_source,
            ),
            SchemaRule::RequireExactlyOneSection(name)
                if counts.get(name).copied().unwrap_or(0) != 1 =>
            {
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
            SchemaRule::RequireSection(_)
            | SchemaRule::RequireExactlyOneSection(_)
            | SchemaRule::AllowSection(_) => {}
        }
    }
}

/// Admit an exported field-pack descriptor. Packs are data and never enter the
/// runnable declaration arena.
pub(crate) fn admit_field_pack(
    decl: &emath_core::tree::Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    let Some(rules) = section_rules("field_pack") else {
        diagnostics.error(
            "E-KIND-001",
            "field_pack section rules are missing",
            decl.head_source,
        );
        return;
    };
    let errors_before = diagnostics.errors().count();
    validate_body(decl, &rules, diagnostics, trace);
    if diagnostics.errors().count() > errors_before {
        return;
    }
    let mut exports = Vec::new();
    for statement in &decl.body {
        let emath_core::tree::StmtKind::Section(section) = &statement.kind else {
            continue;
        };
        if section.name != "exports" {
            continue;
        }
        for nested in &section.suite.statements {
            if let emath_core::tree::StmtKind::Command { head, .. } = &nested.kind {
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
        format!(
            "pack `{}` admitted with {export_count} export(s)",
            decl.name
        ),
        Some(decl.head_source),
    );
}

fn admit_extern(
    decl: &emath_core::tree::Declaration,
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
        diagnostics.error(
            "E-TYPE-112",
            "generic `extern operator` declarations are outside the Phase 1 strict subset",
            decl.head_source,
        );
        return;
    }
    if decl.signature.is_none() {
        diagnostics.error(
            "E-SYN-101",
            "extern operator requires a parameter list and result type",
            decl.head_source,
        );
        return;
    }
    let rules = section_rules("extern").unwrap_or_default();
    let errors_before = diagnostics.errors().count();
    validate_body(decl, &rules, diagnostics, trace);
    if diagnostics.errors().count() > errors_before {
        return;
    }
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

pub(super) fn package_entry(decl: &emath_core::tree::Declaration, kind: &str) -> emath_ir::Declaration {
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

fn validate_body(
    decl: &emath_core::tree::Declaration,
    rules: &[SectionRule],
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    for stmt in &decl.body {
        match &stmt.kind {
            emath_core::tree::StmtKind::Section(section) => {
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
            emath_core::tree::StmtKind::FnDecl {
                head, name, suite, ..
            } if rules
                .iter()
                .any(|rule| rule.fn_heads.contains(&head.as_str())) =>
            {
                admit_fn_head(head, name, suite.as_ref(), decl, diagnostics, trace);
            }
            emath_core::tree::StmtKind::Command { head, .. }
                if body_command_allowed(decl, head) =>
            {
                trace.record(
                    "recognize:body",
                    format!("command {} ({})", head.join(" "), decl.item_kind),
                    Some(stmt.source),
                );
            }
            _ => diagnostics.error(
                "E-SYN-101",
                format!(
                    "statement at declaration level is not admitted for kind `{}`",
                    decl.item_kind
                ),
                stmt.source,
            ),
        }
    }
}
