//! Recognition-level admission (corpus front-end).
//!
//! Implements the semantic doors that the corpus harness reaches after
//! the parser lane: package identity, `use` imports, declaration-kind
//! admission (`function`, `record`, `policy`, `model`, `kind`, `search`,
//! `experiment`, `type`, `extern operator`, and kind applications), the
//! custom-kind schema registry, and per-kind structural validation of
//! sections and statements.
//!
//! Every recognized construct is recorded (package metadata, import
//! entries, trace entries per statement). Anything outside the recognized
//! The surface receives a typed refusal; nothing is silently dropped.
//!
//! Scope note: body expressions are validated structurally and recorded as
//! canonical trace text; typed SIR lowering (typed expression arenas,
//! constructor/dependency records) is the intent-compiler lane and stays
//! open there.

use crate::admit::SemanticTrace;
use emath_core::Diagnostics;
use emath_ir::{ImportEntry, ImportSelection};
use emath_syntax::tree::{
    Declaration, Expr, ExprKind, Item, Stmt, StmtKind, TypeExpr, TypeKind, UseTree,
};
use std::collections::BTreeMap;

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

/// Custom-kind schema rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaRule {
    /// `require section <name>` — the application must declare the section.
    RequireSection(String),
    /// `allow section <name>` — the application may declare the section.
    AllowSection(String),
    /// `require exactly_one <name>` — exactly one such section.
    RequireExactlyOneSection(String),
}

/// A declared `emath kind` definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindDef {
    /// Kind name.
    pub name: String,
    /// Optional `extends <parent>` target.
    pub extends: Option<String>,
    /// Schema rules in declaration order.
    pub schema: Vec<SchemaRule>,
}

// ---- per-kind statement-shape rules --------------------------------------

const FIELD_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Fields];
const ASSIGN_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Assigns];
const EQUATION_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Equations];
const EXPR_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Exprs];
const GOAL_FIRST_WORDS: &[&str] = &[
    "compile",
    "profile",
    "differentiate",
    "target",
    "simulate",
    "observe",
    "linearize",
    "solve",
    "using",
    "require",
    "search",
    "budget",
    "continuation",
];
const VARIANT_FIRST_WORDS: &[&str] = &[
    "implements",
    "when",
    "define",
    "semantics",
    "approximation",
    "error",
];
const FALLBACK_FIRST_WORDS: &[&str] = &[
    "host",
    "fallback",
    "continuation",
    "strict",
    "generate",
    "unresolved",
];
const PROFILE_FIRST_WORDS: &[&str] = &["prefer", "fallback"];
const DISPATCH_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny];
const EVIDENCE_STMTS: &[StmtShapeKind] = &[
    StmtShapeKind::Requires,
    StmtShapeKind::Exprs,
    StmtShapeKind::CommandsAny,
];
const CONSTRUCTOR_STMTS: &[StmtShapeKind] = &[
    StmtShapeKind::Requires,
    StmtShapeKind::Exprs,
    StmtShapeKind::CommandsAny,
];
const CONSTRAINT_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Exprs, StmtShapeKind::Equations];
const SCHEMA_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny];
const GENERATE_FIRST_WORDS: &[&str] = &[
    "algebraic_rewrites",
    "providers",
    "precision",
    "approximations",
    "specialization",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StmtShapeKind {
    Fields,
    Assigns,
    Equations,
    Exprs,
    Requires,
    CommandsAny,
}

/// Statement-shape access shared by section rules and nested rules.
trait ShapeRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind];
    fn command_first_words(&self) -> &'static [&'static str];
}

/// One section rule for a declaration kind.
struct SectionRule {
    name: &'static str,
    /// `None` = any generic allowed; `Some(&[])` = no generic allowed.
    generics: Option<&'static [&'static str]>,
    statement_shapes: &'static [StmtShapeKind],
    /// First words allowed for command statements (empty = any).
    command_first_words: &'static [&'static str],
    /// Fn-like head statements allowed in this section (e.g. `constructor`).
    fn_heads: &'static [&'static str],
    /// Nested section rules (name → statement shapes).
    nested: &'static [NestedRule],
}

impl ShapeRule for SectionRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind] {
        self.statement_shapes
    }
    fn command_first_words(&self) -> &'static [&'static str] {
        self.command_first_words
    }
}

struct NestedRule {
    name: &'static str,
    statement_shapes: &'static [StmtShapeKind],
    command_first_words: &'static [&'static str],
}

impl ShapeRule for NestedRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind] {
        self.statement_shapes
    }
    fn command_first_words(&self) -> &'static [&'static str] {
        self.command_first_words
    }
}

fn section_rules(kind: &str) -> Option<Vec<SectionRule>> {
    Some(match kind {
        "function" => vec![
            sec("input", FIELD_STMTS),
            sec("output", FIELD_STMTS),
            sec("parameter", FIELD_STMTS),
            sec("define", ASSIGN_STMTS),
            ctor_sec(),
            goal_sec(),
            sec("evidence", EVIDENCE_STMTS),
            cmd_sec("export", &["rust"]),
            cmds_sec("variant", VARIANT_FIRST_WORDS),
            sec("dispatch", DISPATCH_STMTS),
            cmd_sec("fallback", FALLBACK_FIRST_WORDS),
        ],
        "record" => vec![sec("state", FIELD_STMTS), ctor_sec()],
        "policy" => vec![
            sec("input", FIELD_STMTS),
            sec("output", FIELD_STMTS),
            sec("state", FIELD_STMTS),
            sec("invariant", EXPR_STMTS),
            ctor_sec(),
            define_sec(),
            goal_sec(),
            sec("evidence", EVIDENCE_STMTS),
            SectionRule {
                name: "host",
                generics: Some(&["rust"]),
                statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                command_first_words: &["host", "rust", "package", "baseline", "candidate"],
                fn_heads: &[],
                nested: &[NestedRule {
                    name: "implement",
                    statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                    command_first_words: &["method"],
                }],
            },
            cmd_sec("fallback", FALLBACK_FIRST_WORDS),
            SectionRule {
                name: "tune",
                generics: None,
                statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                command_first_words: &["baseline"],
                fn_heads: &[],
                nested: &[
                    NestedRule {
                        name: "generate",
                        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                        command_first_words: GENERATE_FIRST_WORDS,
                    },
                    NestedRule {
                        name: "objective",
                        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                        command_first_words: &["minimize", "maximize"],
                    },
                    NestedRule {
                        name: "protect",
                        statement_shapes: EXPR_STMTS,
                        command_first_words: &[],
                    },
                    NestedRule {
                        name: "promotion",
                        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                        command_first_words: &["shadow", "fallback"],
                    },
                ],
            },
        ],
        "model" => vec![
            sec("parameter", FIELD_STMTS),
            sec("state", FIELD_STMTS),
            ctor_sec(),
            sec("equation", EQUATION_STMTS),
            goal_sec(),
            sec("evidence", EVIDENCE_STMTS),
            cmd_sec("profile", PROFILE_FIRST_WORDS),
            sec("input", FIELD_STMTS),
            sec("output", FIELD_STMTS),
            sec("constraint", CONSTRAINT_STMTS),
            cmd_sec("fallback", FALLBACK_FIRST_WORDS),
        ],
        "search" => vec![
            sec("input", FIELD_STMTS),
            sec("witness", FIELD_STMTS),
            sec("constraint", CONSTRAINT_STMTS),
            goal_sec(),
            sec("evidence", EVIDENCE_STMTS),
            cmd_sec("fallback", FALLBACK_FIRST_WORDS),
        ],
        "experiment" => vec![
            cmd_sec("subject", &[]),
            SectionRule {
                name: "host",
                generics: Some(&[]),
                statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
                command_first_words: &["rust", "baseline", "candidate"],
                fn_heads: &[],
                nested: &[],
            },
            cmd_sec("workload", &["dataset", "warmup", "measure"]),
            cmd_sec("metrics", &["minimize", "maximize", "report"]),
            sec("protect", EXPR_STMTS),
            cmd_sec("decision", &["reject", "shadow", "promote", "rollback"]),
        ],
        "kind" => vec![
            sec("schema", SCHEMA_STMTS),
            SectionRule {
                name: "lower",
                generics: None,
                statement_shapes: ASSIGN_STMTS,
                command_first_words: &[],
                fn_heads: &[],
                nested: &[NestedRule {
                    name: "",
                    statement_shapes: ASSIGN_STMTS,
                    command_first_words: &[],
                }],
            },
        ],
        "extern" => vec![cmd_sec("semantics", &["symmetric", "zero_on_identity"])],
        // `type` declarations carry a body `representation <type>` command
        // and no sections.
        "type" => Vec::new(),
        _ => return None,
    })
}

fn sec(name: &'static str, shapes: &'static [StmtShapeKind]) -> SectionRule {
    SectionRule {
        name,
        generics: Some(&[]),
        statement_shapes: shapes,
        command_first_words: &[],
        fn_heads: &[],
        nested: &[],
    }
}

fn cmd_sec(name: &'static str, first_words: &'static [&'static str]) -> SectionRule {
    SectionRule {
        name,
        generics: Some(&[]),
        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
        command_first_words: first_words,
        fn_heads: &[],
        nested: &[],
    }
}

fn cmds_sec(name: &'static str, first_words: &'static [&'static str]) -> SectionRule {
    SectionRule {
        name,
        generics: None,
        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
        command_first_words: first_words,
        fn_heads: &[],
        nested: &[],
    }
}

fn ctor_sec() -> SectionRule {
    SectionRule {
        name: "constructor",
        generics: Some(&[]),
        statement_shapes: CONSTRUCTOR_STMTS,
        command_first_words: &[],
        fn_heads: &["constructor"],
        nested: &[],
    }
}

fn define_sec() -> SectionRule {
    SectionRule {
        name: "define",
        generics: Some(&[]),
        statement_shapes: ASSIGN_STMTS,
        command_first_words: &[],
        fn_heads: &["define"],
        nested: &[],
    }
}

fn goal_sec() -> SectionRule {
    SectionRule {
        name: "goal",
        generics: None,
        statement_shapes: &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny],
        command_first_words: GOAL_FIRST_WORDS,
        fn_heads: &[],
        nested: &[],
    }
}

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
pub fn collect_kind_defs(tree: &emath_syntax::tree::SyntaxTree) -> BTreeMap<String, KindDef> {
    let mut defs = BTreeMap::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.item_kind != "kind" {
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

fn schema_rules_from_section(section: &emath_syntax::tree::Section) -> Vec<SchemaRule> {
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
    tree: &emath_syntax::tree::SyntaxTree,
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
                let mut path = path.clone();
                let selection = match tree {
                    UseTree::All => ImportSelection::All,
                    UseTree::Named(names) if names.is_empty() && path.len() >= 2 => {
                        // `use std.numeric.Real` — the parser keeps the
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
        }
    }
    result
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
    if let Some(def) = kind_defs.get(item_kind) {
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
    let mut declaration = package_entry(decl, &def.name);
    let rules = sections_for_application(def);
    validate_body(decl, &rules, diagnostics, trace);
    enforce_schema(decl, def, diagnostics);
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

/// Kind applications inherit the schema of their kind definition, with the
/// full section vocabulary available for the schema to allow.
fn sections_for_application(def: &KindDef) -> Vec<SectionRule> {
    let _ = def;
    function_sections_for_application()
}

fn function_sections_for_application() -> Vec<SectionRule> {
    // The application vocabulary mirrors the function/record/policy surface;
    // the kind's schema rules decide what is required/forbidden.
    let mut rules = section_rules("function").unwrap_or_default();
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
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: emath_ir::goal::CompileSpec::default(),
        source: decl.source,
    }
}

/// Validate a declaration body against its kind's section rules, emitting
/// trace entries for every recognized statement.
///
/// Fields, definitions, and constructors are recorded as trace entries;
/// typed SIR lowering is the intent-compiler lane.
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

fn format_section_trace(section: &emath_syntax::tree::Section) -> String {
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
    section: &emath_syntax::tree::Section,
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
    section: Option<&emath_syntax::tree::Section>,
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
            trace.record(
                "recognize:field",
                format!("{}: {}", name, type_text(ty)),
                Some(stmt.source),
            );
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
    suite: Option<&emath_syntax::tree::Suite>,
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

fn place_text(place: &emath_syntax::tree::Place) -> String {
    place.segments.join(".")
}

fn argument_text(argument: &emath_syntax::tree::CommandArgument) -> String {
    match argument {
        emath_syntax::tree::CommandArgument::Expr(expr) => expr_text(expr),
        emath_syntax::tree::CommandArgument::Assignment { name, value } => {
            format!("{name} = {}", expr_text(value))
        }
        emath_syntax::tree::CommandArgument::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Compact deterministic rendering of an expression for trace text.
#[must_use]
pub fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(value) | ExprKind::Float(value) | ExprKind::Str(value) => value.clone(),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Path { segments, generics } => {
            let mut out = segments.join("::");
            if let Some(generics) = generics {
                out.push('<');
                out.push_str(
                    &generics
                        .iter()
                        .map(type_text)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('>');
            }
            out
        }
        ExprKind::Call { function, args } => format!(
            "{}({})",
            expr_text(function),
            args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Index { value, indices } => format!(
            "{}[{}]",
            expr_text(value),
            indices.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Unary { op, value } => format!("{op:?}({})", expr_text(value)),
        ExprKind::Binary { op, left, right } => {
            format!("({} {op:?} {})", expr_text(left), expr_text(right))
        }
        ExprKind::Quantity { value, unit } => {
            format!("{} {}", expr_text(value), unit.join(" "))
        }
        ExprKind::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Tuple(items) => format!(
            "({})",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => format!(
            "{}..{}{}",
            start.as_ref().map_or_else(String::new, |e| expr_text(e)),
            if *inclusive { "=" } else { "" },
            end.as_ref().map_or_else(String::new, |e| expr_text(e)),
        ),
        ExprKind::Derivative { value, wrt } => {
            let wrt_text = wrt.as_ref().map_or_else(String::new, |w| {
                format!(
                    " wrt {}",
                    w.iter().map(expr_text).collect::<Vec<_>>().join(", ")
                )
            });
            format!("derivative({}){}", expr_text(value), wrt_text)
        }
        ExprKind::At { value, location } => {
            format!("{} at {}", expr_text(value), expr_text(location))
        }
        ExprKind::On { value, location } => {
            format!("{} on {}", expr_text(value), expr_text(location))
        }
        ExprKind::Conditioned { value, condition } => {
            format!("{} if {}", expr_text(value), expr_text(condition))
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "if {} then {} else {}",
            expr_text(condition),
            expr_text(then_value),
            expr_text(else_value)
        ),
        ExprKind::Binder { kind, .. } => format!("binder({kind:?})"),
    }
}

/// Compact deterministic rendering of a type for trace text.
#[must_use]
pub fn type_text(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => {
            let mut out = segments.join("::");
            if !generic_args.is_empty() {
                out.push('<');
                out.push_str(
                    &generic_args
                        .iter()
                        .map(type_text)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('>');
            }
            out
        }
        TypeKind::List(items) => format!(
            "[{}]",
            items.iter().map(type_text).collect::<Vec<_>>().join(", ")
        ),
        TypeKind::Tuple(items) => format!(
            "({})",
            items.iter().map(type_text).collect::<Vec<_>>().join(", ")
        ),
        TypeKind::Ref(inner) => format!("&{}", type_text(inner)),
        TypeKind::Product(items) => items.iter().map(type_text).collect::<Vec<_>>().join(" * "),
    }
}
