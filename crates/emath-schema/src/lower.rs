//! Restricted lowering language.
//!
//! Bounded, typed ops over the kind schema; applications are validated
//! before publishing HIR (`E-KIND-020`/`021`/`022`) with the trace kept.

use emath_ir::kind_schema::KindSchema;

/// Maximum expansion steps a lowered schema may produce.
pub const MAX_LOWER_OPS: usize = 64;

/// One restricted lowering op.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerOp {
    /// Move `from`'s payload into a core section; no re-hoisting, no cycles.
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

/// Result of applying a program: lowered schema, identity, expansion trace.
#[derive(Clone, Debug)]
pub struct LoweringReport {
    pub schema: KindSchema,
    pub identity: String,
    pub trace: Vec<TraceStep>,
}

/// Applies a bounded lowering program; on the first refusal the schema
/// is left untouched and the issues are returned.
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

/// Validates a lowered schema: every section must be core or carry an
/// admission alias recorded by a program op, else `E-KIND-021`.
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
        let Some(existing) = self.section(from).cloned() else {
            return;
        };
        self.remove_section(from);
        self.insert_section(to, existing);
        // Migrate admission/bind provenance with the section. A bare
        // `admission.{to}=from` would orphan prior hoist aliases under
        // `admission.{from}` and leave `bind.{from}` pointing at a removed
        // section — both corrupt identity and `is_bound` after rename.
        let old_admission = format!("admission.{from}");
        let new_admission = format!("admission.{to}");
        let mut aliases = Vec::new();
        if let Some(prev) = self.default_for(&old_admission) {
            for part in prev.split(',').filter(|part| !part.is_empty()) {
                if !aliases.iter().any(|alias| alias == part) {
                    aliases.push(part.to_string());
                }
            }
        }
        if !aliases.iter().any(|alias| alias == from) {
            aliases.push(from.to_string());
        }
        if let Some(existing_to) = self.default_for(&new_admission) {
            for part in existing_to.split(',').filter(|part| !part.is_empty()) {
                if !aliases.iter().any(|alias| alias == part) {
                    aliases.push(part.to_string());
                }
            }
        }
        self.remove_default(&old_admission);
        self.insert_default(new_admission, aliases.join(","));

        let old_bind = format!("bind.{from}");
        let new_bind = format!("bind.{to}");
        if let Some(symbol) = self.default_for(&old_bind).map(str::to_string) {
            self.remove_default(&old_bind);
            if self.default_for(&new_bind).is_none() {
                self.insert_default(new_bind, symbol);
            }
        }
    }

    fn bind_section(&mut self, section: &str, symbol: &str) {
        self.insert_default(format!("bind.{section}"), symbol.to_string());
    }
}

/// Whether a section was bound via `Bind` during lowering.
#[must_use]
pub fn is_bound(schema: &KindSchema, section: &str) -> bool {
    schema
        .default_for(&format!("bind.{section}"))
        .is_some_and(|symbol| !symbol.is_empty())
}
