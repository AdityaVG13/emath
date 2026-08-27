//! Closed scientific provenance values and the `core::measure` type family.

use crate::{DeclarationId, SchemeBody, SchemeField, TypeExpr, TypeScheme, TypeVar};
use emath_core::QualifiedName;

/// Versioned canonical schema for provenance attached to admitted bindings.
pub const PROVENANCE_SCHEMA_V1: &str = "emath.provenance.v1";

/// Distribution attached to a measured value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DistributionKind {
    /// Normal distribution.
    #[default]
    Normal,
    /// Uniform distribution.
    Uniform,
    /// Empirical distribution represented by external observations.
    Empirical,
}

/// Timestamp retained without inventing calendar or timezone semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timestamp(pub String);

/// Stable reference to an instrument record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstrumentRef(pub String);

/// Closed provenance lattice for scientific values.
///
/// `Assumed` and `Unstated` are deliberately representable so missing
/// authority stays visible rather than being smuggled into prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Exact by a named definition or mathematical identity.
    Exact { source: String },
    /// Published or otherwise externally referenced evidence.
    Citation {
        reference: String,
        adjustment: Option<String>,
    },
    /// Instrument output plus the processing description applied to it.
    InstrumentRun { file: String, processing: String },
    /// Output of a content-addressed fit.
    Fitted { fit_id: String },
    /// Deliberately assumed by the author.
    Assumed { reason: Option<String> },
    /// No provenance was supplied.
    Unstated,
}

impl Provenance {
    /// Stable variant name.
    #[must_use]
    pub const fn variant_name(&self) -> &'static str {
        match self {
            Self::Exact { .. } => "Exact",
            Self::Citation { .. } => "Citation",
            Self::InstrumentRun { .. } => "InstrumentRun",
            Self::Fitted { .. } => "Fitted",
            Self::Assumed { .. } => "Assumed",
            Self::Unstated => "Unstated",
        }
    }

    /// Deterministic, length-framed representation used by package identity.
    #[must_use]
    pub fn canonical(&self) -> String {
        fn field(out: &mut String, name: &str, value: &str) {
            out.push_str(name);
            out.push(':');
            out.push_str(&value.len().to_string());
            out.push(':');
            out.push_str(value);
            out.push('\n');
        }

        let mut out = String::new();
        field(&mut out, "schema", PROVENANCE_SCHEMA_V1);
        field(&mut out, "kind", self.variant_name());
        match self {
            Self::Exact { source } => field(&mut out, "source", source),
            Self::Citation {
                reference,
                adjustment,
            } => {
                field(&mut out, "reference", reference);
                field(
                    &mut out,
                    "adjustment-present",
                    if adjustment.is_some() {
                        "true"
                    } else {
                        "false"
                    },
                );
                if let Some(adjustment) = adjustment {
                    field(&mut out, "adjustment", adjustment);
                }
            }
            Self::InstrumentRun { file, processing } => {
                field(&mut out, "file", file);
                field(&mut out, "processing", processing);
            }
            Self::Fitted { fit_id } => field(&mut out, "fit_id", fit_id),
            Self::Assumed { reason } => {
                field(
                    &mut out,
                    "reason-present",
                    if reason.is_some() { "true" } else { "false" },
                );
                if let Some(reason) = reason {
                    field(&mut out, "reason", reason);
                }
            }
            Self::Unstated => {}
        }
        out
    }

    /// Compact human-facing rendering for `emath explain --provenance`.
    #[must_use]
    pub fn explain(&self) -> String {
        match self {
            Self::Exact { source } => format!("Exact(source={source})"),
            Self::Citation {
                reference,
                adjustment,
            } => adjustment.as_ref().map_or_else(
                || format!("Citation(reference={reference})"),
                |adjustment| format!("Citation(reference={reference}, adjustment={adjustment})"),
            ),
            Self::InstrumentRun { file, processing } => {
                format!("InstrumentRun(file={file}, processing={processing})")
            }
            Self::Fitted { fit_id } => format!("Fitted(fit_id={fit_id})"),
            Self::Assumed { reason } => reason.as_ref().map_or_else(
                || "Assumed".to_string(),
                |reason| format!("Assumed(reason={reason})"),
            ),
            Self::Unstated => "Unstated".to_string(),
        }
    }
}

/// Declaration-local site to which provenance is attached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BindingSite {
    pub declaration: DeclarationId,
    pub binding: String,
}

impl BindingSite {
    #[must_use]
    pub fn new(declaration: DeclarationId, binding: impl Into<String>) -> Self {
        Self {
            declaration,
            binding: binding.into(),
        }
    }
}

/// A value with standard uncertainty and mandatory provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured<T> {
    pub value: T,
    pub std_uncertainty: T,
    pub distribution: DistributionKind,
    pub provenance: Provenance,
    pub timestamp: Option<Timestamp>,
    pub instrument: Option<InstrumentRef>,
}

impl<T> Measured<T> {
    /// Construct a measured value with all semantic fields explicit.
    #[must_use]
    pub fn new(
        value: T,
        std_uncertainty: T,
        distribution: DistributionKind,
        provenance: Provenance,
        timestamp: Option<Timestamp>,
        instrument: Option<InstrumentRef>,
    ) -> Self {
        Self {
            value,
            std_uncertainty,
            distribution,
            provenance,
            timestamp,
            instrument,
        }
    }

    /// Default used by uncertainty literals before explicit provenance is attached.
    #[must_use]
    pub fn unstated(value: T, std_uncertainty: T) -> Self {
        Self::new(
            value,
            std_uncertainty,
            DistributionKind::Normal,
            Provenance::Unstated,
            None,
            None,
        )
    }
}

fn named(name: &str) -> TypeExpr {
    TypeExpr::Con(QualifiedName(name.to_string()), Vec::new())
}

fn optional(inner: TypeExpr) -> TypeExpr {
    TypeExpr::Con(QualifiedName("Option".into()), vec![inner])
}

/// Data-driven `core::measure` schemes. These are ordinary stdlib type
/// descriptions, not parser or compiler builtins.
#[must_use]
pub fn core_measure_schemes() -> Vec<TypeScheme> {
    let measured = TypeScheme {
        name: QualifiedName("core::measure::Measured".into()),
        generics: vec!["T".into()],
        body: SchemeBody::Record(vec![
            SchemeField {
                name: "value".into(),
                ty: TypeExpr::Var(TypeVar("T".into())),
            },
            SchemeField {
                name: "std_uncertainty".into(),
                ty: TypeExpr::Var(TypeVar("T".into())),
            },
            SchemeField {
                name: "distribution".into(),
                ty: named("core::measure::DistributionKind"),
            },
            SchemeField {
                name: "provenance".into(),
                ty: named("core::measure::Provenance"),
            },
            SchemeField {
                name: "timestamp".into(),
                ty: optional(named("core::measure::Timestamp")),
            },
            SchemeField {
                name: "instrument".into(),
                ty: optional(named("core::measure::InstrumentRef")),
            },
        ]),
    };
    let provenance = TypeScheme {
        name: QualifiedName("core::measure::Provenance".into()),
        generics: Vec::new(),
        body: SchemeBody::Variant(vec![
            (
                "Exact".into(),
                vec![SchemeField {
                    name: "source".into(),
                    ty: named("Text"),
                }],
            ),
            (
                "Citation".into(),
                vec![
                    SchemeField {
                        name: "reference".into(),
                        ty: named("DoiOrUri"),
                    },
                    SchemeField {
                        name: "adjustment".into(),
                        ty: optional(named("Text")),
                    },
                ],
            ),
            (
                "InstrumentRun".into(),
                vec![
                    SchemeField {
                        name: "file".into(),
                        ty: named("Hash"),
                    },
                    SchemeField {
                        name: "processing".into(),
                        ty: named("Text"),
                    },
                ],
            ),
            (
                "Fitted".into(),
                vec![SchemeField {
                    name: "fit_id".into(),
                    ty: named("Hash"),
                }],
            ),
            (
                "Assumed".into(),
                vec![SchemeField {
                    name: "reason".into(),
                    ty: optional(named("Text")),
                }],
            ),
            ("Unstated".into(), Vec::new()),
        ]),
    };
    vec![measured, provenance]
}
