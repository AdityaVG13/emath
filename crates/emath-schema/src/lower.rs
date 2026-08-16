//!: restricted lowering language.
//!
//! Lowering is bounded and typed: a fixed set of ops transformed over
//! the kind schema, never arbitrary code. Every application is checked
//! before it publishes HIR (`E-KIND-020` invalid op, `E-KIND-021`
//! unknown core section, `E-KIND-022` bound exceeded), and the
//! expansion trace is retained.

use emath_ir::kind_schema::KindSchema;

/// Maximum number of (synthetic) expansion steps a lowered schema may
/// produce from a bounded program.
pub const MAX_LOWER_OPS: usize = 64;

/// One restricted lowering op.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerOp {
    /// Move the payload of `from` into the core section `into`; `into`
    /// must exist in the core schema and must not already receive a
    /// hoist (no re-hoisting, no cycles).
    Hoist { from: String, into: String },
    /// Rename an existing section; target must be free.
    Rename { from: String, to: String },
    /// Bind a section's generic header (`<name>`) to a declared symbol.
    Bind { section: String, to: String },
}

/// One lowering refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringIssue {
    pub code: &'static str,
    pub op: usize,
    pub detail: String,
}

/// One applied lowering step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    pub op: usize,
    pub summary: String,
}

/// Result of applying a program: the lowered schema, the canonical
/// identity, and the expansion trace.
#[derive(Clone, Debug)]
pub struct LoweringReport {
    pub schema: KindSchema,
    pub identity: String,
    pub trace: Vec<TraceStep>,
}

/// Applies a bounded lowering program to a core schema. Ops are
/// validated before the schema is mutated; on the first refusal the
/// schema is left untouched and the issues are returned.
pub fn apply_lowering(
    core: &KindSchema,
    program: &[LowerOp],
) -> Result<LoweringReport, Vec<LoweringIssue>> {
    let mut issues = Vec::new();
    if program.len() > MAX_LOWER_OPS {
        issues.push(LoweringIssue {
            code: "E-KIND-022",
            op: program.len(),
            detail: format!("lowering program exceeds {MAX_LOWER_OPS} ops"),
        });
        return Err(issues);
    }
    let mut schema = core.clone();
    let mut trace = Vec::new();
    let mut hoisted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for (index, op) in program.iter().enumerate() {
        match op {
            LowerOp::Hoist { from, into } => {
                if schema.section(into).is_none() {
                    issues.push(LoweringIssue {
                        code: "E-KIND-021",
                        op: index,
                        detail: format!(
                            "hoist target `{into}` is not a section of the core schema"
                        ),
                    });
                    return Err(issues);
                }
                if !hoisted.insert(into.clone()) {
                    issues.push(LoweringIssue {
                        code: "E-KIND-022",
                        op: index,
                        detail: format!("recursive hoist into `{into}`"),
                    });
                    return Err(issues);
                }
                schema.set_admission_alias(into, from);
                trace.push(TraceStep {
                    op: index,
                    summary: format!("hoist `{from}` -> `{into}`"),
                });
            }
            LowerOp::Rename { from, to } => {
                if schema.section(from).is_none() {
                    issues.push(LoweringIssue {
                        code: "E-KIND-021",
                        op: index,
                        detail: format!("rename source `{from}` is not a declared section"),
                    });
                    return Err(issues);
                }
                if schema.section(to).is_some() {
                    issues.push(LoweringIssue {
                        code: "E-KIND-021",
                        op: index,
                        detail: format!("rename target `{to}` already exists"),
                    });
                    return Err(issues);
                }
                schema.rename_section(from, to);
                trace.push(TraceStep {
                    op: index,
                    summary: format!("rename `{from}` -> `{to}`"),
                });
            }
            LowerOp::Bind { section, to } => {
                if schema.section(section).is_none() {
                    issues.push(LoweringIssue {
                        code: "E-KIND-021",
                        op: index,
                        detail: format!("bind target `{section}` is not a declared section"),
                    });
                    return Err(issues);
                }
                if to.trim().is_empty() {
                    issues.push(LoweringIssue {
                        code: "E-KIND-020",
                        op: index,
                        detail: "bind symbol cannot be empty".into(),
                    });
                    return Err(issues);
                }
                schema.bind_section(section, to);
                trace.push(TraceStep {
                    op: index,
                    summary: format!("bind `{section}` to `{to}`"),
                });
            }
        }
    }
    let identity = schema.canonical();
    Ok(LoweringReport {
        schema,
        identity,
        trace,
    })
}

/// Validates a lowered schema against the core: every section in the
/// lowered schema must be a core section or carry an admission alias
/// recorded by a program op (rename/hoist provenance), else
/// `E-KIND-021`. Invalid lowering cannot publish HIR.
#[must_use]
pub fn validate_lowered(lowered: &KindSchema, core: &KindSchema) -> Vec<LoweringIssue> {
    let mut issues = Vec::new();
    for (name, _) in lowered.sections() {
        if core.section(name).is_some() {
            continue;
        }
        let provenanced = lowered
            .default_for(&format!("admission.{name}"))
            .is_some_and(|alias| !alias.is_empty());
        if !provenanced {
            issues.push(LoweringIssue {
                code: "E-KIND-021",
                op: usize::MAX,
                detail: format!(
                    "lowered schema publishes section `{name}` outside the core schema"
                ),
            });
        }
    }
    issues
}

/// `KindSchema` extensions used by the restricted lowerer.
trait LowerExt {
    fn set_admission_alias(&mut self, canonical: &str, alias: &str);
    fn rename_section(&mut self, from: &str, to: &str);
    fn bind_section(&mut self, section: &str, symbol: &str);
}

impl LowerExt for KindSchema {
    fn set_admission_alias(&mut self, canonical: &str, alias: &str) {
        // Canonical section names win; aliases are recorded as a
        // comma-joined admission list in the predicate area of the
        // schema (deterministic, identity-bearing).
        let key = format!("admission.{canonical}");
        let existing = self.default_for(&key).unwrap_or_default().to_string();
        let mut names: Vec<String> = existing.split(',').map(str::to_string).collect();
        if !names.iter().any(|name| name == alias) {
            names.push(alias.to_string());
        }
        self.insert_default(key, names.join(","));
    }

    fn rename_section(&mut self, from: &str, to: &str) {
        let existing = self
            .section(from)
            .cloned()
            .expect("rename source checked before mutation");
        self.remove_section(from);
        self.insert_section(to, existing);
        // Keep provenance: the new name admits the old one.
        self.insert_default(format!("admission.{to}"), from.to_string());
    }

    fn bind_section(&mut self, section: &str, symbol: &str) {
        self.insert_default(format!("bind.{section}"), symbol.to_string());
    }
}

/// Whether a symbol is bound to a section via `Bind` during lowering.
#[must_use]
pub fn is_bound(schema: &KindSchema, section: &str) -> bool {
    schema
        .default_for(&format!("bind.{section}"))
        .is_some_and(|symbol| !symbol.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::kind_schema::core_function_schema;

    #[test]
    fn hoist_into_core_section_changes_identity_and_traces() {
        let core = core_function_schema();
        let program = vec![
            LowerOp::Bind {
                section: "compile".into(),
                to: "strict-emath".into(),
            },
            LowerOp::Hoist {
                from: "experiments".into(),
                into: "tests".into(),
            },
        ];
        let report = apply_lowering(&core, &program).unwrap();
        assert!(report.identity.contains("bind.compile=strict-emath"));
        assert_eq!(report.trace.len(), 2);
        assert!(is_bound(&report.schema, "compile"));
        assert!(validate_lowered(&report.schema, &core).is_empty());
    }

    #[test]
    fn hoist_into_unknown_section_is_refused() {
        let core = core_function_schema();
        let program = vec![LowerOp::Hoist {
            from: "x".into(),
            into: "mystery".into(),
        }];
        let issues = apply_lowering(&core, &program).unwrap_err();
        assert_eq!(issues[0].code, "E-KIND-021");
    }

    #[test]
    fn recursive_hoist_is_refused_as_unbounded() {
        let core = core_function_schema();
        let program = vec![
            LowerOp::Hoist {
                from: "a".into(),
                into: "tests".into(),
            },
            LowerOp::Hoist {
                from: "b".into(),
                into: "tests".into(),
            },
        ];
        let issues = apply_lowering(&core, &program).unwrap_err();
        assert_eq!(issues[0].code, "E-KIND-022");
    }

    #[test]
    fn oversized_program_is_refused() {
        let core = core_function_schema();
        let mut program = Vec::new();
        for i in 0..=MAX_LOWER_OPS {
            program.push(LowerOp::Bind {
                section: "compile".into(),
                to: format!("x{i}"),
            });
        }
        let issues = apply_lowering(&core, &program).unwrap_err();
        assert_eq!(issues[0].code, "E-KIND-022");
    }

    #[test]
    fn rename_moves_a_section_and_keeps_payload_policy() {
        let core = core_function_schema();
        let before = core.section("tests").cloned().expect("core has tests");
        let report = apply_lowering(
            &core,
            &[LowerOp::Rename {
                from: "tests".into(),
                to: "verification".into(),
            }],
        )
        .unwrap();
        assert!(report.schema.section("tests").is_none());
        let renamed = report
            .schema
            .section("verification")
            .expect("renamed section");
        assert_eq!(renamed.repeat, before.repeat);
        assert_eq!(renamed.payload, before.payload);
        assert_ne!(report.identity, core.canonical());
        // Renames keep provenance: the lowered schema validates.
        assert!(validate_lowered(&report.schema, &core).is_empty());
    }

    #[test]
    fn bind_with_empty_symbol_is_refused() {
        let core = core_function_schema();
        let issues = apply_lowering(
            &core,
            &[LowerOp::Bind {
                section: "compile".into(),
                to: "   ".into(),
            }],
        )
        .unwrap_err();
        assert_eq!(issues[0].code, "E-KIND-020");
    }
}
