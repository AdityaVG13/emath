//! Recognition-level admission (corpus front-end).
//!
//! Package identity, `use` imports, declaration-kind admission, the
//! custom-kind schema registry, and per-kind structural validation.
//! Out-of-subset constructs get a typed refusal; body expressions are
//! validated structurally (typed SIR lowering is the intent-compiler lane).

use crate::admit::SemanticTrace;
use emath_core::Diagnostics;
use emath_core::tree::{Declaration, Expr, ExprKind, Item, Stmt, StmtKind, TypeKind, UseTree};
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
        // The definition itself is a registry entry; validate it but record
        // no package declaration.
        validate_body(decl, &rules, diagnostics, trace);
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
    let rules = sections_for_application(def);
    validate_body(decl, &rules, diagnostics, trace);
    enforce_schema(decl, def, diagnostics);
    if def.name == "family" {
        if !diagnostics.has_errors() {
            expand_family(decl, package, diagnostics, trace);
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

/// Kind applications inherit the schema of their kind definition, with the
/// full section vocabulary available for the schema to allow.
fn sections_for_application(def: &KindDef) -> Vec<SectionRule> {
    let mut rules = function_sections_for_application();
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
        let rule = match name.as_str() {
            "equation" => sec("equation", EQUATION_STMTS),
            "constraint" => sec("constraint", CONSTRAINT_STMTS),
            "evidence" => sec("evidence", EVIDENCE_STMTS),
            "witness" => sec("witness", EVIDENCE_STMTS),
            "invariant" => sec("invariant", EXPR_STMTS),
            "protect" => sec("protect", EXPR_STMTS),
            "host" => cmd_sec("host", &["rust"]),
            "structure" | "finite" | "mapping" => SectionRule {
                name: name.clone(),
                generics: None,
                statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
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
