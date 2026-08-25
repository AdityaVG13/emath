//! Open declaration framework.
//!
//! `Hir` collects section families, attributes, generics, documentation
//! and extension payloads with provenance. Section admission follows the
//! kind schema: repeat policies enforced, unknown sections refused.

use std::collections::BTreeMap;

use emath_core::tree::{Declaration, GenericParam, Section};
use emath_core::{Diagnostics, Span};
use emath_ir::kind_schema::{KindSchema, RepeatPolicy};

/// Section family classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SectionFamily {
    /// Structural data (`inputs`, `outputs`, `state`, `components`).
    Data,
    /// Behavior (`definitions`, `equations`, `invariants`).
    Behavior,
    /// Construction (`constructors`, `factories`).
    Construction,
    /// Goals (`goals`, `exports`).
    Goals,
    /// Evidence, tests and benchmarks.
    Evidence,
    /// Configuration (`compile`, `policy`).
    Config,
    /// Extension payload.
    Extension,
}

impl SectionFamily {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Behavior => "behavior",
            Self::Construction => "construction",
            Self::Goals => "goals",
            Self::Evidence => "evidence",
            Self::Config => "config",
            Self::Extension => "extension",
        }
    }
}

/// Where a section belongs in the declaration hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hierarchy {
    Pre,
    Body,
    Post,
}

impl Hierarchy {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Body => "body",
            Self::Post => "post",
        }
    }
}

/// Spread of an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Spread {
    PerItem,
    PerDeclaration,
    PerSection,
}

/// One declared attribute with provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAttr {
    pub name: String,
    pub args: Vec<String>,
    pub spread: Spread,
    pub source: Span,
}

/// One declared section with provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSection {
    pub name: String,
    pub generic: Option<String>,
    pub family: SectionFamily,
    pub hierarchy: Hierarchy,
    pub source: Span,
}

/// Type declaration for a kind (payload policy).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenPayload {
    Suite,
    Fields,
    Commands,
}

/// One generic parameter with bound and provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenGeneric {
    pub name: String,
    pub bound: Option<OpenType>,
    pub source: Span,
}

/// Type expression carried by generics and fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenType {
    pub text: String,
    pub source: Span,
}

/// One field in a data section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenField {
    pub name: String,
    pub ty: OpenType,
    pub source: Span,
}

/// The open declaration (core).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenDecl {
    pub name: String,
    pub as_kind: String,
    pub generics: Vec<OpenGeneric>,
    pub attributes: Vec<OpenAttr>,
    pub sections: Vec<OpenSection>,
    /// Extension payloads (section → payload policy).
    pub payloads: BTreeMap<String, OpenPayload>,
    /// Documentation line refs per section.
    pub docs: BTreeMap<String, Vec<String>>,
}

impl OpenDecl {
    /// Collects a bootstrap declaration into `Hir` with provenance.
    #[must_use]
    pub fn from_bootstrap_declaration(decl: &Declaration) -> Self {
        let generics = decl
            .generics
            .iter()
            .map(
                |GenericParam {
                     name,
                     bound,
                     source,
                 }| OpenGeneric {
                    name: name.clone(),
                    bound: bound.as_ref().map(|ty| OpenType {
                        text: "type".into(),
                        source: ty.source,
                    }),
                    source: *source,
                },
            )
            .collect();

        let mut attributes: Vec<OpenAttr> = decl
            .attributes
            .iter()
            .map(|attr| OpenAttr {
                name: attr.name.clone(),
                args: attr.args.clone(),
                spread: attribute_spread(&attr.name),
                source: attr.source,
            })
            .collect();
        attributes.sort_by(|a, b| a.name.cmp(&b.name));
        attributes.dedup_by(|a, b| a.name == b.name && a.source == b.source);

        let sections: Vec<OpenSection> = decl
            .sections_vec()
            .iter()
            .map(|section| OpenSection {
                name: section.name.clone(),
                generic: section.generic.clone(),
                family: family_of(&section.name),
                hierarchy: hierarchy_of(&section.name),
                source: section.source,
            })
            .collect();

        Self {
            name: decl.name.clone(),
            as_kind: decl.as_kind.clone(),
            generics,
            attributes,
            sections,
            payloads: declare_payloads(&decl.sections_vec()),
            docs: collect_docs(&decl.sections_vec()),
        }
    }

    /// Named section.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&OpenSection> {
        self.sections.iter().find(|section| section.name == name)
    }

    /// Section names in declaration order.
    #[must_use]
    pub fn section_names(&self) -> Vec<String> {
        self.sections.iter().map(|s| s.name.clone()).collect()
    }

    /// Deterministic canonical token (schema mutation moves identity).
    #[must_use]
    pub fn canonical(&self) -> String {
        let sections: Vec<String> = self
            .sections
            .iter()
            .map(|s| {
                format!(
                    "{}:{}:{}:{}",
                    s.name,
                    s.family.as_str(),
                    s.hierarchy.as_str(),
                    s.generic.clone().unwrap_or_default()
                )
            })
            .collect();
        let payloads: Vec<String> = self
            .payloads
            .iter()
            .map(|(name, payload)| format!("{name}={}", payload_token(*payload)))
            .collect();
        format!(
            "open-decl:{}:{}:[{}]:[{}]",
            self.name,
            self.as_kind,
            sections.join(";"),
            payloads.join(";")
        )
    }
}

/// `SectionManifest`: kind schema + declared sections, with admission.
#[derive(Clone, Debug)]
pub struct SectionManifest {
    pub schema: KindSchema,
    pub declared: Vec<String>,
}

impl SectionManifest {
    #[must_use]
    pub fn new(schema: KindSchema, declared: Vec<String>) -> Self {
        Self { schema, declared }
    }

    /// Admit declared sections against the schema. Refusals:
    /// unknown (`E-KIND-016`), duplicate (`E-SYN-103`), repeat/payload
    /// policy codes. Ordered refusals; no side effects.
    #[must_use]
    pub fn check(&self, diagnostics: &mut Diagnostics) -> Vec<SectionViolation> {
        let mut violations = Vec::new();
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for (index, name) in self.declared.iter().enumerate() {
            let Some(schema) = self.schema.section(name) else {
                let detail = renamed_goals_hint(name).unwrap_or_else(|| {
                    format!(
                        "section `{name}` is not part of kind `{}`",
                        self.schema.name()
                    )
                });
                let violation = SectionViolation {
                    code: "E-KIND-016",
                    reason: SectionViolationReason::UnknownSection,
                    detail,
                    index,
                };
                diagnostics.error(violation.code, violation.detail.clone(), Span::default());
                violations.push(violation);
                continue;
            };
            if let Some(previous) = seen.get(name).copied() {
                let violation = SectionViolation {
                    code: "E-SYN-103",
                    reason: SectionViolationReason::Duplicate,
                    detail: format!(
                        "duplicate section `{name}` (first at index {previous}, now {index})"
                    ),
                    index,
                };
                diagnostics.error(violation.code, violation.detail.clone(), Span::default());
                violations.push(violation);
            }
            if matches!(
                schema.repeat,
                RepeatPolicy::ExactlyOne | RepeatPolicy::AtMostOne
            ) {
                seen.entry(name.clone()).or_insert(index);
            }
        }
        // Required sections missing.
        for (name, schema) in self.schema.sections() {
            if schema.repeat == RepeatPolicy::ExactlyOne && !seen.contains_key(name) {
                let violation = SectionViolation {
                    code: "E-KIND-011",
                    reason: SectionViolationReason::MissingRequired,
                    detail: format!("kind `{}` requires section `{name}`", self.schema.name()),
                    index: usize::MAX,
                };
                diagnostics.error(violation.code, violation.detail.clone(), Span::default());
                violations.push(violation);
            }
        }
        violations
    }

    /// Whether the manifest admits every declared section (no
    /// unknown/duplicate/missing violations).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let mut diagnostics = Diagnostics::new();
        self.check(&mut diagnostics).is_empty()
    }
}

/// One refusal during section admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionViolation {
    pub code: &'static str,
    pub reason: SectionViolationReason,
    pub detail: String,
    pub index: usize,
}

/// Why a section was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SectionViolationReason {
    UnknownSection,
    Duplicate,
    MissingRequired,
}

/// Notation set name join (mounted by `notation::mount_notation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotationSet {
    pub set: String,
}

impl NotationSet {
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.set
    }
}

/// Known section families.
fn family_of(name: &str) -> SectionFamily {
    match name {
        "inputs" | "outputs" | "state" | "components" | "parameters" | "constants" => {
            SectionFamily::Data
        }
        "definitions" | "equations" | "invariants" | "constraints" => SectionFamily::Behavior,
        "constructors" | "factories" | "delegates" => SectionFamily::Construction,
        "goals" | "exports" | "imports" => SectionFamily::Goals,
        "tests" | "benchmarks" | "certificates" | "examples" => SectionFamily::Evidence,
        "compile" | "policy" | "profiles" => SectionFamily::Config,
        _ => SectionFamily::Extension,
    }
}

fn hierarchy_of(name: &str) -> Hierarchy {
    match name {
        "imports" | "uses" | "aliases" => Hierarchy::Pre,
        "tests" | "benchmarks" | "certificates" | "docs" | "migration" => Hierarchy::Post,
        _ => Hierarchy::Body,
    }
}

fn attribute_spread(name: &str) -> Spread {
    match name {
        "module" | "edition" | "schema" => Spread::PerDeclaration,
        "section" | "family" => Spread::PerSection,
        _ => Spread::PerItem,
    }
}

fn declare_payloads(sections: &[Section]) -> BTreeMap<String, OpenPayload> {
    sections
        .iter()
        .map(|section| {
            let payload = if section.suite.statements.is_empty() {
                OpenPayload::Suite
            } else {
                match section.suite.statements[0].kind {
                    emath_core::tree::StmtKind::FieldDecl { .. } => OpenPayload::Fields,
                    emath_core::tree::StmtKind::Command { .. } => OpenPayload::Commands,
                    _ => OpenPayload::Suite,
                }
            };
            (section.name.clone(), payload)
        })
        .collect()
}

fn collect_docs(sections: &[Section]) -> BTreeMap<String, Vec<String>> {
    let mut docs = BTreeMap::new();
    for section in sections {
        let mut lines = Vec::new();
        for stmt in &section.suite.statements {
            match &stmt.kind {
                emath_core::tree::StmtKind::Command { head, .. }
                    if head.first().is_some_and(|w| w == "doc") =>
                {
                    lines.push(head.iter().skip(1).cloned().collect::<Vec<_>>().join(" "));
                }
                emath_core::tree::StmtKind::Section(inner) if inner.name == "doc" => {
                    for inner_stmt in &inner.suite.statements {
                        if let emath_core::tree::StmtKind::Command { head, .. } = &inner_stmt.kind {
                            lines.push(head.join(" "));
                        }
                    }
                }
                _ => {}
            }
        }
        if !lines.is_empty() {
            docs.insert(section.name.clone(), lines);
        }
    }
    docs
}

/// `request:`/`requests:` are not Goals-family aliases; the migrator
/// rewrites them to `goals:`, and the old spellings refuse with a hint.
fn renamed_goals_hint(name: &str) -> Option<String> {
    matches!(name, "request" | "requests")
        .then(|| format!("section `{name}:` was renamed to `goals:`; use `goals:`"))
}

fn payload_token(payload: OpenPayload) -> &'static str {
    match payload {
        OpenPayload::Suite => "suite",
        OpenPayload::Fields => "fields",
        OpenPayload::Commands => "commands",
    }
}
