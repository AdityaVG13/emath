//! Schema registry — section rules per declaration kind.

// ---- custom-kind schema rules -------------------------------------------

/// Custom-kind schema rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaRule {
    /// `require section <name>`: the application must declare the section.
    RequireSection(String),
    /// `allow section <name>`: the application may declare the section.
    AllowSection(String),
    /// `require exactly_one <name>`: exactly one such section.
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

pub(super) const FIELD_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Fields];
pub(super) const ASSIGN_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Assigns];
pub(super) const EQUATION_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Equations];
pub(super) const EXPR_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Exprs];
pub(super) const GOAL_FIRST_WORDS: &[&str] = &[
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
pub(super) const VARIANT_FIRST_WORDS: &[&str] = &[
    "implements",
    "when",
    "define",
    "semantics",
    "approximation",
    "error",
];
pub(super) const FALLBACK_FIRST_WORDS: &[&str] = &[
    "host",
    "fallback",
    "continuation",
    "strict",
    "generate",
    "unresolved",
];
pub(super) const PROFILE_FIRST_WORDS: &[&str] = &["prefer", "fallback"];
pub(super) const DISPATCH_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny];
pub(super) const EVIDENCE_STMTS: &[StmtShapeKind] = &[
    StmtShapeKind::Requires,
    StmtShapeKind::Exprs,
    StmtShapeKind::CommandsAny,
];
pub(super) const CONSTRUCTOR_STMTS: &[StmtShapeKind] = &[
    StmtShapeKind::Requires,
    StmtShapeKind::Exprs,
    StmtShapeKind::CommandsAny,
];
pub(super) const CONSTRAINT_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Exprs, StmtShapeKind::Equations];
pub(super) const SCHEMA_STMTS: &[StmtShapeKind] = &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny];
pub(super) const GENERATE_FIRST_WORDS: &[&str] = &[
    "algebraic_rewrites",
    "providers",
    "precision",
    "approximations",
    "specialization",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StmtShapeKind {
    Fields,
    Assigns,
    Equations,
    Exprs,
    Requires,
    CommandsAny,
}

/// Statement-shape access shared by section rules and nested rules.
pub(super) trait ShapeRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind];
    fn command_first_words(&self) -> &'static [&'static str];
}

/// One section rule for a declaration kind.
pub(super) struct SectionRule {
    pub(super) name: String,
    /// `None` = any generic allowed; `Some(&[])` = no generic allowed.
    pub(super) generics: Option<&'static [&'static str]>,
    pub(super) statement_shapes: &'static [StmtShapeKind],
    /// First words allowed for command statements (empty = any).
    pub(super) command_first_words: &'static [&'static str],
    /// Fn-like head statements allowed in this section (e.g. `constructor`).
    pub(super) fn_heads: &'static [&'static str],
    /// Nested section rules (name → statement shapes).
    pub(super) nested: &'static [NestedRule],
}

impl ShapeRule for SectionRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind] {
        self.statement_shapes
    }
    fn command_first_words(&self) -> &'static [&'static str] {
        self.command_first_words
    }
}

pub(super) struct NestedRule {
    pub(super) name: &'static str,
    pub(super) statement_shapes: &'static [StmtShapeKind],
    pub(super) command_first_words: &'static [&'static str],
}

impl ShapeRule for NestedRule {
    fn statement_shapes(&self) -> &'static [StmtShapeKind] {
        self.statement_shapes
    }
    fn command_first_words(&self) -> &'static [&'static str] {
        self.command_first_words
    }
}

pub(super) fn section_rules(kind: &str) -> Option<Vec<SectionRule>> {
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
                name: "host".to_string(),
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
                name: "tune".to_string(),
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
                name: "host".to_string(),
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
                name: "lower".to_string(),
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

pub(super) fn sec(name: &'static str, shapes: &'static [StmtShapeKind]) -> SectionRule {
    SectionRule {
        name: name.to_string(),
        generics: Some(&[]),
        statement_shapes: shapes,
        command_first_words: &[],
        fn_heads: &[],
        nested: &[],
    }
}

pub(super) fn cmd_sec(name: &'static str, first_words: &'static [&'static str]) -> SectionRule {
    SectionRule {
        name: name.to_string(),
        generics: Some(&[]),
        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
        command_first_words: first_words,
        fn_heads: &[],
        nested: &[],
    }
}

pub(super) fn cmds_sec(name: &'static str, first_words: &'static [&'static str]) -> SectionRule {
    SectionRule {
        name: name.to_string(),
        generics: None,
        statement_shapes: &[StmtShapeKind::CommandsAny, StmtShapeKind::Requires],
        command_first_words: first_words,
        fn_heads: &[],
        nested: &[],
    }
}

pub(super) fn ctor_sec() -> SectionRule {
    SectionRule {
        name: "constructor".to_string(),
        generics: Some(&[]),
        statement_shapes: CONSTRUCTOR_STMTS,
        command_first_words: &[],
        fn_heads: &["constructor"],
        nested: &[],
    }
}

pub(super) fn define_sec() -> SectionRule {
    SectionRule {
        name: "define".to_string(),
        generics: Some(&[]),
        statement_shapes: ASSIGN_STMTS,
        command_first_words: &[],
        fn_heads: &["define"],
        nested: &[],
    }
}

pub(super) fn goal_sec() -> SectionRule {
    SectionRule {
        name: "goal".to_string(),
        generics: None,
        statement_shapes: &[StmtShapeKind::Requires, StmtShapeKind::CommandsAny],
        command_first_words: GOAL_FIRST_WORDS,
        fn_heads: &[],
        nested: &[],
    }
}
